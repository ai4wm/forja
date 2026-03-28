# Forja Roadmap

This roadmap tracks the repository at a high level. It is intentionally short and should stay aligned with the actual workspace.

## Completed

### Phase 1-18

Delivered across the current repository:

- Core engine loop in `forja-core`
- Multi-provider LLM support in `forja-llm`
- Tool execution for shell, file, web, search, input, browser, and vision flows
- CLI and Telegram channels
- File-based prompt loader with role and think overlays
- `ExecMode` support for `safe`, `auto`, and `trust`
- Integration and phase-based test coverage
- Multilingual README set
- `v0.1.0` release workflow and release assets

## Next Phases

### Phase 19: Core cleanup

Goal: reduce prompt and personality wiring debt in the current runtime.

- Tighten `identity.md` onboarding and lifecycle
- Refactor `emotion.rs` into a simpler, key-driven structure
- Remove duplicated prompt and state handling where possible

### Phase 20: Local model direct execution

Goal: treat local models as first-class runtime targets instead of external-only providers.

- Add direct `llama.cpp` binding or adapter support
- Auto-detect models under `~/.forja/models/`
- Support model download/bootstrap from Hugging Face

### Phase 21: Natural language to internal command mapping

Goal: let users express mode and control changes without remembering slash commands.

- Map conversational requests to internal commands
- Reuse existing `/mode`, `/think`, `/role`, and model-switch logic
- Keep explicit confirmation for state-changing actions where needed

### Phase 22: Skill system

Goal: load reusable task behaviors from local skill definitions.

- Define `SKILL.md` format and loading rules
- Trigger skills from user intent or explicit invocation
- Allow bounded code execution inside skill flows
- Add evaluation and improvement loops for reusable skills

### Phase 23: 3-stage memory system

Goal: move from a single rolling memory file to layered memory management.

- Session memory for the active run
- Compression/summarization layer for medium-term context
- Long-term database-backed memory for durable retrieval

### Phase 24: Dual-model autonomous agent

Goal: combine cheap local autonomy with selective cloud escalation.

- Run a local background monitor for continuous low-cost tasks
- Escalate to a stronger cloud model only when needed
- Share state, memory, and execution policy between both layers

### Phase 25: Voice channel

Goal: add spoken input and output without replacing the current text runtime.

- Speech-to-text input pipeline
- Text-to-speech replies
- Voice-aware channel orchestration and interruptions

### Phase 26: TUI / Web UI

Goal: provide richer interaction surfaces on top of the existing engine.

- Terminal UI for local workflows
- Web UI for session history, tools, and status visibility
- Keep `forja-core` independent from UI-specific concerns

### Phase 27: Notification system

Goal: make background work visible even when the user is not in the main session.

- Windows notification support through BurntToast
- Agent alerts for long-running or completed work
- Basic notification routing policy by task type and urgency

## Notes

- `docs/STATUS.md` is the operational snapshot for the current repository state.
- This roadmap is for direction, not for file-level implementation detail.
