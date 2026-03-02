# Forja 로드맵

## 슬로건
"Start light. Finish strongest."

## Phase 1 — Core Engine (2주)
목표: CLI에서 AI와 대화 가능
- [ ] forja-core: Message, EventBus, Engine
- [ ] forja-llm: Anthropic 프로바이더
- [ ] src/main.rs: `forja chat "질문"` 동작
- [ ] 기본 스트리밍 응답

## Phase 2 — OpenClaw 동등 (4주)
목표: OpenClaw이 하는 모든 것을 Forja로 가능
- [ ] forja-llm: OpenAI, Gemini, Ollama 추가
- [ ] forja-memory: 마크다운 + 벡터 검색
- [ ] forja-channel: Telegram, Discord
- [ ] forja-tools: 쉘 실행, 파일 조작
- [ ] 컨텍스트 압축 + 메모리 플러시
- [ ] MCP 지원
- [ ] 스케줄러

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
