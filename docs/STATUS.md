# Forja Status

Last updated: 2026-03-29

## Task Record

- Changed files: `crates/forja-core/Cargo.toml`, `crates/forja-core/src/lib.rs`, `crates/forja-core/src/skill.rs`, `crates/forja-core/src/skill_eval.rs`, `crates/forja-core/src/skill_improve.rs`, `src/main.rs`, `tests/integration_test.rs`, `examples/skills/hello-world/SKILL.md`, `examples/skills/git-summary/SKILL.md`, `docs/STATUS.md`
- Dependencies for next task: keep `docs/STATUS.md` aligned with code changes; decide whether eval results should be persisted to a dedicated skill history file in addition to MemoryManager; expand direct integration coverage for `/skill eval`, `/skill improve`, and `/skill benchmark`; decide whether benchmark output should retain per-case breakdowns or only aggregate stats
- Verification: `cargo test --workspace --exclude forja-llm`; `cargo build --workspace`; `cargo clippy --workspace -- -D warnings`

## Feature Status

| Feature | Status | File(s) | Notes |
| --- | --- | --- | --- |
| Engine loop | Done | `crates/forja-core/src/engine.rs`, `src/main.rs` | Main runtime loop, tool recursion, slash interception, and channel dispatch are wired. |
| Streaming | Done | `crates/forja-core/src/engine.rs`, `crates/forja-llm/src/client.rs` | Streaming-first path exists with fallback to non-streaming tool handling. |
| Prompt loader | Done | `crates/forja-core/src/prompt/loader.rs`, `crates/forja-core/src/prompt/mod.rs`, `src/main.rs` | File-based prompts are loaded from `~/.forja/prompts/` by default and bootstrapped with defaults. |
| Session buffer | Done | `crates/forja-memory/src/session.rs`, `crates/forja-memory/src/manager.rs` | Ephemeral per-process session memory tracks recent messages and estimated token usage before compression. |
| Auto compression | Done | `crates/forja-memory/src/compressor.rs`, `crates/forja-memory/src/manager.rs` | Session overflow now compresses the oldest half of buffered messages into rule-based summaries without LLM calls. |
| Long-term store | Done | `crates/forja-memory/src/longterm.rs`, `crates/forja-memory/src/manager.rs` | Long-term memory is stored in append-only `longterm.md` entries and searched with an internal keyword-ranking scorer. |
| MemoryManager | Done | `crates/forja-memory/src/manager.rs`, `crates/forja-memory/src/lib.rs`, `src/main.rs` | `MemoryManager` now owns session, compression, and long-term recall, with a compatibility wrapper feeding query-aware context into the existing engine memory path. |
| `/memory` commands | Done | `crates/forja-memory/src/manager.rs`, `src/main.rs`, `tests/integration_test.rs` | `/memory`, `/memory search <query>`, `/memory clear session`, and `/memory flush` are implemented in the runtime slash path. |
| Per-agent memory paths | Not started (Phase 28) | `crates/forja-memory/src/longterm.rs` | The path helper exists, but agent-scoped memory routing is not wired into the runtime yet. |
| Shell tool | Done | `crates/forja-tools/src/shell.rs`, `crates/forja-core/src/safety.rs`, `crates/forja-core/src/engine.rs` | Shell execution supports confirmation and mode-aware safety checks. |
| Browser tool | Done | `crates/forja-tools/src/browser.rs`, `src/main.rs` | Chromium CDP backend, screenshotting, tab control, and confirmation flow are implemented. |
| Vision tool | Done | `crates/forja-tools/src/vision.rs`, `src/main.rs` | Screen capture, region analysis, OCR, and image analysis flows are implemented. |
| Input tool | Done | `crates/forja-tools/src/input.rs`, `src/main.rs` | Keyboard and mouse actions are implemented behind backend and confirmation layers. |
| Search tool | Done | `crates/forja-tools/src/search.rs`, `src/main.rs` | DuckDuckGo, Brave, and Grok-backed search providers are supported. |
| CLI channel | Done | `crates/forja-channel/src/cli.rs`, `src/main.rs` | Interactive terminal input/output is implemented. |
| Telegram channel | Done | `crates/forja-channel/src/multi.rs`, `crates/forja-channel/src/telegram.rs`, `src/main.rs` | Telegram runs alongside CLI with allowlisted chat IDs and typing indicators. |
| ExecMode | Done | `crates/forja-core/src/mode.rs`, `crates/forja-core/src/safety.rs`, `crates/forja-tools/src/confirm.rs`, `src/main.rs` | `safe`, `auto`, and `trust` are resolved and enforced across shell, browser, and input paths. |
| `/mode` | Done | `crates/forja-core/src/mode.rs`, `src/main.rs` | Switches execution mode at runtime. |
| `/think` | Done | `crates/forja-core/src/mode.rs`, `src/main.rs` | Switches reasoning depth at runtime. |
| `/role` | Done | `crates/forja-core/src/mode.rs`, `src/main.rs` | Switches role prompt selection at runtime. |
| `/model` | Done | `src/main.rs`, `src/provider_registry.rs` | Resolves and switches the active provider/model entry. |
| `/models` | Done | `src/main.rs`, `src/provider_registry.rs` | Lists active and available model entries. |
| `/ss` | Done | `crates/forja-core/src/mode.rs`, `src/main.rs` | Captures the screen and routes it through the vision analyzer. |
| `/image` | Done | `crates/forja-core/src/mode.rs`, `src/main.rs` | Loads an image file and routes it through the vision analyzer. |
| `/help` | 🔧 Partial | `README.md`, `src/main.rs`, `tests/integration_test.rs` | The runtime slash handler now responds to `/help`, but direct integration coverage is still limited and the older ignored placeholder test remains. |
| Natural language command mapping | Done | `crates/forja-core/src/intent.rs`, `crates/forja-core/src/lib.rs`, `src/main.rs`, `tests/integration_test.rs` | Natural language requests are mapped to internal commands through zero-cost pattern matching and routed through the existing slash-command execution path. |
| Skill system (loader) | Done | `crates/forja-core/src/skill.rs`, `crates/forja-core/src/lib.rs`, `tests/integration_test.rs` | Skill folders under `~/.forja/skills/` are discovered, parsed, cached, and summarized for prompt use. |
| Skill system (execution) | Done | `src/main.rs`, `crates/forja-tools/src/shell.rs` | Skill scripts are executed from their skill directory with skill-scoped environment injection and ExecMode-aware confirmation behavior. |
| Skill system (trigger matching) | Done | `crates/forja-core/src/intent.rs`, `crates/forja-core/src/skill.rs`, `tests/integration_test.rs` | Built-in commands are checked first, then installed skills are matched by trigger to produce `InternalCommand::Skill`. |
| Skill system (slash commands) | Done | `src/main.rs`, `crates/forja-core/src/engine.rs` | `/skill list`, `/skill run <name> [args]`, `/skill info <name>`, and `/skill reload` are handled in the runtime slash path. |
| Skill system (eval) | Done | `crates/forja-core/src/skill_eval.rs`, `crates/forja-core/src/skill.rs`, `src/main.rs`, `tests/integration_test.rs` | Skills can now load structured test cases and run rule-based evaluations through callback-backed script execution. |
| Skill system (improve) | Done | `crates/forja-core/src/skill_improve.rs`, `src/main.rs`, `tests/integration_test.rs` | Improvement suggestions are generated from eval failures without LLM calls and are recorded through the memory layer. |
| Skill system (benchmark) | Done | `crates/forja-core/src/skill_eval.rs`, `src/main.rs`, `tests/integration_test.rs` | Benchmark runs aggregate pass rate and timing statistics across repeated evaluations. |
| Background model manager | Done | `crates/forja-core/src/background.rs`, `src/background_runtime.rs`, `src/main.rs` | Background manager start/stop/status handling is implemented, and startup auto-discovery runs without blocking the foreground engine. |
| Groq provider | Done | `crates/forja-llm/src/presets.rs`, `src/provider_registry.rs`, `src/config.rs` | Groq is available through the OpenAI-compatible client path and can be selected with registry-backed models. |
| OpenRouter provider | Done | `crates/forja-llm/src/presets.rs`, `src/provider_registry.rs`, `src/config.rs` | OpenRouter is available through the OpenAI-compatible client path with curated free-model registry entries and background-model probing support. |
| Local model detection | Done | `crates/forja-llm/src/local.rs`, `src/background_runtime.rs`, `src/main.rs` | `~/.forja/models/` is created automatically, GGUF files are detected, and a stub local provider is available for future inference integration. |
| emotion.rs refactor | Done | `crates/forja-core/src/emotion.rs`, `crates/forja-core/src/engine/emotion.rs`, `crates/forja-core/src/prompt/loader.rs`, `crates/forja-core/src/prompt/mod.rs` | Emotion handling is now key-based and local. `emotion.md` is auto-created when missing, and active signal keys are appended during prompt assembly. |
| Identity onboarding | Done | `src/bootstrap.rs`, `src/main.rs`, `crates/forja-core/src/prompt/base.rs`, `crates/forja-core/src/prompt/loader.rs` | First-run onboarding now writes a single `identity.md` profile with `user_name`, `assistant_name`, `language`, and `tone`, and those values drive base prompt placeholder rendering. |
| Integration tests | Done | `tests/integration_test.rs`, `tests/phase13e_bootstrap.rs`, `tests/phase18_mode.rs` | Integration and phase-based coverage exists for major subsystems. |
| CI/CD release | Done | `.github/workflows/ci.yml`, `.github/workflows/release.yml` | CI builds and tests the workspace, and tagged releases generate artifacts. |
| Multilingual README | Done | `README.md`, `docs/README.ko.md`, `docs/README.ja.md`, `docs/README.zh-CN.md`, `docs/README.es.md`, `docs/README.pt-BR.md` | Root README links to translated documentation variants. |
