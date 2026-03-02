# Forja — The Most Powerful AI Agent Engine, Built in Rust

## 비전
OpenClaw를 넘어서는 세계 최강의 AI 에이전트 엔진.
시작은 초경량, 도착은 풀스펙 — 단, 무겁지 않게.

## 핵심 철학
- "가볍게 시작하되, 강력하게 완성한다"
- OpenClaw의 기능을 100% 커버하면서 1/10 리소스로 동작
- 모듈을 하나씩 추가해도 전체가 무거워지지 않는 레고 구조

## 목표 스펙
|  | OpenClaw | Forja 목표 |
|--|----------|-----------|
| 코드 | 430k LOC (TS) | <30k LOC (Rust) |
| 바이너리 | ~2GB+ (Node) | <30MB |
| RAM | 200-500MB | <100MB (풀스펙) |
| 기능 | 100% | 120% (초과 목표) |
| 플랫폼 | 서버 전용 | CLI + 데스크톱 + 모바일 + 서버 |

## 기능 로드맵 (OpenClaw 대비)

### Phase 1 — Core (OpenClaw 30%)
- LLM 멀티 프로바이더 (Claude, GPT, Gemini, Ollama)
- CLI 대화 인터페이스
- 기본 메모리 (마크다운 파일)

### Phase 2 — Parity (OpenClaw 100%)
- 멀티 채널 (Telegram, Discord, Slack, WhatsApp)
- 하이브리드 메모리 검색 (벡터 + BM25 + 시간 감쇠)
- 도구 실행 (쉘, 파일, 웹 스크래핑)
- 스케줄러 (cron 작업)
- 컨텍스트 압축 + 자동 메모리 플러시
- MCP 서버/클라이언트 지원

### Phase 3 — Beyond (OpenClaw 120%+)
- 네이티브 음성 입출력 (STT/TTS, Whisper + Piper)
- 멀티 에이전트 오케스트레이션 (에이전트 간 협업)
- Tauri v2 데스크톱 UI (StoryForge 연동)
- 모바일 지원 (Tauri v2 iOS/Android)
- WASM 플러그인 샌드박스 (서드파티 확장)
- 로컬 LLM 네이티브 통합 (llama.cpp 직접 바인딩)
- 실시간 스트리밍 UI (SSE/WebSocket)
- 엔드투엔드 암호화 메모리
- 자기 학습 메모리 (사용 패턴 분석 → 자동 최적화)
- 크리에이터 특화 스킬 (YouTube, SNS 자동화)

## 기술 스택
- 언어: Rust (edition 2021)
- 비동기: tokio
- HTTP: reqwest
- DB: SQLite → SurrealDB
- 벡터: sqlite-vec → qdrant
- 직렬화: serde + serde_json
- 음성: whisper-rs + piper-rs
- 로컬 LLM: llama-cpp-rs

## 워크스페이스 구조
- forja-core: Engine, Message, EventBus, Domain trait
- forja-llm: 멀티 LLM 프로바이더
- forja-memory: 하이브리드 메모리 시스템
- forja-channel: 멀티 채널 어댑터
- forja-tools: 도구 실행 + 샌드박스

## 설계 원칙
1. 모든 crate는 forja-core에만 의존
2. crate 간 직접 의존 금지
3. feature flag로 선택적 빌드 (미사용 기능 = 0 오버헤드)
4. trait 기반 추상화, 구현체 자유 교체
5. 최소 빌드는 항상 경량 유지
