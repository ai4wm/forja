[한국어](README.ko.md) | [日本語](README.ja.md) | **中文** | [Español](README.es.md) | [Português](README.pt-BR.md) | [English](../README.md)

This document is a translation of the [English README](../README.md).

# Forja

一个使用 Rust 构建的轻量级、跨平台 AI agent engine。

Forja 是一个驻留在终端中的个人 AI assistant。它能够通过自然语言记住过去的对话、检测情绪上下文、控制你的 OS、分析截图，并调整推理深度。

## Features

**多 provider LLM 支持**
可连接到 OpenAI、Anthropic、Google Gemini、DeepSeek、Moonshot、xAI、GLM，或本地 Ollama model。可通过 `/model` 在运行时切换 provider 和 model。

**持久化 Memory**
采用存储在 markdown 中的 rolling memory 系统。Forja 能跨重启记住过去的对话，没有“session”边界。

**Emotion 与关系感知**
能够检测深夜工作、长时间离开、挫败感等情绪信号，并自然地调整语气。

**OS 控制**
- 带安全确认的 shell command 执行
- 键盘和鼠标输入（type、click、scroll、hotkeys）
- CDP browser 自动化（navigate、click、type、read pages、take screenshots）
- 屏幕捕获 + GPT Vision 分析

**Smart Input**
- 拖放 image 文件即可立即进行 Vision 分析
- 使用 `/ss` 进行屏幕捕获 + 分析
- 使用 `/image <path>` 进行基于文件的 image 分析
- 使用 `\` continuation 的 multiline input

**自适应 Thinking**
三种推理模式：`/think min`（简洁）、`/think mid`（默认）、`/think max`（带 self-verification 的深度推理）。

**执行模式**
`/mode safe`（全部确认）、`/mode auto`（仅确认危险操作）、`/mode trust`（不确认）。

**自动 Role 检测**
根据对话上下文，自动在 coder、writer、assistant 和 analyst prompt 之间切换。

**可配置的 Identity**
可在 onboarding 期间设置 assistant 名称和 user title。没有硬编码语言，会根据你使用的语言进行响应。

## Quick Start

### 从源码安装

```bash
git clone https://github.com/ai4wm/forja.git
cd forja
cargo run
```

首次运行会启动 onboarding wizard，用于配置 provider、assistant 名称和偏好设置。

### 从 crates.io 安装

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

配置文件：`~/.forja/config.toml`

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
| `FORJA_MODE` | 执行模式：`safe`、`auto`、`trust` |
| `FORJA_THINK` | Thinking 级别：`min`、`mid`、`max` |
| `FORJA_ASSISTANT_NAME` | 覆盖 assistant 名称 |
| `FORJA_USER_TITLE` | 覆盖 user title |
| `FORJA_PROVIDER` | 覆盖 LLM provider |
| `FORJA_MODEL` | 覆盖 model |
| `FORJA_USE_MOCK` | 在不进行真实 API 调用的情况下运行 |
| `FORJA_VISION` | 启用/禁用 vision（`true`/`false`） |
| `FORJA_BROWSER` | 启用/禁用 browser tool |
| `FORJA_INPUT` | 启用/禁用 input tool |

## Slash Commands

| Command | Description |
|---------|-------------|
| `/model <name>` | 切换 model |
| `/models` | 列出可用 model |
| `/mode <safe\|auto\|trust>` | 设置执行模式 |
| `/think <min\|mid\|max>` | 设置推理深度 |
| `/role <coder\|writer\|assistant\|analyst\|auto>` | 设置 role |
| `/ss [prompt]` | 屏幕捕获 + Vision 分析 |
| `/image <path> [prompt]` | 分析 image 文件 |
| `/help` | 显示可用 command |

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

OpenAI (API & OAuth)、Anthropic、Google Gemini (API & OAuth)、DeepSeek、Moonshot、xAI、GLM、Ollama。

运行时可使用 `/models` 查看所有可用的 model。

## Prompt Loading Order

1. Base prompt（identity、memory rules、core rules）
2. Think mode prompt（min/mid/max）
3. Role-specific prompt（coder/writer/assistant/analyst）
4. Tool descriptions
5. Emotion context
6. Relationship context
7. Knowledge context
8. Memory context（来自 memory.md）
9. User global prompt: `~/.forja/USER.md`
10. Project prompt: `AGENTS.md` `FORJA.md` `CLAUDE.md`

## Channels

- **CLI**：始终可用，并支持 streaming output
- **Telegram**：使用 bot token 启用。提供基于 whitelist 的 access control，并带有 typing indicators。

## License

MIT OR Apache-2.0

## Links

- [Repository](https://github.com/ai4wm/forja)
- [Architecture](ARCHITECTURE.md)
- [Roadmap](ROADMAP.md)
