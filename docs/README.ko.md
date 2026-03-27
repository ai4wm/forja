**한국어** | [日本語](README.ja.md) | [中文](README.zh-CN.md) | [Español](README.es.md) | [Português](README.pt-BR.md) | [English](../README.md)

This document is a translation of the [English README](../README.md).

# Forja

Rust로 구축된 가볍고 크로스플랫폼 AI agent 엔진입니다.

Forja는 터미널에서 동작하는 개인용 AI assistant입니다. 자연어만으로 과거 대화를 기억하고, 감정적 맥락을 감지하며, OS를 제어하고, 스크린샷을 분석하고, 추론 깊이를 조절합니다.

## Features

**멀티 프로바이더 LLM 지원**
OpenAI, Anthropic, Google Gemini, DeepSeek, Moonshot, xAI, GLM 또는 로컬 Ollama 모델에 연결할 수 있습니다. `/model`로 런타임에 provider와 model을 전환할 수 있습니다.

**지속형 Memory**
markdown에 저장되는 rolling memory 시스템입니다. Forja는 재시작 후에도 과거 대화를 기억하며 "session" 경계가 없습니다.

**Emotion 및 관계 인식**
늦은 밤 작업, 오랜 부재, 좌절감 같은 감정 신호를 감지하고 자연스럽게 말투를 조정합니다.

**OS 제어**
- 안전 확인이 포함된 shell command 실행
- 키보드 및 마우스 입력 (type, click, scroll, hotkeys)
- CDP browser 자동화 (navigate, click, type, read pages, take screenshots)
- 화면 캡처 + GPT Vision 분석

**Smart Input**
- Drag-and-drop image 파일로 즉시 Vision 분석
- `/ss`로 화면 캡처 + 분석
- `/image <path>`로 파일 기반 image 분석
- `\` continuation을 사용하는 multiline input

**적응형 Thinking**
세 가지 추론 모드: `/think min` (간결), `/think mid` (기본값), `/think max` (self-verification이 포함된 심층 추론).

**실행 모드**
`/mode safe` (모든 것 확인), `/mode auto` (위험한 것만 확인), `/mode trust` (확인 없음).

**자동 역할 감지**
대화 맥락에 따라 coder, writer, assistant, analyst 프롬프트 사이를 자동으로 전환합니다.

**설정 가능한 Identity**
온보딩 중 assistant 이름과 user title을 설정할 수 있습니다. 하드코딩된 언어가 없으며, 사용자가 쓰는 언어에 맞춰 응답합니다.

## Quick Start

### 소스에서 설치

```bash
git clone https://github.com/ai4wm/forja.git
cd forja
cargo run
```

처음 실행하면 onboarding wizard가 시작되어 provider, assistant 이름, 환경설정을 구성합니다.

### crates.io에서 설치

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

설정 파일: `~/.forja/config.toml`

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
| `FORJA_MODE` | 실행 모드: `safe`, `auto`, `trust` |
| `FORJA_THINK` | Thinking 수준: `min`, `mid`, `max` |
| `FORJA_ASSISTANT_NAME` | assistant 이름 재정의 |
| `FORJA_USER_TITLE` | user title 재정의 |
| `FORJA_PROVIDER` | LLM provider 재정의 |
| `FORJA_MODEL` | model 재정의 |
| `FORJA_USE_MOCK` | 실제 API 호출 없이 실행 |
| `FORJA_VISION` | vision 활성화/비활성화 (`true`/`false`) |
| `FORJA_BROWSER` | browser tool 활성화/비활성화 |
| `FORJA_INPUT` | input tool 활성화/비활성화 |

## Slash Commands

| Command | Description |
|---------|-------------|
| `/model <name>` | model 전환 |
| `/models` | 사용 가능한 model 목록 표시 |
| `/mode <safe\|auto\|trust>` | 실행 모드 설정 |
| `/think <min\|mid\|max>` | 추론 깊이 설정 |
| `/role <coder\|writer\|assistant\|analyst\|auto>` | 역할 설정 |
| `/ss [prompt]` | 화면 캡처 + Vision 분석 |
| `/image <path> [prompt]` | image 파일 분석 |
| `/help` | 사용 가능한 command 표시 |

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

모든 사용 가능한 model은 런타임에 `/models`로 확인할 수 있습니다.

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

- **CLI**: 항상 사용 가능하며, streaming output을 지원합니다
- **Telegram**: bot token으로 활성화합니다. typing indicator가 포함된 whitelist 기반 access control을 제공합니다.

## License

MIT OR Apache-2.0

## Links

- [Repository](https://github.com/ai4wm/forja)
- [Architecture](ARCHITECTURE.md)
- [Roadmap](ROADMAP.md)
