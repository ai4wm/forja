# SPEC-DESKTOP-001 Research: Expand Desktop Dashboard UI

## Findings

- The existing dashboard already provides Axum routes for audit, debates, budget, tasks, history, tools, memory counts, and SSE polling.
- The current UI is a single embedded HTML file and already includes a Tools tab and event stream view.
- The missing capabilities are live chat submission, incremental streaming output, and deeper memory inspection.
- `Engine::stream_step_with_tools` currently streams only to CLI, so desktop streaming needs a channel-level hook.

## Recommended Shape

- Add a small dashboard bridge in the channel layer rather than wiring HTTP directly into the engine.
- Preserve the existing Axum server and extend it with chat and richer memory endpoints.
- Keep the UI self-contained in the embedded HTML shell for now.
