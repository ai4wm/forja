# Forja Architecture

This document summarizes the current repository layout and runtime behavior from the codebase as of `src/main.rs`.

## Workspace Structure

```text
forja/
├─ src/main.rs                    Binary entry point and runtime wiring
├─ src/config.rs                  Config loading, onboarding, env overrides
├─ src/bootstrap.rs               identity.md / user.md bootstrap and prompt prefix
├─ src/provider_registry.rs       Runtime model registry for /models and /model
├─ crates/forja-core/             Engine loop, traits, message types, mode system
├─ crates/forja-llm/              Multi-provider LLM client implementations
├─ crates/forja-memory/           Markdown-backed memory storage and summarization
├─ crates/forja-tools/            Shell, browser, vision, input, search, file, web tools
└─ crates/forja-channel/          CLI and Telegram channel implementations
```

## Layer Diagram

```text
User / External Input
        |
        v
Channel Layer
  CLI / Telegram
  (`forja-channel`)
        |
        v
Engine Layer
  `forja-core::Engine`
        |
        +-------------------+
        |                   |
        v                   v
LLM / Tools / Memory Layer
  `forja-llm`   `forja-tools`   `forja-memory`
        |
        v
APIs, shell, browser, filesystem, screenshots, local memory files
```

## Key Traits And Backend Interfaces

- `LlmProvider` in `crates/forja-core/src/traits.rs`
  - Defines `chat()` and `stream()` for single-shot and token-stream responses.
  - Implemented by `forja-llm::LlmClient` and the local `MockLlmProvider` in `src/main.rs`.
- `Channel` in `crates/forja-core/src/traits.rs`
  - Defines `receive()`, `send()`, `confirm()`, and source-specific helpers.
  - Implemented by CLI and Telegram-facing channel adapters in `crates/forja-channel/`.
- `InputBackend` in `crates/forja-tools/src/input.rs`
  - Separates OS input execution from the tool wrapper.
  - Implemented by `EnigoBackend` and `MockBackend`.
- Tool backend pattern
  - There is no single repo-wide `ToolBackend` trait today.
  - The engine-facing abstraction is `forja_core::traits::Tool`.
  - Tool-specific backend interfaces are split by capability, such as `BrowserBackend`, `ScreenCaptureBackend`, and `VisionAnalyzer`.

## Prompt Loading

- Prompt files are loaded from `~/.forja/prompts/` by default.
- `src/main.rs` resolves the prompt directory from `agent.prompts_dir`, or falls back to `bootstrap_paths.forja_dir.join("prompts")`.
- `PromptLoader` in `crates/forja-core/src/prompt/loader.rs` creates missing defaults for:
  - `base.md`
  - `memory-rules.md`
  - `roles/coder.md`, `roles/writer.md`, `roles/assistant.md`, `roles/analyst.md`
  - `think/min.md`, `think/max.md`
- The final system prompt also includes bootstrap content from `~/.forja/identity.md` and `~/.forja/user.md`, plus the first project prompt found in `AGENTS.md -> FORJA.md -> CLAUDE.md`.

## Runtime Assembly

`src/main.rs` builds the runtime in this order:

1. Parse CLI arguments and login/setup subcommands.
2. Load config or run onboarding.
3. Initialize the prompt loader.
4. Build the system prompt from bootstrap files and project prompt files.
5. Create the provider registry and active `LlmProvider`.
6. Resolve `ExecMode`, think level, and role state.
7. Create the active channel (`CliChannel` or `MultiChannel` with Telegram).
8. Construct `Engine`, then attach tool prompt, assistant profile, knowledge, serendipity, emotion, memory, and slash handlers.
9. Register tools and optional external CLI bridge tools.
10. Start `Engine::run_streaming()`.

## Data Flow

Normal user requests follow this path:

1. A channel receives user input and converts it into `Message`.
2. `Engine` refreshes role, emotion, knowledge, and memory context for the turn.
3. `assemble_system_prompt()` builds the current system prompt from prompt files, mode state, tool prompt, bootstrap content, and contextual sections.
4. The engine sends request messages to the active `LlmProvider`.
5. If the model returns text, the engine streams or sends the response to the channel.
6. If the model returns a tool call, the engine executes the matching `Tool`, appends the tool result, and asks the model again.
7. The final assistant response is sent to the active channel and optionally written to memory.

## ExecMode Enforcement

Forja currently enforces execution policy in two layers:

- Shell flow
  - `Engine::handle_step()` inspects shell tool arguments before execution.
  - `safe` confirms every shell command.
  - `auto` confirms only commands classified as dangerous by `forja_core::safety`.
  - `trust` skips shell confirmation.
- Input and browser flow
  - `src/main.rs` shares the active mode through `exec_mode_handle`.
  - `StdinConfirmation::from_shared()` is passed into `InputTool` and `BrowserTool`.
  - Those tools confirm based on the live mode and their own danger checks.

## Streaming, Tools, And Memory

- Streaming-first execution lives in `Engine::run_streaming()`.
- If provider streaming fails or looks like a tool-call payload, the engine falls back to `handle_step()`.
- Tool recursion is bounded by `MAX_TOOL_DEPTH = 10`.
- Memory uses `MarkdownMemoryStore` in `crates/forja-memory/`.
  - `memory.md` is the primary rolling log.
  - older daily blocks can be summarized and archived.

## Current Boundaries

- CLI and Telegram are implemented; Discord is feature-gated but not implemented in this workspace.
- The prompt system is file-based and override-friendly.
- Tool backends are capability-specific, not unified under one `ToolBackend` trait.
- Memory is wired into the runtime, but retrieval remains file-centric rather than a full database-backed memory system.
