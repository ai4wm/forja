# SPEC-CREATION-001: Forja Creation Engine v0.2.0 Expansion

---
id: SPEC-CREATION-001
title: Forja Creation Engine v0.2.0 Expansion
version: 0.1.0
status: completed
priority: Must
---

## Purpose

Expand the existing `forja-core` creation engine so it matches the v0.2.0 architecture more closely by adding explicit combination and mutation stages and by wiring RALF and budget management into real creation-agent execution.

## Background

The current codebase already contains:

- `crates/forja-core/src/creation/debate.rs` with divergence, conflict, and convergence
- `crates/forja-core/src/creation/agents.rs` with a 5-agent default team
- `crates/forja-core/src/engine/creation.rs` with `/debate` runtime wiring
- `crates/forja-core/src/ralf/executor.rs` for retry orchestration
- `crates/forja-core/src/engine/budget.rs` and `budget/manager.rs` for budget enforcement

However, the creation layer still lacks:

- combination stage prompts and transcript structure
- mutation stage prompts and transcript structure
- per-round RALF wrapping around creation-agent calls
- per-agent budget checks and usage recording inside creation execution
- resilient task-list extraction beyond the current simple converged-text parser

## Requirements

### Ubiquitous

The system shall preserve the existing 5-agent debate baseline and extend it rather than replacing it with a separate creation subsystem.

The system shall represent creation runs as a staged pipeline that includes divergence, conflict, combination, mutation, and convergence.

The system shall keep all creation execution inside the existing single-binary runtime.

The system shall log creation transcript messages and failure events through the existing audit logger.

The system shall use the existing RALF retry policy for individual creation-agent model calls.

The system shall use the existing budget-management system for creation-agent calls rather than introducing a second budget policy path.

The system shall continue to convert the converged creation output into executable `TaskItem` values.

The system shall preserve the existing `/debate <topic>` entrypoint as the primary user-facing surface for creation runs.

The system shall keep the creation engine inside the existing single-binary runtime and shall not require a second daemon, worker process, or server split.

The system shall reuse the existing budget policy path by registering or mapping inner creation-agent execution to scoped agent ids under the existing budget manager rather than introducing a second budget store.

The system shall treat all provider text as untrusted input even when that text is reused in later-stage prompts or parsed into `TaskItem` fields.

The system shall parse provider-generated task lines through bounded parsing and allowed-field validation, and shall never execute raw provider text directly.

The system shall record creation transcript and failure audit data with bounded retention expectations and redaction-safe logging rules that avoid expanding raw sensitive content storage unnecessarily.

The system shall preserve the existing `Result<DebateResult>` creation API surface for this phase rather than introducing a second success-or-failure envelope type.

The system shall map per-creation-agent budget usage onto stable scoped ids such as `creation/{agent-id}` within the existing monthly budget manager, registering missing ids lazily and reusing the same ids across runs.

The system shall truncate audit-logged creation transcript or failure text to at most 512 UTF-8 characters per stored message and shall mask recognized credentials before persistence.

The system shall not introduce any new long-lived persistence target for raw creation transcripts beyond the existing `audit.db`.

### Event-Driven

WHEN the user issues `/debate <topic>` THEN the system shall run divergence, conflict, combination, mutation, and convergence in sequence.

WHEN a creation-agent model call starts THEN the system shall evaluate the relevant budget policy before the call is executed.

WHEN a creation-agent model call succeeds THEN the system shall record budget usage for that specific creation agent.

WHEN a transient model-call failure occurs during a creation stage THEN the system shall retry the call through the existing RALF executor before surfacing a final failure.

WHEN a creation stage emits a message THEN the system shall append a transcript entry with the correct stage label and round number.

WHEN convergence completes THEN the system shall parse the output into a summary plus an executable task list.

WHEN the converged output contains malformed task lines THEN the system shall keep the summary and salvage any valid task items that can still be parsed safely.

WHEN the creation engine reuses prior provider output in a later-stage prompt THEN the system shall wrap that reused content in bounded, stage-scoped prompt context of at most 2,000 characters per prior-stage aggregate rather than treating it as executable instructions.

### Unwanted

IF a creation-agent model call exceeds budget in enforce mode THEN the system shall stop the creation run with a clear budget error rather than silently continuing.

IF a creation-agent model call exceeds budget in monitor mode THEN the system shall log the exceeded-budget event and continue the run.

IF a creation-agent call fails after exhausting RALF retries THEN the system shall record the failure in the audit log and return a bounded `Err(...)` result instead of panicking, where the surfaced error text is redacted and truncated to at most 512 UTF-8 characters.

IF one stage produces no usable text THEN the following stage shall still receive a bounded placeholder context rather than an empty transcript that breaks prompt construction.

IF task-list extraction yields zero valid tasks in convergence THEN the system shall return the summary and an explicit structured fallback task item rather than an empty execution handoff.

IF provider output contains malformed, oversized, or unexpected task fields THEN the system shall reject those fields, keep only allowed parsed values, and avoid promoting the raw text into executable task metadata.

IF creation transcript or failure text contains secrets, tokens, or unrelated user-sensitive content THEN the system shall log a redacted or bounded form rather than expanding raw long-form content retention.

### Optional

WHERE configuration enables complexity-based team sizing THEN the system shall reduce the active creation team from 5 agents to 3 or 4 for simpler prompts while preserving the same stage pipeline.

WHERE configuration customizes stage counts THEN the system shall honor those counts for divergence, conflict, combination, mutation, and convergence rounds.

### Complex

WHILE the creation engine is in the combination stage the system shall force cross-domain fusion prompts using explicit patterns such as TRIZ or SCAMPER rather than reusing conflict-stage prompts.

WHILE the creation engine is in the mutation stage the system shall force inversion, amplification, reduction, elimination, or failure-to-advantage transformations rather than reusing divergence-stage prompts.

## Acceptance Criteria

- [x] `/debate` runs the expanded stage pipeline without introducing a second creation entrypoint.
- [x] The transcript includes combination and mutation stage messages in addition to divergence, conflict, and convergence.
- [x] RALF retry behavior is exercised per creation-agent call.
- [x] Budget checks and usage recording are applied per creation-agent call.
- [x] The default 5-agent team still works end to end.
- [x] Complexity-based team sizing and stage-count customization are testable through configuration-facing behavior.
- [x] The final creation result contains a summary and a parsed executable task list.
- [x] Zero-valid-task fallback returns a bounded structured task item instead of an empty execution handoff.
- [x] Provider-generated task text is handled as untrusted input through bounded parsing and allowed-field validation.
- [x] Transcript and failure logging use documented redaction and retention bounds.
- [x] Later-stage prompt reuse is bounded to documented context limits.
- [x] Exhausted creation failures keep the existing `Result<DebateResult>` surface and expose only bounded, redacted error text.
- [x] Existing debate tests remain green and new tests cover the expanded stages and operational hooks.

## Out of Scope

- dashboard visualization changes for the creation pipeline
- autonomous execution of the generated task list
- cross-process or distributed creation workers
- provider-specific prompt tuning beyond what is needed for baseline correctness

## Traceability

| Requirement | Test | Status |
|-------------|------|--------|
| Expanded staged pipeline runs through `/debate` | `tests/phase28_creation.rs`, `crates/forja-core/src/creation/tests.rs` | Covered |
| Combination and mutation stages appear in transcript | `crates/forja-core/src/creation/expanded_tests.rs` | Covered |
| RALF wraps creation-agent calls | `crates/forja-core/src/creation/expanded_tests.rs` | Covered |
| Budget is checked and recorded per creation agent | `crates/forja-core/src/creation/expanded_tests.rs` | Covered |
| Single-binary runtime shape is preserved | `tests/phase28_creation.rs` runtime path | Covered |
| Existing budget policy path is reused | `crates/forja-core/src/creation/expanded_tests.rs`, `policy_tests.rs` | Covered |
| Complexity-based team sizing is honored | `crates/forja-core/src/creation/policy_tests.rs` | Covered |
| Stage-count customization is honored | `crates/forja-core/src/creation/policy_tests.rs` | Covered |
| Zero-valid-task fallback remains bounded | `crates/forja-core/src/creation/expanded_tests.rs` | Covered |
| Provider-generated task parsing is bounded and validated | `crates/forja-core/src/creation/expanded_tests.rs`, `policy_tests.rs` | Covered |
| Transcript/failure logging is redacted and bounded | `crates/forja-core/src/creation/policy_tests.rs` | Covered |
| Later-stage prompt reuse is bounded to 2,000 characters per prior-stage aggregate | `crates/forja-core/src/creation/policy_tests.rs` | Covered |
| Exhausted creation failures remain bounded `Err(...)` results | `crates/forja-core/src/creation/policy_tests.rs` | Covered |
