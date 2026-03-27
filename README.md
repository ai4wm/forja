[한국어](docs/README.ko.md) | [日本語](docs/README.ja.md) | [中文](docs/README.zh-CN.md) | [Español](docs/README.es.md) | [Português](docs/README.pt-BR.md)

# Forja

A lightweight, cross-platform AI agent engine built in Rust.

Forja is a personal AI assistant that lives in your terminal. It remembers past conversations, detects emotional context, controls your OS, analyzes screenshots, and adapts its reasoning depth all through natural language.

## Features

**Multi-Provider LLM Support**
Connect to OpenAI, Anthropic, Google Gemini, DeepSeek, Moonshot, xAI, GLM, or local Ollama models. Switch providers and models at runtime with `/model`.

**Persistent Memory**
Rolling memory system stored in markdown. Forja remembers past conversations across restarts no "session" boundaries.

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

## Quick Start

### Install from source

```bash
git clone https://github.com/ai4wm/forja.git
cd forja
cargo run
```

First run launches the onboarding wizard to configure your provider, assistant name, and preferences.

### Install from crates.io

```bash
cargo install forja
```

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
| `/ss [prompt]` | Capture screen + Vision analysis |
| `/image <path> [prompt]` | Analyze image file |
| `/help` | Show available commands |

## Architecture

```text
forja/
 src/main.rs              # Entry point, onboarding, tool registration
 crates/
    forja-core/          # Engine loop, prompt assembly, mode system
    forja-llm/           # Multi-provider LLM client
    forja-memory/        # Markdown + BM25 memory store
    forja-tools/         # Shell, input, browser, vision, search tools
    forja-channel/       # CLI and Telegram channels
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
8. Memory context (from memory.md)
9. User global prompt: `~/.forja/USER.md`
10. Project prompt: `AGENTS.md` `FORJA.md` `CLAUDE.md`

## Channels

- **CLI**: Always available, with streaming output
- **Telegram**: Activate with bot token. Whitelist-based access control with typing indicators.

## License

MIT OR Apache-2.0

## Links

- [Repository](https://github.com/ai4wm/forja)
- [Architecture](docs/ARCHITECTURE.md)
- [Roadmap](docs/ROADMAP.md)
