# Forja

Lightweight Rust AI Agent Engine

Forja는 Rust 워크스페이스로 구성된 경량 AI 에이전트 엔진입니다. 현재 저장소는 CLI 중심 런타임, 멀티 프로바이더 LLM 라우팅, 도구 실행, 대화형 설정, 모델 전환, 선택적 Telegram 채널을 포함합니다.

자세한 구조는 [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md), 진행 방향은 [docs/ROADMAP.md](docs/ROADMAP.md)를 참고하세요.

## 현재 상태

현재 코드 기준으로 다음 기능이 구현되어 있습니다.

- 멀티 LLM 프로바이더: OpenAI API, OpenAI OAuth, Anthropic, Gemini API, Gemini OAuth, DeepSeek, GLM, Moonshot, xAI, Ollama
- 채널: CLI 기본, Telegram 선택 지원
- 도구: `FileTool`, `WebTool`, `ShellTool`, `SearchTool`
- 외부 CLI 브리지 도구: `ClaudeCodeTool`, `CodexTool`, `GeminiCliTool`
- 런타임 기능: 토큰 스트리밍 출력, 슬래시 명령(`/models`, `/model`), 프로젝트 프롬프트 자동 로드, 대화형 설정 위저드

주의:

- `forja-memory` 크레이트 자체는 구현되어 있지만, 현재 top-level 바이너리에서는 메모리 스토어 연결 코드가 비활성화되어 있습니다.
- Telegram은 바이너리에 포함되어도 토큰이 없으면 자동으로 CLI 전용 모드로 동작합니다.

## 워크스페이스 구성

```text
forja                CLI 바이너리, 설정 로드, 채널/도구/프롬프트 조립
forja-core           엔진 루프, ToolCall 처리, 스트리밍, 슬래시 명령
forja-llm            멀티 프로바이더 LLM 클라이언트와 프리셋
forja-memory         Markdown + BM25 기반 메모리 스토어
forja-tools          파일/웹/쉘/검색 및 외부 CLI 브리지 도구
forja-channel        CLI / Telegram 멀티 채널 입력·출력
```

## 지원 프로바이더

현재 `src/provider_registry.rs` 기준 활성 모델 테이블은 아래 계열을 포함합니다.

- OpenAI API: `gpt-5.4`, `gpt-5.4-mini`, `gpt-5.3-codex`
- OpenAI OAuth: `gpt-5.4`, `gpt-5.3-codex`, `gpt-5.3-codex-spark`, `o3-pro`
- Anthropic: `claude-opus-4-6`, `claude-sonnet-4-6`
- Gemini API: `gemini-3.1-pro-preview`, `gemini-3-flash-preview`, `gemini-3.1-flash-lite-preview`, `gemini-2.5-pro`, `gemini-2.5-flash`
- Gemini OAuth: `gemini-3.1-pro-preview`, `gemini-3-flash-preview`, `gemini-2.5-pro`, `gemini-2.5-flash`
- DeepSeek: `deepseek-chat`, `deepseek-reasoner`
- GLM: `glm-5`, `glm-4.5v`
- Moonshot: `kimi-k2.5`
- xAI: `grok-3`, `grok-3-mini`
- Ollama: `qwen3.5:9b`, `llama3:8b`, `mistral:7b`

실행 중 `/models`로 현재 사용 가능한 모델 목록을 보고 `/model <번호|이름|별칭>`으로 전환할 수 있습니다.

## 빠른 시작

### 1. 실행

- `cargo run`
- 설정이 없으면 자동 온보딩이 시작됩니다.

### 2. 설정 다시 실행

- `cargo run -- setup`
- `cargo run -- --setup`

두 방식 모두 현재 코드에서 동작합니다.

### 3. 로그인

OAuth 또는 토큰 저장이 필요한 경우:

- `cargo run -- login openai`
- `cargo run -- login gemini`
- `cargo run -- login anthropic`

### 4. 실행 중 프로바이더/모델 변경

- `cargo run -- --provider moonshot`
- `cargo run -- --model kimi-k2.5`

### 5. 모의 실행

실제 API 호출 없이 런타임만 확인하려면:

```powershell
$env:FORJA_USE_MOCK = "1"
cargo run
```

## 설정 파일

기본 설정 파일 위치:

- `~/.forja/config.toml`

현재 코드 기준 주요 구조:

```toml
[active]
provider = "moonshot"
model = "kimi-k2.5"

[keys]
openai = "..."
anthropic = "..."
gemini = "..."
deepseek = "..."
glm = "..."
moonshot = "..."
xai = "..."

[channel.telegram]
bot_token = "123456:token"
allowed_chat_ids = [123456789]

[tools.search]
provider = "duckduckgo" # duckduckgo | brave | grok
brave_api_key = ""
xai_api_key = ""
```

환경 변수 오버라이드:

- `FORJA_PROVIDER`
- `FORJA_MODEL`
- `FORJA_API_KEY`
- `FORJA_SYSTEM_PROMPT`
- `FORJA_USE_MOCK`
- `TELEGRAM_BOT_TOKEN`

## 프롬프트 로드 순서

런타임은 아래 순서로 프롬프트를 합쳐 시스템 프롬프트로 주입합니다.

1. 사용자 전역 프롬프트: `~/.forja/USER.md`
2. 프로젝트 프롬프트: `AGENTS.md` -> `FORJA.md` -> `CLAUDE.md`

프로젝트 프롬프트가 존재하면 현재 날짜 정보도 함께 주입됩니다.

## 도구

기본 등록 도구:

- `FileTool`: 파일 읽기/쓰기
- `WebTool`: HTTP GET 기반 본문 수집
- `ShellTool`: 사용자 확인 기반 로컬 명령 실행
- `SearchTool`: DuckDuckGo, Brave, xAI Grok 웹 검색

설치되어 있을 때만 동적으로 등록되는 도구:

- `ClaudeCodeTool`
- `CodexTool`
- `GeminiCliTool`

## 채널

- CLI: 항상 사용 가능
- Telegram: `bot_token` 또는 `TELEGRAM_BOT_TOKEN`이 있을 때 `CLI + Telegram` 멀티채널로 실행

Telegram이 활성화되면 허용된 `chat_id`만 처리하며, 응답 중 타이핑 인디케이터를 전송합니다.

## 개발 메모

- 메인 진입점: `src/main.rs`
- 설정 로직: `src/config.rs`
- 모델 레지스트리: `src/provider_registry.rs`
- 엔진 코어: `crates/forja-core/src/engine.rs`
- LLM 프리셋: `crates/forja-llm/src/presets.rs`

## 문서

- [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)
- [docs/ROADMAP.md](docs/ROADMAP.md)
- [docs/RESEARCH.md](docs/RESEARCH.md)
