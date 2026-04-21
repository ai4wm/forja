# PRD: Forja Dream System Phase 4: Background Memory Consolidation During Idle

## Meta

- SPEC ID: `SPEC-DREAM-001`
- Mode: `standard`
- Status: `draft`
- Date: `2026-04-16`
- Source: `/auto plan "Forja Dream System: Background memory consolidation during idle"`
- Review gate: pending because this planning turn was executed in `--solo` mode

## Problem

`SPEC-MEMORY-001` gave Forja a structured memory layout with `index.md`, `topics/`, `daily/`, and `archive/`.
That layout is durable and query-aware, but it still behaves like a write-through log:

- topics keep growing through append-only updates
- duplicate or near-duplicate topic files can accumulate
- contradictions remain unresolved unless the user revisits them explicitly
- stale topics are never archived automatically
- index maintenance only reflects append-time summaries, not deeper consolidation

The current engine also has no dedicated idle-maintenance path.
`forja-core` can react to heartbeat ticks and autonomy queue work, but it does not track user-idle windows, background memory maintenance jobs, or graceful-shutdown consolidation work.

## Goals

- Add a rule-based "dream" maintenance pass that consolidates structured memory during idle periods.
- Keep dream execution isolated from the active conversation and main engine context.
- Support automatic idle-triggered dreams, manual `/dream`, and a graceful-shutdown fallback trigger.
- Consolidate memory without any LLM calls in Phase 4.
- Preserve data by archiving instead of deleting and by keeping an append-only dream history.
- Rebuild `index.md` atomically after dream changes.
- Emit concise completion notifications through existing autonomy and Telegram-capable channels.

## Non-Goals

- No LLM-assisted summarization, clustering, or semantic contradiction solving in this phase.
- No edits to the live conversation history buffer or active prompt context.
- No dashboard UI for dream status in this phase.
- No attempt to fully normalize every possible natural-language contradiction.
- No replacement of the existing turn-time append pipeline from `SPEC-MEMORY-001`.

## User Value

The user should experience:

- cleaner long-term memory without manual maintenance
- fewer duplicated topic files and stale fragments
- more accurate durable facts when newer evidence exists
- zero interruption to the foreground engine while dream work runs in the background
- a visible audit trail of what the dream changed and why

## Core User Flows

### 1. Idle-triggered dream

When the user has been inactive for the configured idle threshold, the engine launches an isolated dream worker.
The worker reads a bounded memory snapshot, produces a rule-based consolidation plan, applies safe filesystem updates, writes a dream log entry, and posts a short completion summary.

### 2. Manual `/dream`

When the user enters `/dream`, the engine starts a dream immediately if no dream is already running.
The command returns quickly with a start/skip message rather than blocking until consolidation finishes.

### 3. Shutdown fallback dream

When the user exits gracefully and the last completed dream is older than one hour, the engine performs one final best-effort dream pass before shutdown completes.

### 4. Recovery after interrupted dream

If a dream fails after staging work but before all outputs are committed, the next dream detects the incomplete state, repairs or reuses the staging metadata, and continues from a consistent on-disk state.

## Constraints

- Primary implementation targets are `forja-memory` for dream logic and `forja-core` for runtime triggering.
- `SPEC-MEMORY-001` is a prerequisite and remains the source-of-truth layout.
- No new external dependencies may be added for Phase 4.
- No LLM calls are allowed during the dream pipeline.
- Dream execution must not block the main engine loop.
- Dream must never delete user data; stale or superseded data must be archived.
- The dream worker must read only `index.md`, all topic files, and the most recent seven daily files.
- `index.md` updates must use a temp-file + rename pattern.

## Product Decisions

- "Dream subagent" is implemented as an isolated async background worker, not as an LLM-based agent.
- The dream worker analyzes an immutable snapshot first and applies changes only through a controlled storage commit path.
- Duplicate-topic merging is conservative and rule-based, using slug similarity, keyword overlap, and timestamp evidence.
- Contradiction resolution is conservative and timestamp-driven: newer daily evidence wins when the conflict can be recognized deterministically.
- Stale topics are archived after 30+ days of inactivity rather than deleted.
- Oversized topics are split into narrower sub-topics when deterministic keyword grouping is possible; otherwise they fall back to bounded shards.
- Dream history is append-only under `~/.forja/memory/dreams/YYYY-MM-DD.md`.
- Notification fan-out reuses existing `autonomy.log` and channel notification infrastructure.

## Success Criteria

- Dream triggers automatically after the configured idle threshold and on shutdown when overdue.
- Manual `/dream` works without blocking the foreground engine loop.
- Duplicate topics, stale topics, contradictions, and oversized topics are handled by deterministic rules only.
- Dream writes an append-only daily log under `dreams/`.
- Interrupted dreams recover safely on the next attempt.
- `index.md` remains consistent because dream writes it atomically.
- Dream completion produces a summary in `autonomy.log` and, when available, a brief Telegram notification.
