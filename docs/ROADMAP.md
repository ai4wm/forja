# Forja Roadmap

This roadmap tracks the repository at a high level. It is intentionally short and should stay aligned with the actual workspace.

## Completed

### Phase 1-27

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
- `identity.md` lifecycle cleanup, simpler key-driven emotion handling, and prompt/state deduplication
- local `llama.cpp` HTTP adapter support, automatic `~/.forja/models/` discovery, and Hugging Face model bootstrap support
- natural-language mapping for mode/think/role/model changes with explicit confirmation before state changes
- file-based skill loading, `/skills` and `/skill` activation, shell-backed skill execution, and basic skill improvement suggestions
- a three-stage memory system with in-process session memory, automatic compression/summarization, and SQLite FTS5 long-term retrieval layered on top of the existing markdown memory layout
- dual-model autonomy with a local heartbeat monitor, selective cloud escalation, and shared memory/policy context between local and cloud routing
- optional voice channel support with microphone capture, OpenAI speech transcription, OpenAI text-to-speech playback, and `/voice` runtime controls
- an optional TUI viewer and expanded dashboard APIs for session history, tool logs, memory state, and real-time event streaming without moving UI concerns into `forja-core`
- optional desktop notification routing with configurable filters, `/notify` controls, and platform-specific delivery handled outside `forja-core`

## Next Phases

## Notes

- `docs/STATUS.md` is the operational snapshot for the current repository state.
- This roadmap is for direction, not for file-level implementation detail.
