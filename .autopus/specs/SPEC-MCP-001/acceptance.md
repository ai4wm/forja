# SPEC-MCP-001 Acceptance

## Scenarios

### Scenario 1: MCP initialize

Given an MCP client connected via stdio  
When it sends `initialize`  
Then the server shall return server info and tool capabilities

### Scenario 2: Tool listing

Given a running MCP server  
When the client sends `tools/list`  
Then the response shall include tool names, descriptions, and input schemas

### Scenario 3: Tool call success

Given a registered tool such as `file_tool` or `web`  
When the client sends `tools/call` with valid arguments  
Then the tool shall execute and the server shall return structured content

### Scenario 4: Tool call failure

Given a missing tool or invalid arguments  
When the client sends `tools/call`  
Then the server shall return a structured error and remain alive
