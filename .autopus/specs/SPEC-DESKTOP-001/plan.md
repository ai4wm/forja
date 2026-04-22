# SPEC-DESKTOP-001 Plan: Expand Desktop Dashboard UI

## File Impact

- `src/dashboard/routes.rs` or split route modules
- `src/dashboard/static/index.html`
- `src/runtime/boot_dashboard.rs`
- `src/runtime/boot_channel.rs`
- `crates/forja-channel/src/multi.rs`
- `crates/forja-core/src/traits.rs`
- `crates/forja-core/src/engine/streaming.rs`

## Strategy

1. Add a lightweight UI bridge in the channel layer for dashboard-originated turns.
2. Expose chat submission and SSE endpoints from the dashboard backend.
3. Expand memory APIs from counts to browsable entries and summaries.
4. Refresh the dashboard UI around chat, memory, and tool activity without breaking existing tabs.

## Risks

- Streaming output must not regress CLI streaming behavior.
- Dashboard routes are already large and should be split if they grow further.
- The UI bridge must remain optional so non-dashboard runs still work.
