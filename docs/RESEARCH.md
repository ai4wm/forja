# Forja 리서치 자료

## 1. 경쟁 분석 — OpenClaw 대안들
| 프로젝트 | 언어 | 바이너리 | RAM | 특징 |
|----------|------|----------|-----|------|
| OpenClaw | TypeScript | ~2GB+ | 200-500MB | 원본, 풀스펙 |
| ZeroClaw | Rust | 3.4MB | 5MB | 초경량 |
| IronClaw | Rust | ~20MB | ~30MB | WASM 샌드박스 |
| Moltis | Rust | ~60MB | ~50MB | 음성/WebUI |
| OpenCrust | Rust | 17MB | ~25MB | 충실한 포트 |
| PicoClaw | Go | 경량 | 경량 | 라즈베리파이용 |
| Nanobot | Python | - | - | 4000줄 미니멀 |
| NanoClaw | TypeScript | - | - | 4000줄, 컨테이너 |

## 2. Go vs Rust 비교 결론
- 성능 차이 미미 (병목은 LLM API 지연)
- Rust 선택 이유: 미래지향성, 기업 채택률 40-68% YoY 성장,
  Linux 커널 공식 언어, AI 코드 생성 품질 향상
- AI가 Rust 100k줄 생성 가능 (2026 기준)
- Rust 컴파일러 피드백 → AI 자동 수정 루프

## 3. OpenClaw 아키텍처 분석
- 3계층: Channel → Gateway → Agent
- TypeScript/Node.js, pnpm 모노레포
- 메모리: MEMORY.md + 일별 로그 + 벡터 검색
- 약점: 싱글스레드, 의존성 지옥, 메모리 비효율

## 4. Tauri v2 연동 전략
- StoryForge(Tauri v2 앱)에 forja-core를 crate로 임베드
- WASM 플러그인은 v1에서 불필요, 커뮤니티 성장 후 도입
- Sidecar vs crate 임베드 → crate 임베드 추천

## 5. 메모리 시스템 비교
| | Claude Code | OpenClaw |
|--|------------|----------|
| 구조 | CLAUDE.md + Auto Memory | MEMORY.md + 일별 로그 |
| 검색 | 없음 | 벡터 + BM25 하이브리드 |
| 비용 | $6/일 (구독) | $50-300/월 (토큰) |

## 6. 참고 링크
- OpenClaw: https://github.com/openclaw/openclaw
- ZeroClaw: https://github.com/zeroclaw-labs/zeroclaw
- IronClaw: https://github.com/nearai/ironclaw
- Moltis: https://github.com/moltis-org/moltis
- Wails (Go 데스크톱): https://wails.io
- Tauri v2: https://v2.tauri.app

## 7. OpenClaw 기능 완전 목록 (Forja 커버 대상)

### 필수 구현 (Phase 2)
- [x] 멀티 LLM 프로바이더
- [ ] 채널 게이트웨이 (라우팅, 세션 관리)
- [ ] 퍼시스턴트 메모리 (MEMORY.md + 일별 로그)
- [ ] 시맨틱 검색 (임베딩 + 벡터 DB)
- [ ] 컨텍스트 컴팩션 (토큰 절약)
- [ ] 메모리 자동 플러시 (컴팩션 전 저장)
- [ ] 도구 실행 (shell, file, web fetch)
- [ ] 스케줄링 (cron 기반 반복 작업)
- [ ] MCP 프로토콜 지원

### OpenClaw에 없는 것 (Phase 3 — Forja 차별화)
- [ ] 네이티브 데스크톱 UI (Tauri v2)
- [ ] 모바일 앱 (Tauri v2 iOS/Android)
- [ ] 음성 대화 (로컬 STT/TTS)
- [ ] 로컬 LLM 직접 실행 (llama.cpp)
- [ ] 멀티 에이전트 협업
- [ ] WASM 플러그인 마켓플레이스
- [ ] 암호화 메모리
- [ ] 자기 학습 메모리
- [ ] 크리에이터 툴킷 (YouTube API, SNS 자동화)
- [ ] 단일 바이너리 배포 (Node.js 불필요)
- [ ] 라즈베리파이 / 엣지 디바이스 실행
