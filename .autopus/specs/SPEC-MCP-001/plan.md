# SPEC-MCP-001 Plan

## File Impact

- `crates/forja-tools/Cargo.toml`
- `crates/forja-tools/src/lib.rs`
- `crates/forja-tools/src/mcp/**`
- `crates/forja-tools/src/bin/forja-mcp.rs` or equivalent

## Strategy

1. Add a small MCP runtime with stdio framing and JSON-RPC message handling.
2. Build a tool registry from existing `Tool` implementations in `forja-tools`.
3. Expose `initialize`, `tools/list`, and `tools/call`.
4. Keep runtime dependencies local to `forja-tools` and avoid coupling to the full Forja binary.

## Scope Notes

- Focus on tool listing and execution, not prompts/resources/sampling.
- Prefer self-contained tools that do not require the full Forja runtime to boot.
