# PRD: Forja Creation Engine v0.2.0 Expansion

## Meta

- SPEC ID: `SPEC-CREATION-001`
- Mode: `standard`
- Status: `draft`
- Date: `2026-04-22`
- Source: `/auto dev "ARCHITECTURE.md v0.2.0에 명시된 Creation Engine 구현: 5-agent debate (divergence/conflict/convergence), combination engine (TRIZ/SCAMPER), mutation engine, 토론 결과를 실행 가능한 태스크 리스트로 자동 변환. 기존 forja-core/src/creation/ 모듈 활용. RALF loop과 budget management도 실전 연동." --auto`
- Review gate: pending

## Problem

The current `forja-core` creation layer already has a working baseline:

- a 5-agent debate loop
- fixed divergence, conflict, and convergence stages
- synchronous `/debate <topic>` execution
- audit logging for debate messages and timeouts
- a simple parser that converts the final synthesis text into `TaskItem` entries

That baseline is useful, but it does not yet match the target v0.2.0 architecture in `docs/ARCHITECTURE.md`.
The current gaps are structural, not cosmetic:

- there is no combination stage that forces cross-domain fusion using patterns such as TRIZ and SCAMPER
- there is no mutation stage for inversion, amplification, reduction, elimination, and turning failure into advantage
- RALF retry logic is not applied inside debate agent calls
- budget management is only recorded at the end of the overall debate, not enforced or tracked per creation agent and per round
- agent behavior is mostly static per role instead of switching behavior across stages
- task-list conversion is brittle and assumes a single final output format

As a result, the current creation layer is still closer to a structured chat macro than a production-grade creation engine.

## Goals

- Extend the existing `forja-core/src/creation/` module into a fuller v0.2.0 creation engine instead of replacing it.
- Preserve the current 5-agent debate baseline while adding combination and mutation stages.
- Integrate RALF retries into each creation-agent call.
- Integrate budget checking and usage recording at the creation-agent level.
- Keep final output automatically convertible into executable task lists.
- Preserve real-time intermediate output and audit logging.
- Keep the system single-binary and local-first.

## Non-Goals

- No new external orchestration service.
- No separate creation-worker process.
- No dashboard redesign in this phase.
- No multi-provider creation pipeline distinct from the main runtime.
- No separate persistence store for creation transcripts outside the existing audit log.

## User Value

The user should get:

- better idea quality than the current debate-only baseline
- more deliberate synthesis through explicit combination and mutation passes
- more reliable task-list output that is ready for execution planning
- operational safety because creation calls honor retry and budget policies
- no change to the CLI-first workflow surface

## Core User Flows

### 1. Rich creation session

The user runs `/debate <topic>`.
The engine executes divergence, conflict, combination, mutation, and convergence inside the existing creation layer.
The user sees intermediate agent messages in real time and receives a concise final summary plus an executable task list.

### 2. Budget-aware creation

When the runtime is in budget-enforced mode, the creation engine checks and records budget usage for each internal creation agent.
If a creation agent exceeds budget, the user receives a clear error or degraded result according to the current budget mode.

### 3. Retry-aware creation

When an LLM call fails transiently during a creation stage, the creation engine retries using the existing RALF policy before giving up or falling back.

### 4. Execution handoff

The final converged output is parsed into `TaskItem` objects robustly enough to feed later execution planning without manual cleanup in the common case.

## Constraints

- The implementation must reuse and extend the current `forja-core/src/creation/` module.
- `forja-core/src/creation/debate.rs` is already large, so the feature should prefer splitting into smaller modules rather than growing the file further.
- The runtime must remain single-binary.
- Budget and retry behavior must reuse existing `budget` and `ralf` subsystems instead of introducing parallel policy engines.
- Existing `/debate` behavior must remain available throughout the refactor.
- Tests must stay inside unit tests or root phase tests, not under `crates/forja-llm/tests/`.

## Product Decisions

- The v0.2.0 creation engine will be modeled as a staged pipeline rather than a single debate loop with larger prompts.
- Combination and mutation are explicit stages with their own prompts and transcript entries.
- The final synthesis step remains responsible for task-list generation, but upstream stages may contribute normalized task candidates.
- Budget integration is per internal creation agent, not only per outer runtime agent.
- RALF integration wraps each creation-agent model call rather than the entire debate run as one opaque unit.
- The runtime surface remains `/debate`; no new mandatory slash command is introduced in this phase.

## Success Criteria

- A creation run uses the existing 5-agent baseline and adds explicit combination and mutation stages.
- The transcript distinguishes divergence, conflict, combination, mutation, and convergence.
- RALF retry rules are exercised per creation-agent call.
- Budget checks and usage recording occur per creation-agent call.
- Final output still yields a non-empty `TaskItem` list under the common case.
- Existing debate tests remain green, and new tests cover the expanded stages and operational hooks.
