# SPEC-DREAM-001 Research: Forja Dream System Phase 4: Background Memory Consolidation During Idle

## Codebase Analysis

The current memory and runtime behavior is split across these layers:

- `crates/forja-memory`: structured filesystem memory store
- `crates/forja-core`: engine, slash-command results, heartbeat, autonomy integration, and channel traits
- `src/runtime`: runtime wiring and slash-handler construction

### Relevant current behavior

`crates/forja-memory/src/storage/mod.rs`

- `Storage::init()` already creates `index.md`, `topics/`, `daily/`, and `archive/`
- `append_entry()` writes daily logs, updates topic files, and rebuilds the index immediately
- `read_startup_context()` loads `index.md` plus bounded recent daily logs
- `read_relevant()` searches `index.md` summaries and loads bounded topic shards
- `rebuild_index()` rewrites `index.md`, but not atomically
- topic size limits are currently character-based, not token-based
- there is no dream snapshot, consolidation, journaling, or append-only dream history path

`crates/forja-memory/src/storage/classifier.rs`

- topic classification is still coarse (`people`, `preferences`, `decisions`, `projects`, `workflow`, `general`)
- duplicate-topic matching and semantic split heuristics do not exist yet

`crates/forja-core/src/traits.rs`

- `MemoryStore` exposes `save`, `load_all`, `load_startup_context`, `load_relevant`, and `flush`
- there is no dream API, dream result type, or dream status type
- `Channel::send_notification()` already exists and can be reused for Telegram-capable summaries

`crates/forja-core/src/engine.rs`

- the runtime loop already supports background heartbeat signals
- the engine holds memory as a trait object and can therefore trigger dream through an abstract interface
- there is no idle tracker, no dream worker handle, no dream slash result, and no shutdown-dream hook

`crates/forja-core/src/heartbeat/scheduler.rs`

- heartbeat emits generic internal envelopes on a fixed interval
- it does not track actual last user activity
- it cannot satisfy the dream idle requirement by itself without additional state

`crates/forja-core/src/autonomy/task_store.rs` and `loop_runner.rs`

- `autonomy.log` already exists as an append-only notification sink under `~/.forja/tasks/`
- `AutonomousLoop::append_notification_log()` provides a reusable way to write summaries there

`src/runtime/slash.rs`

- `/dream` does not exist yet
- current slash routing patterns make adding a dedicated command straightforward

### Target files

| File | Role | Dream impact |
|------|------|--------------|
| `crates/forja-memory/src/lib.rs` | public store entrypoint | Yes |
| `crates/forja-memory/src/storage/mod.rs` | memory coordinator | Yes |
| `crates/forja-memory/src/storage/classifier.rs` | slug and keyword helpers | Yes |
| `crates/forja-core/src/traits.rs` | engine-facing memory abstraction | Yes |
| `crates/forja-core/src/engine.rs` | runtime loop and state | Yes |
| `crates/forja-core/src/engine/dream.rs` | new background dream runtime module | New |
| `src/config.rs` | config model and defaults | Yes |
| `src/runtime/startup.rs` | runtime assembly | Yes |
| `src/runtime/slash.rs` | slash routing | Yes |
| `src/runtime/slash/tests.rs` | slash coverage | Yes |
| `tests/phase13g_memory.rs` | filesystem memory coverage | Yes |
| `tests/phase13f_memory.rs` or new phase test | engine/runtime coverage | Yes |

## Dependencies

- `forja-memory` already depends on `forja-core`
- `forja-core` already depends on `tiktoken-rs`
- `forja-memory` currently does not use `tokio::sync`, but it can add the `sync` feature to the existing `tokio` dependency without introducing a new external crate
- no new external dependency is needed for dream planning or token counting if shared helpers are exposed through `forja-core`

## Lore Decisions

No dedicated lore context was loaded in this planning turn.
The review gate configured in `autopus.yaml` remains pending because this request explicitly used `--solo`.

## Architecture Compliance

The feature is architecture-compatible if:

- `forja-memory` owns snapshot loading, consolidation rules, archive decisions, journaling, and atomic commit behavior
- `forja-core` owns idle detection, lifecycle management, and notification fan-out
- `src/runtime` only wires config and slash entrypoints

The strongest internal architectural precedent is `SPEC-MEMORY-001`, which already established the structured file layout and query-aware memory path.

## Key Findings

- The current memory store has no maintenance phase beyond append-time updates and index rebuilds.
- Current `index.md` rewriting is not atomic, so dream needs a stronger commit path than existing `rebuild_index()`.
- There is no user-idle tracker in the engine, so idle-triggered dream must add new runtime state instead of piggybacking only on heartbeat.
- Heartbeat is optional in the current runtime, which makes it insufficient as the sole dream trigger mechanism.
- Foreground turn-time writes and dream-time rewrites will race unless commit is serialized or revalidated.
- `autonomy.log` and `Channel::send_notification()` already provide the required notification paths, so dream does not need a new notification subsystem.
- Current topic classification is intentionally coarse; duplicate merging and semantic splitting must therefore rely on conservative keyword overlap and timestamp evidence.
- `forja-core` already has token-count helpers, so the 2K-token split budget can be implemented without pulling in another dependency.

## Recommendations

- Add explicit dream methods to `MemoryStore` rather than overloading existing read/write APIs.
- Introduce a dedicated `engine/dream.rs` module to keep `engine.rs` from growing further.
- Build dream around immutable snapshots plus guarded commit, not direct in-place mutation during analysis.
- Use append-only journaling and archive-first behavior so interrupted dreams can be resumed or retried safely.
- Keep contradiction resolution conservative and timestamp-based in Phase 4; defer fuzzy semantic rewrites to the LLM-assisted Phase 5.
- Prefer a new top-level dream config section over hiding dream settings inside autonomy settings, because dream is maintenance work rather than queued autonomy work.
- Treat the user-provided external references as inspiration only and keep the implementation grounded in this codebase’s current abstractions.
