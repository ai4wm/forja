# Forja 로드맵

## 슬로건
"Start light. Finish strongest."

## Phase 1 — Core Engine (완료)
목표: CLI에서 AI와 대화 가능
- [x] forja-core: Message, Trait, Engine (런타임 루프)
- [x] forja-llm: OpenAI 통합 호환 클라이언트 기본 아키텍처
- [x] src/main.rs: 기본 터미널 대화형 루프 연동
- [x] 스트리밍 및 Chat 텍스트 응답 기반 마련

## Phase 2 — OpenClaw Parity (완료)
목표: OpenClaw의 핵심 기능을 Forja로 구현 완료 (초경량 아키텍처 기반)
- [x] forja-llm: OpenAI, Anthropic, Gemini, DeepSeek, XAI, GLM, 로컬 Ollama 통합 지원
- [x] forja-memory: BM25 기반 마크다운 메모리 관리 (`~/.forja/memory`) 및 자동 컨텍스트 정리
- [x] forja-channel: `CliChannel` 구현 (CLI 환경 입력/출력 인터페이스)
- [x] forja-tools: 시스템 파일 읽기/쓰기, 쉘 명령어 실행(화이트리스트 자동 승인 지원), 웹 페이지 수집
- [x] UX 개선: 대화형 설정 온보딩, 스트리밍 타이핑 출력, `--setup` `--provider` 등 CLI 플래그 연동

## Phase 3 — OpenClaw 초월 (8주)
목표: OpenClaw에 없는 기능으로 차별화
- [ ] Tauri v2 데스크톱 UI
- [ ] 음성 입출력 (Whisper + Piper)
- [ ] 로컬 LLM (llama.cpp 바인딩)
- [ ] 멀티 에이전트 오케스트레이션
- [ ] 모바일 (iOS/Android)
- [ ] WASM 플러그인 시스템
- [ ] 암호화 메모리
- [ ] 크리에이터 스킬 (YouTube, SNS)

## Phase 4 — 생태계 (지속)
- [ ] crates.io 정식 릴리즈
- [ ] 플러그인 마켓플레이스
- [ ] 커뮤니티 + 문서
- [ ] 엔터프라이즈 기능
