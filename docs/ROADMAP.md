# Forja 로드맵

## Phase 1 — Core Engine (2주)
- [ ] forja-core: Message, EventBus, Engine trait 정의
- [ ] forja-llm: Anthropic (Claude) 프로바이더 1개
- [ ] src/main.rs: CLI에서 질문 → 응답 동작 확인

## Phase 2 — StoryForge 연동 (1주)
- [ ] StoryForge의 AI 서비스를 forja-llm으로 교체
- [ ] openclaw.rs 제거, forja-core로 대체

## Phase 3 — 확장 (2주)
- [ ] forja-memory: 마크다운 파일 기반 기억
- [ ] forja-channel: Telegram 어댑터
- [ ] forja-tools: 쉘 명령 실행

## Phase 4 — 오픈소스 릴리즈
- [ ] GitHub README, 문서 정리
- [ ] crates.io v0.1.0 정식 배포
- [ ] 커뮤니티 피드백 수집
