# SPEC-DREAM-001 Acceptance: Forja Dream System Phase 4: Background Memory Consolidation During Idle

## Test Scenarios

### Scenario 1: Idle-triggered dream starts in the background

Given an engine with dream mode enabled and no user activity for longer than the configured idle threshold  
When the dream trigger checks runtime activity  
Then the engine shall start a dream worker without blocking the foreground loop  
And the next foreground turn shall still be processed normally

### Scenario 2: Manual `/dream` starts a single dream

Given an engine with dream mode enabled and no dream currently running  
When the user issues `/dream`  
Then the slash layer shall return an immediate start acknowledgement  
And the engine shall begin exactly one background dream pass

### Scenario 3: Manual `/dream` is deduplicated while running

Given a dream worker already running  
When the user issues `/dream` again  
Then the engine shall not spawn a second dream worker  
And the user shall receive a reply that dream is already in progress

### Scenario 4: Shutdown dream runs only when overdue

Given the engine is shutting down gracefully  
And the last completed dream is older than one hour  
When shutdown cleanup begins  
Then the engine shall run one best-effort dream before final shutdown completes

### Scenario 5: Dream snapshot uses bounded inputs

Given a memory directory with many topic files and more than seven daily files  
When a dream snapshot is built  
Then the snapshot shall include `index.md`, all topic files, and only the most recent seven daily files  
And older daily files shall be excluded from dream analysis

### Scenario 6: Dream paths stay relative to the storage base directory

Given a memory store initialized from a custom memory base directory  
When dream support is initialized and a dream log is written  
Then the system shall create and use `dreams/` under that same storage base directory  
And archive outputs shall also remain under the same storage-relative `archive/` directory  
And no hardcoded home-directory path shall be required

### Scenario 7: Duplicate topics are merged conservatively

Given active topic files whose slugs match exactly or whose normalized slug-token overlap exceeds 80 percent  
When dream consolidation runs  
Then the system shall produce one canonical active topic representation  
And it shall archive the superseded duplicate files  
And it shall record the merge in the dream log

### Scenario 8: Oversized topics are split

Given an active topic whose content exceeds the 2K-token dream budget  
When dream consolidation runs  
Then the topic shall be split into bounded shards  
And `index.md` shall be updated to reference the new structure

### Scenario 9: Stale topics are archived, not deleted

Given a topic that has no durable updates and no supporting recent-daily references for 30 or more days  
When dream consolidation runs  
Then the topic shall be moved to archive  
And it shall no longer remain in the active topic set  
And the dream log shall record the stale archive action

### Scenario 10: Completion summary reaches notification sinks

Given a dream completes successfully  
When the completion handler runs  
Then a concise summary shall be appended to `autonomy.log`  
And, if Telegram notification delivery is available, a short summary shall also be sent through the channel notification path

## Edge Cases

### Edge Case 1: Foreground writes arrive during dream analysis

Given a dream snapshot has been built  
And new turn-time memory writes occur before dream commit begins  
When the dream tries to commit  
Then it shall compare current file `mtime` values against the snapshot `mtime` values  
And it shall abort the commit when any relevant file changed  
And it shall avoid overwriting the fresher data blindly

### Edge Case 2: Dream fails after staging but before full commit

Given a dream wrote staging metadata or temporary outputs  
When the process fails before the final commit completes  
Then the next dream shall recover from that state safely  
And active memory files shall still be readable and consistent

### Edge Case 3: Ambiguous duplicate detection

Given two topics do not share an exact slug and do not exceed the 80 percent normalized slug-token overlap threshold  
When dream consolidation runs  
Then the system shall keep both topics active  
And it shall log that no safe merge was applied

### Edge Case 4: Dream log already exists for the day

Given `dreams/YYYY-MM-DD.md` already contains prior entries  
When a new dream completes on the same day  
Then the system shall append a new entry  
And it shall not truncate or rewrite existing dream history

## Definition of Done

- [x] Idle, manual, and shutdown dream triggers are covered by tests
- [x] Dream remains non-blocking for the main foreground loop
- [x] Snapshot bounds, storage-relative paths, and 7-day daily-log limits are covered
- [x] Duplicate merge thresholding, stale archiving, and split behavior are covered
- [x] Dream log append-only behavior is covered
- [x] `mtime` conflict aborts, interrupted recovery, and atomic index updates are covered
- [x] Notification behavior to `autonomy.log` and Telegram-capable channels is covered
- [x] Review gate is completed later or explicit approval for draft status is recorded
