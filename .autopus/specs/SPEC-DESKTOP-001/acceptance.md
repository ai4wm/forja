# SPEC-DESKTOP-001 Acceptance: Expand Desktop Dashboard UI

## Scenarios

### Scenario 1: Desktop chat submits a turn

Given the runtime dashboard is open  
When the user submits a message in the desktop chat panel  
Then the backend shall enqueue it into the engine loop

### Scenario 2: Desktop chat receives streaming output

Given a desktop-originated conversation  
When the engine streams a response  
Then the UI shall receive incremental assistant chunks and a final assistant message

### Scenario 3: Memory browser shows entries

Given `memory.db` contains entries and summaries  
When the user opens the memory browser  
Then the UI shall show browsable memory rows and summaries

### Scenario 4: Tool monitor remains visible

Given the engine executes tools  
When the user inspects the desktop dashboard  
Then recent tool calls shall still be visible from the monitoring surface
