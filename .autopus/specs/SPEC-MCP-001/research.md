# SPEC-MCP-001 Research

## Findings

- `forja-core` already defines `ToolDefinition` and the async `Tool` trait.
- `forja-tools` already contains reusable tool implementations with JSON-schema-like definitions.
- Runtime tool registration currently lives in `src/runtime/tools.rs`, so the MCP server needs its own lightweight registry path inside `forja-tools`.

## Recommended Shape

- Add an internal MCP module in `forja-tools`.
- Register a pragmatic subset of tools that do not depend on the full runtime state.
- Provide a small binary entrypoint so external agents can launch the MCP server directly.
