[한국어](docs/README.ko.md) | [日本語](docs/README.ja.md) | [中文](docs/README.zh-CN.md) | [Español](docs/README.es.md) | [Português](docs/README.pt-BR.md)

# Forja

A lightweight, cross-platform AI agent engine built in Rust.

Forja is a personal AI assistant that lives in your terminal. It remembers past conversations, detects emotional context, controls your OS, analyzes screenshots, and adapts its reasoning depth all through natural language.

## Features

**Multi-Provider LLM Support**
Connect to OpenAI, Anthropic, Google Gemini, DeepSeek, Moonshot, xAI, GLM, or local Ollama models. Switch providers and models at runtime with `/model`.

**Persistent Memory**
Three-layer markdown wiki memory with `index.md`, `topics/`, and `daily/`. Forja keeps durable context across restarts and migrates legacy `memory.md` into the structured layout.

**Emotion & Relationship Awareness**
Detects emotional signals (late night work, long absence, frustration) and adjusts tone naturally.

**OS Control**
- Shell command execution with safety confirmations
- Keyboard and mouse input (type, click, scroll, hotkeys)
- CDP browser automation (navigate, click, type, read pages, take screenshots)
- Screen capture + GPT Vision analysis

**Smart Input**
- Drag-and-drop image files for instant Vision analysis
- `/ss` for screen capture + analysis
- `/image <path>` for file-based image analysis
- Multiline input with `\` continuation

**Adaptive Thinking**
Three reasoning modes: `/think min` (concise), `/think mid` (default), `/think max` (deep reasoning with self-verification).

**Execution Modes**
`/mode safe` (confirm everything), `/mode auto` (confirm dangerous only), `/mode trust` (no confirmations).

**Auto Role Detection**
Automatically switches between coder, writer, assistant, and analyst prompts based on conversation context.

**Configurable Identity**
Set assistant name and user title during onboarding. No hardcoded language responds in whatever language you use.

**Local Desktop Dashboard**
`/dashboard` opens an Axum-backed local desktop surface with live conversation streaming, memory browsing, tool monitoring, and audit/task views.

**MCP Tool Server**
`forja-tools` now includes a stdio MCP server so external agents can list and call Forja tools through `tools/list` and `tools/call`.

## Quick Start

### Install (recommended)

```bash
cargo install forja
forja                # launch (interactive onboarding on first run)
```

After installation, `forja` is available globally from any directory.

### From source

```bash
git clone https://github.com/ai4wm/forja.git
cd forja
cargo run
```

### MCP server

```bash
cargo run -p forja-tools --bin forja-mcp
```

The MCP server uses stdio transport and exposes a pragmatic default tool set from `forja-tools`.

### Pre-built binaries

Download from [GitHub Releases](https://github.com/ai4wm/forja/releases/latest) for Windows, macOS, and Linux.

### Setup

```bash
forja setup          # Run setup wizard
forja login openai   # OAuth login
forja login gemini   # OAuth login
forja --provider openai_oauth --model gpt-5.4  # Override at launch
```

## Configuration

Config file: `~/.forja/config.toml`

```toml
[active]
provider = "openai_oauth"
model = "gpt-5.4"

[identity]
assistant_name = "Forja"
user_title = "User"

[keys]
openai = "sk-..."
anthropic = "sk-ant-..."

[channel.telegram]
bot_token = "123456:token"
allowed_chat_ids = [123456789]

[channel.discord]
bot_token = "discord-bot-token"
allowed_user_ids = [123456789012345678]
# allowed_channel_ids = [123456789012345678]
# allowed_guild_ids = [123456789012345678]
```

### Environment Variables

| Variable | Description |
|----------|-------------|
| `FORJA_MODE` | Execution mode: `safe`, `auto`, `trust` |
| `FORJA_THINK` | Thinking level: `min`, `mid`, `max` |
| `FORJA_ASSISTANT_NAME` | Override assistant name |
| `FORJA_USER_TITLE` | Override user title |
| `FORJA_PROVIDER` | Override LLM provider |
| `FORJA_MODEL` | Override model |
| `FORJA_USE_MOCK` | Run without real API calls |
| `FORJA_VISION` | Enable/disable vision (`true`/`false`) |
| `FORJA_BROWSER` | Enable/disable browser tool |
| `FORJA_INPUT` | Enable/disable input tool |

## Slash Commands

| Command | Description |
|---------|-------------|
| `/model <name>` | Switch model |
| `/models` | List available models |
| `/mode <safe\|auto\|trust>` | Set execution mode |
| `/think <min\|mid\|max>` | Set reasoning depth |
| `/role <coder\|writer\|assistant\|analyst\|auto>` | Set role |
| `/dashboard` | Open the local dashboard |
| `/task add <description>` | Queue a task for autonomy mode |
| `/task list` | List queued tasks |
| `/task cancel <id>` | Cancel a queued task |
| `/autonomy <start\|stop\|status>` | Control or inspect autonomy mode |
| `/ss [prompt]` | Capture screen + Vision analysis |
| `/image <path> [prompt]` | Analyze image file |
| `/skills` | List recorded skill usage |
| `/unresolved` | List unresolved autonomy tasks |

## Memory Layout

- `index.md`: topic index used for startup memory context
- `topics/`: sharded topic notes loaded for relevant queries
- `daily/`: recent chronological memory logs
- Legacy `memory.md`: migrated into the structured layout on startup

## Architecture

```text
forja/
 src/main.rs              # Thin CLI entry point and shutdown orchestration
 src/runtime/
    startup.rs           # Runtime assembly, channel/memory/dashboard wiring
    tools.rs             # Tool registration
    slash.rs             # Slash command routing
    shutdown.rs          # Shutdown coordination
    prompt.rs            # System/tool/memory prompt assembly
    mock.rs              # Mock provider support
 crates/
    forja-core/          # Engine loop, autonomy, audit, gateway, mode system
    forja-llm/           # Multi-provider LLM client
    forja-memory/        # Structured markdown wiki memory store
    forja-tools/         # Shell, input, browser, vision, search tools
    forja-channel/       # CLI channel and supervised Telegram sidecar
```

## Supported Providers

OpenAI (API & OAuth), Anthropic, Google Gemini (API & OAuth), DeepSeek, Moonshot, xAI, GLM, Ollama.

Use `/models` at runtime to see all available models.

## Prompt Loading Order

1. Base prompt (identity, memory rules, core rules)
2. Think mode prompt (min/mid/max)
3. Role-specific prompt (coder/writer/assistant/analyst)
4. Tool descriptions
5. Emotion context
6. Relationship context
7. Knowledge context
8. Memory context (from `index.md`, recent `daily/`, and relevant `topics/` shards)
9. User global prompt: `~/.forja/USER.md`
10. Project prompt: `AGENTS.md` `FORJA.md` `CLAUDE.md`

## Channels

- **CLI**: Always available, with streaming output
- **Telegram**: Supervised sidecar with whitelist-based access control, typing indicators, bounded auto-reconnect, and dashboard-visible connection status.
- **Discord**: Optional channel adapter with allowlist-based access control, typing indicators, and reconnecting gateway runtime.

## MCP Integration

- Launch the MCP server with `cargo run -p forja-tools --bin forja-mcp`
- Transport: stdio
- Supported methods: `initialize`, `tools/list`, `tools/call`, `ping`
- Default tool registry: `file_tool`, `web_tool`, `search_tool`, `shell`

## License

MIT OR Apache-2.0

## Links

- [Repository](https://github.com/ai4wm/forja)
- [Architecture](docs/ARCHITECTURE.md)
- [Roadmap](docs/ROADMAP.md)
