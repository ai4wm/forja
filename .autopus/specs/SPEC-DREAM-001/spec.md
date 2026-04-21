# SPEC-DREAM-001: Forja Dream System Phase 4: Background Memory Consolidation During Idle

---
id: SPEC-DREAM-001
title: Forja Dream System Phase 4: Background Memory Consolidation During Idle
version: 0.1.0
status: completed
priority: HIGH
---

## Purpose

Add a non-blocking, rule-based dream maintenance system that consolidates structured memory in the background during idle periods and at graceful shutdown, without any LLM calls.

## Background

`SPEC-MEMORY-001` already moved Forja memory into `index.md`, `topics/`, `daily/`, and `archive/`.
The current implementation in `crates/forja-memory/src/storage/mod.rs` provides:

- append-time writes to daily files and topic files
- index rebuilds from topic files
- bounded startup loading of recent daily files
- bounded query-time topic loading

Relevant current facts:

- `Storage::append_entry()` immediately writes daily logs, updates topics, and rebuilds the index, but it has no offline consolidation phase.
- `Storage` already resolves `index.md`, `topics/`, `daily/`, and `archive/` relative to its `base_dir`, so dream paths should follow the same base-dir-relative pattern.
- `Storage` already owns `archive/`, so archive-first dream behavior can stay inside `forja-memory`.
- `Storage::ensure_layout()` does not currently create a `dreams/` directory.
- topic sizing is currently character-based (`TOPIC_CHAR_BUDGET`), not token-based.
- `MemoryStore` in `crates/forja-core/src/traits.rs` has no dream-specific API.
- `Engine` in `crates/forja-core/src/engine.rs` has no idle tracker, no dream worker handle, and no background task registry beyond heartbeat/autonomy.
- `HeartbeatScheduler` emits generic periodic heartbeat envelopes, but it does not represent user-idle state.
- the slash layer in `src/runtime/slash.rs` supports `/task`, `/autonomy`, `/dashboard`, `/skills`, and `/unresolved`, but not `/dream`.
- `Channel::send_notification()` and `AutonomousLoop::append_notification_log()` already provide a path for Telegram-capable and file-log notifications.

Because dream must stay heuristic-only in Phase 4, it needs a bounded snapshot, explicit conflict rules, deterministic topic-merging logic, and a commit protocol that does not race unsafely with normal turn-time memory writes.

## Requirements

### Ubiquitous

The system shall support a dream maintenance pass over the structured memory layout introduced by `SPEC-MEMORY-001`.

The system shall treat dream analysis as a background async worker that operates on an immutable memory snapshot and does not mutate the active conversation buffer, prompt context, or streaming path.

The system shall bound dream inputs to:

- `index.md`
- all topic files under `topics/`
- the most recent seven daily files under `daily/`

The system shall keep dream execution LLM-free and use deterministic local rules only.

The system shall resolve all dream files relative to the existing memory storage `base_dir` established by `Storage::init()`, not by hardcoded `~/.forja/memory/*` paths.

The system shall preserve data by archiving superseded or stale topic files under the storage-relative `archive/` directory rather than deleting them.

The system shall keep an append-only dream history under the storage-relative path `dreams/YYYY-MM-DD.md`.

The system shall ensure the dream worker cannot rewrite or truncate existing dream-log history.

The system shall extend storage layout initialization so `dreams/` is created and tracked alongside `topics/`, `daily/`, and `archive/`.

The system shall update `index.md` atomically by writing a temp file and renaming it into place.

The system shall prevent more than one dream from running concurrently for the same memory store.

The system shall keep foreground turn processing available while a dream is running.

### Event-Driven

WHEN the engine has been idle for the configured duration THEN the system shall start a dream pass in the background.

WHEN the engine is shutting down gracefully AND the last completed dream is older than one hour THEN the system shall run one best-effort shutdown dream before shutdown completes.

WHEN the user issues `/dream` THEN the system shall trigger a dream immediately unless another dream is already running.

WHEN a dream starts THEN the system shall read `index.md`, all topic files, and the most recent seven daily files into an immutable snapshot before rule evaluation begins.

WHEN the dream snapshot identifies duplicate topics by exact slug match or by normalized slug-token overlap greater than 80 percent THEN the system shall merge them into one canonical active topic and archive the superseded files.

WHEN a topic has not been referenced for 30 or more days THEN the system shall archive it as stale instead of keeping it in the active topic set.

WHEN a topic exceeds the 2K-token dream budget THEN the system shall split it into bounded shards and update the index to reflect the new active shard set.

WHEN dream finishes successfully THEN the system shall append a human-readable summary to the dream log and also append a concise summary to `autonomy.log`.

WHEN dream finishes successfully AND Telegram notifications are available through the current channel THEN the system shall send a brief completion summary through `send_notification()`.

### Unwanted

IF the engine becomes active again while a dream is still analyzing a snapshot THEN the dream shall continue in the background without mutating the active turn context.

IF foreground memory writes modify files that a dream plans to rewrite before the dream commit begins THEN the system shall compare current file `mtime` values against the dream's pre-processing `mtime` snapshot and abort the commit when any relevant file changed, leaving the next dream cycle to retry safely.

IF the dream worker fails after staging outputs but before commit finishes THEN the next dream shall detect the leftover state and recover from a consistent checkpoint or restart without requiring manual cleanup.

IF duplicate-topic detection is ambiguous THEN the system shall keep both topics active and log the ambiguity rather than merging aggressively.

IF stale-topic pruning matches a file that still has recent evidence in the last seven daily logs THEN the system shall keep it active.

IF a manual `/dream` request arrives while another dream is running THEN the system shall respond that a dream is already in progress and shall not spawn a second worker.

IF Telegram is unavailable THEN dream completion shall still be recorded in `autonomy.log`.

### Optional

WHERE configuration enables dream mode the system shall expose idle-threshold and shutdown-threshold settings through the runtime config model.

WHERE deterministic token counting is available from existing workspace dependencies the system shall use shared token counting utilities instead of introducing a new external dependency.

## Acceptance Criteria

- [x] The runtime can trigger a background dream after the configured idle duration without blocking normal turn handling.
- [x] `/dream` exists and starts a dream immediately when no dream is already active.
- [x] A shutdown-triggered dream runs only when the most recent completed dream is older than one hour.
- [x] Dream input is bounded to `index.md`, all topics, and recent seven daily files.
- [x] Dream uses storage-base-relative `archive/` and `dreams/` paths rather than hardcoded home-directory paths.
- [x] Storage layout initialization creates `dreams/` alongside the existing memory directories.
- [x] Duplicate topics can be merged only by exact slug match or normalized slug-token overlap greater than 80 percent, and archived rather than deleted.
- [x] Oversized topics can be split using a 2K-token budget.
- [x] Dream writes append-only history under `dreams/YYYY-MM-DD.md`.
- [x] `mtime` snapshot checks abort dream commit when relevant inputs changed during processing.
- [x] Dream completion is summarized in `autonomy.log` and optionally in Telegram notifications.
- [x] Interrupted dream state can be recovered safely on the next run.
- [x] `index.md` is rewritten atomically.
- [x] No new external dependency is required and no LLM call is made.

## Out of Scope

- dream dashboard pages or live dream status UI
- embeddings, vector search, or semantic clustering
- contradiction removal or reconciliation based on natural-language semantics
- vague-to-fact promotion based on natural-language semantics
- rewriting the existing append-time memory ingestion pipeline
- any LLM-assisted dream refinement planned for Phase 5

## Traceability

| Requirement | Test | Status |
|-------------|------|--------|
| Idle-triggered dream starts in background | `tests/phase20_dream.rs` idle-trigger integration test | Covered |
| Manual `/dream` starts or deduplicates correctly | `tests/phase20_dream.rs`, `src/runtime/slash/tests.rs` | Covered |
| Shutdown dream runs when overdue | `tests/phase20_dream.rs` shutdown-trigger test | Covered |
| Storage-relative dream paths and `dreams/` layout are used | `tests/phase20_dream.rs` storage layout tests | Covered |
| Dream input is bounded to 7 recent daily files | `crates/forja-memory/src/storage/dream.rs` bounded snapshot implementation | Implemented |
| Duplicate merge archives superseded topics using exact slug or >80% slug-token overlap | `tests/phase20_dream.rs` duplicate merge test | Covered |
| Oversized topic split respects 2K-token budget | `tests/phase20_dream.rs` split test | Covered |
| `mtime` change detection aborts conflicting dream commits | `crates/forja-memory/src/storage/dream.rs` commit guard | Implemented |
| Dream log is append-only | `tests/phase20_dream.rs` append-only log test | Covered |
| Interrupted dream recovers safely | `crates/forja-memory/src/storage/journal.rs`, `crates/forja-memory/src/storage/dream.rs` | Implemented |
| Completion notifications reach autonomy.log and Telegram path | `crates/forja-core/src/engine/dream.rs` notification fan-out | Implemented |
