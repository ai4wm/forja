# SPEC-CREATION-001 Research: Forja Creation Engine v0.2.0 Expansion

## Self-Verify Summary

- Applied the spec-quality checklist after the first review failure and revised the draft to use allowed priority values and `[NEW]` markers for proposed files and types.
- Added traceability for single-binary preservation, budget-path reuse, team sizing, stage-count customization, bounded parsing, and zero-valid-task fallback.
- Added explicit trust-boundary, allowed-field validation, and redaction/retention expectations for provider output and audit logging.
- Tightened the plan around per-creation-agent budget mapping so review can judge feasibility against the current `current_agent_id`-based budget path.
- Updated stale evidence in the research notes where the previous `debate.rs` line-count observation was out of date.
- Narrowed the failure contract to the current `Result<DebateResult>` surface and defined bounded failure behavior through redacted, truncated `Err(...)` values instead of inventing a new result envelope.

## Codebase Analysis

The existing creation baseline lives in these files:

- `crates/forja-core/src/creation/mod.rs`
- `crates/forja-core/src/creation/debate.rs`
- `crates/forja-core/src/creation/agents.rs`
- `crates/forja-core/src/creation/types.rs`
- `crates/forja-core/src/engine/creation.rs`
- `crates/forja-core/src/ralf/executor.rs`
- `crates/forja-core/src/engine/budget.rs`
- `src/runtime/boot_engine.rs`
- `src/config.rs`

### Current behavior already present

`crates/forja-core/src/creation/agents.rs`

- defines a default 5-agent team
- already uses role + framework + budget fields

`crates/forja-core/src/creation/debate.rs`

- runs divergence, conflict, and convergence in sequence
- emits transcript entries with phase and round information
- logs debate messages and timeouts to the audit logger
- converts the final synthesis text into `TaskItem` values using a fixed line format
- uses a per-call timeout
- delays between calls after the first provider call

`crates/forja-core/src/engine/creation.rs`

- exposes the current creation engine through `/debate`
- streams intermediate transcript output to the current channel
- records only the aggregate token usage after the full debate result completes

`crates/forja-core/src/ralf/executor.rs`

- already provides retry logic with max retry count and repeated-error cutoffs
- is currently used in the main engine paths but not inside creation-agent calls

`crates/forja-core/src/engine/budget.rs`

- already exposes budget checking and usage recording at the engine level
- is currently scoped to the current outer agent id, not the inner creation-agent participants

### Feasibility note for budget reuse

The cleaner implementation path is to keep one `BudgetManager` instance and introduce stable scoped inner ids such as `creation/{agent-id}`.
That preserves the existing monthly accumulation model in `budget/manager.rs`, avoids depending on an unregister API that does not exist today, and still makes creation-agent usage visible and enforceable inside `engine/creation.rs` or a dedicated creation execution context.

### Gaps relative to `docs/ARCHITECTURE.md`

- no combination stage
- no mutation stage
- no stage-aware behavior switching beyond the current three prompts
- no per-creation-agent budget check or usage recording
- no RALF wrapping around individual creation-agent calls
- no complexity-based 3-5 agent sizing
- final task parsing is still narrow and format-dependent

### File-size and change-risk observations

- `crates/forja-core/src/creation/debate.rs` is currently 323 lines and should not absorb more stage logic directly
- `src/config.rs` is already large, so creation-config additions should stay minimal
- `src/runtime/slash.rs` is already large, which favors keeping `/debate` semantics stable rather than adding new runtime commands

## Existing Tests

`crates/forja-core/src/creation/tests.rs`

- verifies the default 5-agent count
- verifies current diverge/conflict/converge counts
- verifies timeout handling
- verifies the existing delay between provider calls

This is a good baseline, but it does not yet cover:

- additional creation phases
- per-call retry logic
- per-agent budget enforcement
- malformed task parsing fallback

## Architecture Alignment

The v0.2.0 target architecture in `docs/ARCHITECTURE.md` calls for:

- 5-agent debate as the default
- divergence / conflict / convergence
- a combination engine using patterns such as TRIZ and SCAMPER
- a mutation engine
- automatic task-list conversion
- RALF and budget-management alignment with operational runtime policies

The cleanest architecture fit is:

- keep stage orchestration in `creation/`
- keep runtime service injection in `engine/creation.rs`
- keep policy reuse through explicit context values for RALF, budget, and audit

## Trust-Boundary Note

The current creation flow already reuses model output as later-stage prompt input and already parses converged text into `TaskItem`.
That means the implementation must explicitly treat provider text as untrusted:

- bound reused transcript size before injecting it into later prompts
- keep reused prior-stage aggregate context at or below 2,000 characters per stage input
- allowlist parsed fields for `TaskItem`
- reject malformed or oversized fields
- never execute raw provider text directly
- define redaction and truncation expectations for transcript and failure audit logging
- keep surfaced failure text on `Result<DebateResult>` as redacted, truncated `Err(...)` values capped at 512 UTF-8 characters

## Recommendations

- Create new creation submodules rather than growing `debate.rs`.
- Introduce explicit stage variants for combination and mutation.
- Add a scoped execution helper that owns timeout, RALF retry, budget checks, audit logging, and callback fan-out for a single creation-agent call.
- Keep the current final task-line format supported, but add fallback parsing rather than replacing it abruptly.
- Preserve the current 5-agent default team and make 3-5 auto sizing optional within the same SPEC.

## Lore Context

No dedicated lore context was loaded during this planning turn.

## Verification Notes

Planning-only task.
Code build, clippy, and runtime tests were not rerun in this step.
