# SPEC-CREATION-001 Acceptance: Forja Creation Engine v0.2.0 Expansion

## Test Scenarios

### Scenario 1: `/debate` runs the expanded stage pipeline

Given the runtime is configured with the creation engine enabled  
When the user runs `/debate <topic>`  
Then the engine shall execute divergence, conflict, combination, mutation, and convergence in order  
And the user shall receive intermediate transcript output plus a final summary and task list

### Scenario 2: Default 5-agent team still works

Given the default creation-engine configuration  
When a creation run starts  
Then the system shall activate the existing 5-agent baseline  
And the run shall complete without requiring a separate configuration file

### Scenario 3: Combination stage emits transcript messages

Given a creation run reaches the combination stage  
When the stage executes  
Then the transcript shall include messages labeled as combination output  
And those prompts shall use explicit cross-domain fusion instructions such as TRIZ or SCAMPER patterns

### Scenario 4: Mutation stage emits transcript messages

Given a creation run reaches the mutation stage  
When the stage executes  
Then the transcript shall include messages labeled as mutation output  
And those prompts shall force inversion, amplification, reduction, elimination, or failure-to-advantage transformations

### Scenario 5: Final output still becomes executable tasks

Given the convergence stage returns the final creation output  
When the result parser runs  
Then the system shall produce a non-empty `TaskItem` list  
And each parsed task shall include name, assigned role, estimated hours, and priority

### Scenario 6: RALF retries wrap creation-agent calls

Given a transient provider failure during a creation-agent call  
When the stage executes  
Then the system shall retry the call according to the configured RALF policy  
And the retry or final failure shall be reflected in the audit log

### Scenario 7: Budget is enforced per creation agent

Given the runtime is in budget enforce mode  
And a creation-agent call would exceed budget  
When the call is about to execute  
Then the system shall stop the run with a clear budget error  
And it shall not silently continue that call

### Scenario 8: Existing budget path is reused for inner creation agents

Given the runtime already registered the outer runtime agent with the existing budget manager  
When the creation engine starts a per-stage inner agent call  
Then the system shall map that call to a scoped creation-agent id within the same budget manager  
And it shall avoid introducing a second budget database or parallel policy path

### Scenario 9: Team sizing and stage-count customization are observable

Given configuration enables complexity-based team sizing or custom stage counts  
When a creation run starts  
Then the system shall activate the configured 3-5 agent subset and round counts  
And the transcript and result metadata shall expose the effective team size and stage counts

### Scenario 10: Untrusted provider task text is bounded before handoff

Given a provider returns malformed or oversized task text during convergence  
When final parsing runs  
Then only allowed task fields shall be retained  
And raw provider text shall not be treated as directly executable metadata

### Scenario 11: Later-stage prompt reuse is bounded

Given an earlier creation stage produced a large transcript segment  
When a later stage builds its prompt  
Then reused prior-stage content shall be truncated or summarized to at most 2,000 characters for that stage input  
And the later-stage prompt shall not embed unbounded raw prior transcript text

### Scenario 12: Exhausted failures keep a bounded error surface

Given a creation-agent call still fails after RALF retries are exhausted  
When the creation run returns the failure to the runtime layer  
Then the API surface shall remain `Result<DebateResult>`  
And the returned error text shall be redacted and truncated to at most 512 UTF-8 characters

### Scenario 13: Redacted audit logging uses explicit bounds

Given a creation message or failure contains secrets, tokens, or long free-form text  
When the audit logger records the event  
Then the stored transcript or failure text shall be masked for recognized credentials  
And the persisted text field shall be truncated to at most 512 UTF-8 characters  
And no new raw-transcript persistence target beyond `audit.db` shall be created

## Edge Cases

### Edge Case 1: Monitor mode budget overrun

Given the runtime is in budget monitor mode  
When a creation-agent call exceeds budget  
Then the system shall log the overrun  
And the creation run shall continue

### Edge Case 2: Unparseable task lines

Given the converged text contains some malformed task lines  
When final parsing runs  
Then valid task lines shall still be preserved  
And the summary shall still be returned

### Edge Case 3: Stage with no useful prior output

Given an earlier creation stage returned no meaningful text  
When a later stage builds its prompt  
Then the system shall use a bounded placeholder context  
And prompt construction shall not panic on empty transcript segments

### Edge Case 4: Zero valid task lines

Given the converged output contains no valid task lines  
When final parsing completes  
Then the system shall still return the summary  
And it shall emit a bounded fallback `TaskItem` instead of an empty execution handoff

### Edge Case 5: Sensitive transcript content

Given a creation message or failure contains secrets, tokens, or unrelated sensitive text  
When audit logging records the event  
Then the logged payload shall be redacted or truncated according to the documented logging bounds  
And raw long-form sensitive content shall not be retained unnecessarily

### Edge Case 6: Missing budget registration for an inner creation agent

Given a debate run reaches a creation agent whose scoped budget id has not been registered yet  
When the call is about to execute  
Then the runtime shall lazily register the stable scoped id such as `creation/{agent-id}` in the existing budget manager  
And the call shall continue through the same monthly budget path instead of failing due to missing registration

## Definition of Done

- [x] The expanded stage pipeline is covered by tests
- [x] Combination and mutation transcript stages are covered by tests
- [x] RALF retry integration is covered by tests
- [x] Budget enforcement and monitor-mode behavior are covered by tests
- [x] Existing budget-path reuse for inner creation agents is covered by tests
- [x] Complexity-based team sizing and stage-count customization are covered by tests
- [x] Task-list parsing and malformed-output fallback are covered by tests
- [x] Zero-valid-task fallback is covered by tests
- [x] Trust-boundary and bounded task parsing behavior are covered by tests
- [x] Transcript/failure redaction and retention bounds are covered by tests
- [x] Later-stage prompt reuse limits are covered by tests
- [x] Bounded failure-surface behavior is covered by tests
- [x] Existing debate behavior remains covered and does not regress
