# SPEC-CREATION-001 Plan: Forja Creation Engine v0.2.0 Expansion

## Implementation Strategy

Treat this as an incremental expansion of the existing creation subsystem, not a rewrite.

1. First stabilize the current `creation` module boundaries by splitting oversized files before adding new stages.
2. Add explicit stage types and prompts for combination and mutation.
3. Add creation-runtime execution context so `DebateEngine` can use RALF and budget services without depending on `Engine` state directly.
4. Keep `/debate` as the sole runtime entrypoint.
5. Preserve the current 5-agent default team and task-list output while broadening stage coverage.
6. Keep provider-output trust boundaries explicit by validating parsed task fields, bounding reused transcript text, and documenting redaction expectations for audit logging.
7. Keep the existing `Result<DebateResult>` API surface and express bounded failure behavior through redacted and truncated `Err(...)` results rather than a new envelope type.

## File Impact Analysis

| File | Action | Notes |
|------|--------|-------|
| `crates/forja-core/src/creation/mod.rs` | Modify | Export new stage helpers and runtime context types. |
| `crates/forja-core/src/creation/types.rs` | Modify | Extend `DebatePhase` and possibly enrich `TaskItem` or result metadata. |
| `crates/forja-core/src/creation/debate.rs` | Split/Modify | Current file is already large; move prompt/build/execution helpers out before adding more stages. |
| `crates/forja-core/src/creation/agents.rs` | Modify | Keep the 5-agent baseline and optionally add complexity-based team sizing helpers. |
| `[NEW] crates/forja-core/src/creation/combination.rs` | Create | TRIZ/SCAMPER prompt builders and round execution helpers. |
| `[NEW] crates/forja-core/src/creation/mutation.rs` | Create | Inversion/amplification/reduction/elimination/failure-to-advantage prompt builders and round execution helpers. |
| `[NEW] crates/forja-core/src/creation/execution.rs` | Create | Shared call path that applies RALF, timeout handling, budget checks, audit logging, and callback fan-out. |
| `crates/forja-core/src/creation/tests.rs` | Modify | Expand unit coverage for the new stages and retry/budget hooks. |
| `crates/forja-core/src/engine/creation.rs` | Modify | Inject runtime services into the creation pipeline and preserve `/debate` output behavior. |
| `crates/forja-core/src/engine/budget.rs` | Possibly modify | Reuse helpers if the current API needs stable scoped creation-agent ids such as `creation/{agent-id}`. |
| `crates/forja-core/src/creation/debate.rs` or creation audit helper | Possibly modify | Bound or redact transcript logging if the current raw audit payload remains too open-ended. |
| `src/config.rs` | Modify | Add optional round counts or sizing settings for combination/mutation and team sizing. |
| `src/runtime/boot_engine.rs` | Modify | Wire new creation config into the engine bundle. |
| `src/runtime/slash.rs` | Minimal or no change | `/debate` remains the primary surface. |
| `[NEW] tests/phase28_creation.rs` | Create | Runtime-level regression for `/debate` with the expanded pipeline. |

## Architecture Considerations

- `forja-core/src/creation/` should own stage orchestration and prompt structure.
- `engine/creation.rs` should remain the runtime adapter that provides channel output, current provider, audit logger, and policy services.
- RALF and budget integration should be passed into creation through explicit context values, not through hidden globals.
- Creation must continue to operate inside the main runtime loop and must not require a second service boundary.

## Data Model Notes

Recommended additions:

- `[NEW] DebatePhase::{Combination, Mutation}`
- `[NEW] CreationRunContext`
- `[NEW] CreationPolicyContext`
- `[NEW] TaskParseOutcome` or a similar helper for resilient final parsing
- `[NEW] CreationAgentBudgetScope` or equivalent stable scoped id strategy such as `creation/{agent-id}` for inner creation-agent registration

Potential config additions:

- `[NEW] combination_rounds`
- `[NEW] mutation_rounds`
- `[NEW] min_agents`
- `[NEW] auto_team_sizing`

## RED / GREEN / REFACTOR

1. RED
- Add unit tests that fail because combination and mutation phases do not exist yet.
- Add tests for per-call RALF retry usage and per-agent budget enforcement.
- Add a runtime-facing `/debate` regression test that expects the expanded stage transcript.

2. GREEN
- Split the existing `debate.rs` helpers into smaller stage/execution modules.
- Add the new stage types and execution flow.
- Integrate `ralf_execute()` into each creation-agent call.
- Integrate budget checks and usage recording per creation agent through stable scoped creation-agent ids registered lazily against the existing budget manager and reused across runs instead of requiring unregister support.
- Harden final task-list parsing and fallback behavior.
- Bound or redact transcript and failure audit logging according to the documented retention rules.
- Keep exhausted creation failures on the current `Result<DebateResult>` surface by returning bounded, redacted `Err(...)` values.

3. REFACTOR
- Remove duplicated prompt-building code across stages.
- Normalize transcript logging and callback fan-out.
- Keep each new module below the repository’s preferred size limits when practical.

## Risks & Mitigations

| Risk | Impact | Mitigation |
|------|--------|------------|
| `debate.rs` becomes larger and harder to change safely | High | Split it before adding stages. |
| Budget logic becomes duplicated between engine and creation | High | Pass budget services through a scoped runtime context instead of copying logic. |
| RALF retries obscure per-stage failure visibility | Medium | Keep audit events tied to stage, round, and agent id. |
| Task-list parsing regresses existing `/debate` behavior | Medium | Preserve the current format while adding fallback parsing tests. |
| Added stages inflate cost too aggressively | Medium | Keep stage counts configurable and preserve budget enforcement. |
| Provider output leaks into executable metadata or later prompts unsafely | High | Treat provider text as untrusted, allowlist parsed fields, and bound reused transcript context. |
| Expanded transcript logging increases sensitive data retention | Medium | Define redaction and truncation rules before widening audit coverage. |
| Stable scoped creation-agent ids accumulate budget unexpectedly across runs | Medium | Reuse the existing monthly model explicitly and test registration plus usage semantics against the current `BudgetManager` contract. |

## Dependencies

- Existing baseline creation engine in `crates/forja-core/src/creation/`
- Existing `ralf_execute()` in `crates/forja-core/src/ralf/executor.rs`
- Existing budget manager and engine budget helpers
- Existing audit logger and `/debate` runtime wiring
- Existing outer runtime agent registration model in `src/runtime/boot_engine.rs` and `crates/forja-core/src/engine/budget.rs`, which must be extended rather than bypassed

## Exit Criteria

- [x] Combination and mutation are first-class creation stages.
- [x] RALF is integrated per creation-agent call.
- [x] Budget checks and usage recording are integrated per creation-agent call.
- [x] `/debate` still produces a summary and executable `TaskItem` list.
- [x] Existing creation tests and new expanded-stage tests pass.
