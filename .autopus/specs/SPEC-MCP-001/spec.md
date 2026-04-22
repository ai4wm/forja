# SPEC-MCP-001: Add MCP Server to forja-tools

---
id: SPEC-MCP-001
title: Add MCP Server to forja-tools
version: 0.1.0
status: completed
priority: HIGH
---

## Purpose

Expose Forja tools through a Model Context Protocol server so external agents can list and call tools over stdio.

## Requirements

### Ubiquitous

The system shall provide an MCP server implementation inside `forja-tools`.

The MCP server shall support stdio transport, tool listing, and tool execution against the existing `Tool` trait implementations.

### Event-Driven

WHEN an MCP client initializes over stdio THEN the server shall return protocol and server capability information.

WHEN a client requests `tools/list` THEN the server shall return the currently registered tools and their JSON schemas.

WHEN a client requests `tools/call` THEN the server shall execute the addressed tool with JSON arguments and return structured content.

### Unwanted

IF a requested tool does not exist THEN the server shall return a structured protocol error rather than crashing.

IF tool execution fails THEN the server shall surface the error as MCP-compatible content and keep the server alive for later requests.

## Acceptance Criteria

- [ ] `forja-tools` contains an MCP server implementation.
- [ ] stdio framing is implemented.
- [ ] tool listing returns registered tool definitions.
- [ ] tool execution works for at least the core self-contained tools.
- [ ] unit tests cover request parsing and call dispatch.
