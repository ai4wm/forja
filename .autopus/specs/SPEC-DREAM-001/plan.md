# SPEC-DREAM-001 Plan: Forja Dream System Phase 4: Background Memory Consolidation During Idle

## Implementation Strategy

Treat Phase 4 as a background-maintenance feature with a clear ownership split:

1. `forja-memory` owns dream snapshot loading, rule-based consolidation, journaling, archive-first commit logic, and append-only dream logging.
2. `forja-core` owns idle detection, manual trigger wiring, shutdown fallback triggering, concurrency guards, and notification fan-out.
3. `src/runtime` only wires config and slash entry points; it must not absorb dream business logic.

Recommended strategy:

1. Extend the memory API with explicit dream operations instead of hiding dream behavior behind `load_all()`.
2. Add a dedicated engine-side dream runtime module instead of growing `crates/forja-core/src/engine.rs`.
3. Build dream in two phases:
   - snapshot + rule evaluation on immutable inputs
   - guarded commit through a storage coordinator with atomic `index.md` replacement
4. Serialize dream commit against normal memory writes and revalidate touched files before commit with `mtime` checks to avoid clobbering fresh turn-time updates.
5. Keep journal and dream history append-only so interrupted work can be resumed or retried safely.

## File Impact Analysis

| File | Action | Notes |
|------|--------|-------|
| `crates/forja-memory/src/lib.rs` | Modify | Expose public dream request/result types and wire new trait-facing APIs. |
| `crates/forja-memory/src/storage/mod.rs` | Modify/Slim | Keep as coordinator; avoid embedding all dream logic into the existing file. |
| `crates/forja-memory/src/storage/dream.rs` | Create | Snapshot loading, duplicate detection, stale pruning, shard splitting, `mtime` validation, and dream-log formatting. |
| `crates/forja-memory/src/storage/journal.rs` | Create | Staging metadata, recovery markers, and atomic commit helpers. |
| `crates/forja-memory/src/storage/classifier.rs` | Modify | Reuse or extend slug/token helpers for deterministic duplicate detection. |
| `crates/forja-core/src/traits.rs` | Modify | Add defaulted dream-capable memory APIs and any dream result/status types needed by the engine. |
| `crates/forja-core/src/engine.rs` | Modify | Register the new engine submodule and store dream runtime state handles. |
| `crates/forja-core/src/engine/dream.rs` | Create | Idle-trigger logic, manual trigger entrypoint, shutdown fallback, and notification fan-out. |
| `src/config.rs` | Modify | Add dream configuration with defaults such as idle threshold and shutdown freshness threshold. |
| `src/runtime/startup.rs` | Modify | Pass dream config into the engine when building the runtime. |
| `src/runtime/slash.rs` | Modify | Add `/dream` routing. |
| `src/runtime/slash/tests.rs` | Modify | Add `/dream` coverage. |
| `tests/phase13g_memory.rs` | Modify | Add filesystem-level dream behavior tests. |
| `tests/phase13f_memory.rs` or new `tests/phase20_dream.rs` | Modify/Create | Add engine/runtime dream trigger and non-blocking behavior tests. |

## Architecture Considerations

Dream should preserve the existing layering:

`runtime -> forja-core engine -> MemoryStore trait -> forja-memory filesystem implementation`

Important decisions:

- `forja-memory` must remain the only layer that rewrites `index.md`, topic files, dream logs, or archive files.
- `forja-core` must remain the only layer that decides when a dream starts and how completion is announced to the user-facing channels.
- Dream must not reuse the autonomy queue as executable work; it is maintenance work local to the engine and memory store.
- `autonomy.log` reuse is acceptable as a notification sink, but dream lifecycle should not be modeled as a queued autonomy task.
- Because current heartbeat scheduling is optional and not tied to user activity, dream should use its own idle tracker and ticker rather than depending entirely on `HeartbeatScheduler`.
- All dream paths must be resolved relative to the storage `base_dir`, matching the current `Storage::init()` layout rules.

## Data Model Notes

Suggested new types:

- `DreamConfig`
- `DreamTrigger` with values such as `Idle`, `Manual`, and `Shutdown`
- `DreamSnapshot`
- `DreamPlan`
- `DreamOutcome`
- `DreamJournal`
- `DreamStatus`

Suggested persistent paths under the storage `base_dir`:

- `dreams/YYYY-MM-DD.md` for append-only history
- `archive/` for stale or superseded topics
- a small journal file such as `dreams/pending.json` or `dream-state.json` for interrupted-commit recovery

## Tasks

- [x] Define config defaults for dream enablement, idle threshold, and shutdown freshness threshold.
- [x] Add dream-oriented methods and result types to `MemoryStore` with safe default no-op behavior.
- [x] Extend storage layout initialization so `dreams/` is created and tracked alongside `topics/`, `daily/`, and `archive/`.
- [x] Add a dedicated engine dream runtime module.
- [x] Track last user activity and idle eligibility independently of optional heartbeat ticks.
- [x] Add single-flight guarding so only one dream can run at a time.
- [x] Add a manual trigger entrypoint for `/dream`.
- [x] Add a shutdown fallback entrypoint for overdue dream runs.
- [x] Reuse existing notification paths for completion fan-out.
- [x] Build a bounded dream snapshot loader in `forja-memory` for `index.md`, all topics, and recent seven daily files using storage-base-relative paths.
- [x] Add deterministic duplicate-topic detection using exact slug match or normalized slug-token overlap greater than 80 percent.
- [x] Add stale-topic detection based on the latest durable evidence and recent-daily references.
- [x] Add 2K-token topic split behavior using shared token-count helpers or a conservative local wrapper over existing workspace token counting.
- [x] Add append-only dream logging under `dreams/YYYY-MM-DD.md`.
- [x] Capture file `mtime` values before dream analysis and re-check them before commit.
- [x] Add archive-first commit logic and journal-based recovery for interrupted dreams.
- [x] Refactor `index.md` rewriting to use atomic writes for dream-triggered rebuilds and any shared rebuild path touched by dream.
- [x] Add `/dream` slash routing and tests.
- [x] Add regression tests for idle triggering, manual triggering, shutdown triggering, storage-relative paths, duplicate merging, stale pruning, split behavior, append-only dream logs, `mtime` conflict handling, and interrupted recovery.

## Risks & Mitigations

| Risk | Impact | Mitigation |
|------|--------|------------|
| Dream commit overwrites fresh turn-time writes | High | Separate snapshot from commit, serialize commit, and verify touched-file `mtime` values before applying. |
| Idle detection is inaccurate if tied only to optional heartbeat | High | Add a dedicated dream ticker or last-activity check inside the engine runtime instead of relying purely on heartbeat. |
| Topic merge heuristics are too aggressive | High | Keep merge rules conservative: exact slug match or normalized slug-token overlap >80 only, and archive superseded files rather than deleting them. |
| Storage-relative paths drift from current layout rules | Medium | Resolve dream directories through the existing storage `base_dir` and cover custom-base-dir cases with tests. |
| Token-budget splitting adds external dependency pressure | Medium | Reuse existing workspace token-count helpers from `forja-core` instead of introducing a new dependency. |
| Dream log becomes mutable by accident | Medium | Route all dream-log writes through append-only helpers and forbid rewrite paths in the storage API. |
| Shutdown dream delays exit excessively | Medium | Use a bounded best-effort shutdown window and skip duplicate runs if a dream already completed recently. |
| New logic bloats existing large files | High | Create `engine/dream.rs` and `storage/dream.rs`/`journal.rs` early instead of extending current monoliths. |

## Dependencies

- Functional prerequisite: `SPEC-MEMORY-001`
- Core integration points: `MemoryStore`, `Engine`, runtime startup, and slash routing
- Notification reuse: `Channel::send_notification()` and `AutonomousLoop::append_notification_log()`
- No new external dependencies
- Conceptual references only: Claude Code autoDream pattern and LM wiki maintenance workflows

## Exit Criteria

- [x] Dream can run from idle, manual, and shutdown triggers.
- [x] Dream runs in the background without blocking normal turn handling.
- [x] Dream uses only bounded structured-memory inputs and no LLM calls.
- [x] Duplicate merge, stale archiving, oversized-topic splitting, and `mtime` conflict detection all exist in deterministic form.
- [x] Dream history is append-only and recoverable after interruption.
- [x] `index.md` updates are atomic.
- [x] Notifications reach `autonomy.log` and optionally Telegram.
- [x] Build, clippy, and targeted dream/memory tests pass once implementation begins.
