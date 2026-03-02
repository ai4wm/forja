# Forja — Lightweight Rust AI Agent Engine

## 프로젝트 목적
- OpenClaw(TS, 430k LOC, 500MB RAM) 대체
- 목표: <10k LOC, <20MB binary, <50MB RAM

## 기술 스택
- 언어: Rust (edition 2021)
- 비동기: tokio
- HTTP: reqwest
- DB: SQLite (rusqlite) → 추후 SurrealDB
- 직렬화: serde + serde_json

## 워크스페이스 구조
- forja-core: Message, EventBus, Domain trait
- forja-llm: LLM 프로바이더 (Anthropic, OpenAI, Ollama)
- forja-memory: 마크다운 파일 + 벡터 검색
- forja-channel: CLI, Telegram, Discord
- forja-tools: 쉘 명령, 파일 조작
- src/main.rs: CLI 바이너리

## 설계 원칙
- 모든 crate는 forja-core에만 의존
- crate 간 직접 의존 금지
- feature flag로 선택적 빌드
- trait 기반 추상화, 구현체 교체 가능
