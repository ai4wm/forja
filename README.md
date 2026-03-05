# Forja
Lightweight Rust AI Agent Engine

OpenClaw의 대안으로 개발되는 초경량 텍스트/음성 AI 에이전트 엔진입니다. 
자세한 기술 문맥 및 설계 원칙은 [CLAUDE.md](CLAUDE.md) 및 [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)를 참고하세요.

## 현재 진행 상황 (Phase 2 완료)
- **멀티 LLM**: OpenAI, Anthropic, Gemini, DeepSeek, XAI, Moonshot, GLM 7개 메이저 프로바이더 및 로컬 Ollama 공식 지원.
- **RAG/메모리**: BM25 단어 단위 기반의 마크다운 메모리 관리와 자동 컨텍스트 비우기 기능(`forja-memory`).
- **Tool Use**: `FileTool`(읽기/쓰기), `WebTool`(reqwest GET), `ShellTool`(기본 명령어 자동 승인 및 대화형 사용자 확인).
- **UX**: 토큰 단위 실시간 스트리밍 출력 및 타이핑 폴백 지원.
- **설정 및 실행**: 터미널 기반 대화형 온보딩, `config.toml` 상태 관리, 그리고 다양한 CLI 플래그(`--setup`, `--provider`, `--model`) 지원.

### 사용 방법

- `cargo run` : 기본 실행 (설정 파일 누락 시 자동 온보딩 진입)
- `cargo run -- --setup` : 설정(API 키, 모델 등) 초기화 및 재구성
- `cargo run -- --provider <이름>` : 특정 프로바이더(예: `moonshot`, `anthropic`)로 임시 전환하여 실행
- `cargo run -- --model <이름>` : 특정 모델(예: `kimi-k2.5`)로 임시 전환하여 실행

> 환경 변수 제어: `FORJA_USE_MOCK=1` 지정 시 LLM API 호출 없이 에코 응답만 반환하는 로컬 테스트 모드로 진입합니다.

### 주요 기능
- **실시간 스트리밍**: LLM의 응답이 타이핑되듯 실시간으로 출력됩니다.
- **도구(Tool) 자동 실행**: 에이전트가 필요 시 파이썬 쉘, 파일 시스템, 웹 검색 등을 스스로 판단하여 사용합니다.
- **자동 메모리 관리**: 대화가 길어지면 자동으로 중요 내용을 요약하여 저장하고 컨텍스트를 비워 효율을 유지합니다.
