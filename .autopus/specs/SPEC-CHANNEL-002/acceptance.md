# SPEC-CHANNEL-002 Acceptance: Add Discord Channel Adapter

## Scenarios

### Scenario 1: Allowed Discord source reaches the engine

Given a Discord adapter configured with matching allowlists  
When a Discord user posts a message in an allowed location  
Then the runtime shall receive it as a user `Message`

### Scenario 2: Non-allowed Discord source is denied

Given a Discord adapter configured with allowlists  
When a Discord user posts outside the allowed scope  
Then the runtime shall reject the message and avoid engine delivery

### Scenario 3: Discord replies show typing state

Given an active Discord conversation  
When the engine is generating a response  
Then the adapter shall keep a typing indicator active until completion or cancellation

### Scenario 4: Discord disconnect does not terminate CLI

Given the runtime is also serving CLI  
When Discord disconnects or reconnects  
Then CLI interaction shall remain available
