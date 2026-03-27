[한국어](README.ko.md) | **日本語** | [中文](README.zh-CN.md) | [Español](README.es.md) | [Português](README.pt-BR.md) | [English](../README.md)

This document is a translation of the [English README](../README.md).

# Forja

Rustで構築された、軽量なクロスプラットフォーム AI agent engineです。

Forja はターミナルで動作する個人向け AI assistant です。自然言語だけで、過去の会話を記憶し、感情的な文脈を検出し、OS を操作し、スクリーンショットを解析し、推論の深さを調整します。

## Features

**マルチ provider LLM 対応**
OpenAI、Anthropic、Google Gemini、DeepSeek、Moonshot、xAI、GLM、またはローカル Ollama model に接続できます。`/model` を使ってランタイム中に provider と model を切り替えられます。

**永続 Memory**
markdown に保存される rolling memory システムです。Forja は再起動をまたいでも過去の会話を記憶し、「session」の境界がありません。

**Emotion と関係性の認識**
深夜の作業、長期間の不在、苛立ちなどの感情シグナルを検出し、自然にトーンを調整します。

**OS 操作**
- 安全確認付きの shell command 実行
- キーボードとマウス入力（type、click、scroll、hotkeys）
- CDP browser automation（navigate、click、type、read pages、take screenshots）
- 画面キャプチャ + GPT Vision 解析

**Smart Input**
- Drag-and-drop した image ファイルを即座に Vision 解析
- `/ss` で画面キャプチャ + 解析
- `/image <path>` でファイルベースの image 解析
- `\` continuation による multiline input

**適応型 Thinking**
3 つの推論モード: `/think min`（簡潔）、`/think mid`（デフォルト）、`/think max`（self-verification 付きの深い推論）。

**実行モード**
`/mode safe`（すべて確認）、`/mode auto`（危険なもののみ確認）、`/mode trust`（確認なし）。

**自動 Role 判定**
会話の文脈に応じて、coder、writer、assistant、analyst の prompt を自動で切り替えます。

**設定可能な Identity**
onboarding 中に assistant 名と user title を設定できます。ハードコードされた言語設定はなく、使う言語に合わせて応答します。

## Quick Start

### ソースからインストール

```bash
git clone https://github.com/ai4wm/forja.git
cd forja
cargo run
```

初回起動時には onboarding wizard が始まり、provider、assistant 名、設定を構成します。

### crates.io からインストール

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

設定ファイル: `~/.forja/config.toml`

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
| `FORJA_MODE` | 実行モード: `safe`、`auto`、`trust` |
| `FORJA_THINK` | Thinking レベル: `min`、`mid`、`max` |
| `FORJA_ASSISTANT_NAME` | assistant 名を上書き |
| `FORJA_USER_TITLE` | user title を上書き |
| `FORJA_PROVIDER` | LLM provider を上書き |
| `FORJA_MODEL` | model を上書き |
| `FORJA_USE_MOCK` | 実際の API 呼び出しなしで実行 |
| `FORJA_VISION` | vision を有効化/無効化 (`true`/`false`) |
| `FORJA_BROWSER` | browser tool を有効化/無効化 |
| `FORJA_INPUT` | input tool を有効化/無効化 |

## Slash Commands

| Command | Description |
|---------|-------------|
| `/model <name>` | model を切り替え |
| `/models` | 利用可能な model を一覧表示 |
| `/mode <safe\|auto\|trust>` | 実行モードを設定 |
| `/think <min\|mid\|max>` | 推論の深さを設定 |
| `/role <coder\|writer\|assistant\|analyst\|auto>` | role を設定 |
| `/ss [prompt]` | 画面キャプチャ + Vision 解析 |
| `/image <path> [prompt]` | image ファイルを解析 |
| `/help` | 利用可能な command を表示 |

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

利用可能なすべての model は、ランタイム中に `/models` で確認できます。

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

- **CLI**: 常に利用可能で、streaming output に対応
- **Telegram**: bot token で有効化します。typing indicator 付きの whitelist ベース access control を提供します。

## License

MIT OR Apache-2.0

## Links

- [Repository](https://github.com/ai4wm/forja)
- [Architecture](ARCHITECTURE.md)
- [Roadmap](ROADMAP.md)
