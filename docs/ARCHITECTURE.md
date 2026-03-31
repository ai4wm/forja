# FORJA v0.2.0 Architecture Document

This document defines the target architecture for Forja v0.2.0.
It is a product and system design document for the next milestone, not a snapshot of the current implementation.
The previous implementation-focused document is preserved in `docs/ARCHITECTURE-v010.md`.

## Vision

An AI agent engine that multiplies a solo developer's effective thinking capacity by 5x.

## Core Principles

- Single binary: everything should run from one `cargo run`.
- No separate server: use SQLite locally and defer UI expansion to Tauri later.
- Three primitive structures: message, agent, and loop.
- Creative reasoning and execution should stay inside one runtime loop.

## Four-Layer Architecture

### Layer 1. Context Engineering

Goal: give the system durable working memory without losing runtime simplicity.

- Token counting with an 80% warning threshold and 90% automatic compression.
- Memory system built around `MEMORY.md` and `USER.md`.
- Preserve the most recent N turns and compress older conversation into summaries.
- Long-term memory backed by SQLite FTS5 for full-text retrieval.

### Layer 2. Harness

Goal: control execution, cost, safety, and observability with a unified runtime envelope.

- Gateway with a common message format:

```text
{ sender, text, channel, timestamp, type }
```

- RALF loop with automatic retry on failure:
  - Retry up to 5 times.
  - Stop early if the same error appears 3 times in a row.
- Heartbeat scheduler for each agent:
  - Agents run on schedule.
  - Idle agents should cost zero.
- Budget management per agent:
  - Monthly token limit.
  - 80% warning.
  - 100% automatic stop.
- Append-only audit log in SQLite for all tool calls and decision records.
- Governance prompts for risky actions such as deployment or deletion.
- Mention filtering to remove noise in group-chat environments.

### Layer 3. Creation Engine

Goal: produce better decisions by forcing structured divergence, conflict, and synthesis.

- Debate mode with three stages:
  - Divergence: expand ideas with a "Yes, and..." pattern.
  - Conflict: challenge proposals through explicit cognitive frameworks.
  - Convergence: compress the discussion into a decision.
- Combination engine that forces cross-domain fusion using patterns such as TRIZ and SCAMPER.
- Mutation engine for inversion, amplification, reduction, elimination, and turning failure into advantage.
- Agent composition is defined by:
  - `role`
  - `framework`
  - `budget`
- Default team size is 5 agents, with automatic adjustment to 3-5 agents based on task complexity.
- Standard round structure:
  - Divergence: 2 rounds
  - Conflict: 3 rounds
  - Convergence: 1 round
  - Total: 6 rounds
- Debate output should be transformed automatically into executable task lists.
- The same agent may switch behavior between stages instead of remaining locked to one mode.

### Layer 4. Autonomy

Goal: let the system take initiative without introducing a separate orchestration service.

- Heartbeat-based autonomous execution.
- Automatic skill registration after 5 or more successful tool-call runs.
- Unresolved problem storage with periodic re-approach.
- Multi-company and per-project data isolation.

## Agent Definition Example

```toml
[agents.architect]
role = "Architecture"
framework = "Break every proposal into no more than three components. Reject it if it stays complex."
budget = 50000

[agents.critic]
framework = "Find falsifiable flaws in every claim and estimate the probability of failure."

[agents.builder]
framework = "If it cannot be implemented within 48 hours, propose an alternative."

[agents.researcher]
framework = "Ignore claims without sources. Judge only from data."

[agents.synthesizer]
framework = "Summarize the discussion in three sentences and convert it into an execution task list."
```

## Creation To Execution Flow

```text
User: "Should we try something like this?"

[Creation]
5-agent debate
+ combination
+ mutation
= conclusion

[Execution]
Conclusion
+ task list
+ agent assignment
= work

[Record]
Audit log
+ skill registration
```

## Technology Stack

- Language: Rust
- Database: SQLite with FTS5
- UI: CLI first, Tauri later
- Channels: CLI and Telegram first, Discord later
- LLM access: OpenAI OAuth by default, with model replacement kept possible

## Implementation Priority

1. Context engineering
2. Gateway refactor, RALF loop, and audit log
3. Heartbeat scheduling and budget management
4. Debate engine
5. Combination and mutation rounds
6. Automatic skill registration and autonomous execution
7. Tauri UI

## Imported Ideas

### From Paperclip

- Heartbeat scheduling
- Budget management
- Audit logging
- Adapter pattern
- Ticket system
- Governance

Constraint: implement these inside a single-binary runtime with no server split.

### From Hermes

- Learning system with `MEMORY.md` and `USER.md`
- Automatic skill registration
- Automatic context compression

### From Chorus

- Replace persona-first design with cognitive frameworks
- Use framework conflict to generate emergent outcomes

## Differentiators

- One loop that combines creation and execution instead of separating them.
- Paperclip-class operational ideas inside a single binary.
- Optimized for a solo developer workflow that should finish with one `cargo run`.
