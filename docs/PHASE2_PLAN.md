# Phase 2: OpenClaw Parity 구현 계획

Phase 2의 핵심 목표는 Forja 프레임워크가 기존 OpenClaw의 모든 핵심 기능을 대체할 수 있도록 **기능적 동등성(Parity)** 을 확보하는 것입니다.
각 컴포넌트별 상세 구현 계획 및 진행 순서는 다음과 같습니다.

## 1. 모듈별 구현 계획

# Phase 2 구현 결과 (완료)

## 1. 개요 및 진행 결과
Phase 2 목표인 'OpenClaw Parity'를 성공적으로 달성했습니다.
모든 핵심 크이트(`forja-core`, `forja-llm`, `forja-memory`, `forja-tools`, `forja-channel`)가 연동되었으며 `cargo build --workspace`를 통과합니다.

## 2. 세부 항목 체크리스트
- [x] **A. forja-llm 프리셋 확장**: 11개 프로바이더 프리셋 및 유연한 Config 빌더 구현
- [x] **B. forja-memory 하이브리드 검색**: BM25 기반 검색 및 마크다운 자동 저장
- [x] **C. forja-tools 도구 구현**: Shell, File, Web 검색 도구 및 실행 파이프라인
- [x] **D. forja-channel 입출력**: CliChannel 구현 및 스트리밍 실시간 출력 UX
- [x] **E. 컨텍스트 관리**: 토큰 기반 Auto-Flush 및 System Prompt 주입/지속
- [x] **F. 설정 및 온보딩**: `config.toml` 자동 관리 및 대화형 온보딩 (설정 마법사)

---

## 3. 진행 중인 작업 (Phase 3 전환 준비)
- [ ] `keys` 및 `active` 분리 구조의 고급 설정 관리 (기획안 승인됨)
- [ ] Telegram/Discord 채널 본인 인증 및 서버 세팅 (Phase 2 선택)
- [ ] Tauri v2 데스크톱 UI 프로토타입 설계

---

## 2. 작업 진행 순서 (제안)

본 프로젝트는 구조 변경이 쉬운 부분부터 살을 붙여나가는 것을 목적으로 합니다.
1. **`forja-memory` 구현**: AI의 지속적인 대화 컨텍스트를 유지하기 위함 (가장 시급)
2. **`forja-tools` 구현**: CLI 채널 안에서 파일도 열어보고 쉘도 쳐보게 권한 부여
3. **`forja-core` 고도화**: 컨텍스트 윈도우 한계가 오기 시작하므로 압축 및 플러시 메커니즘 개발
4. **`forja-channel` 연동**: 외부 메신저 접근 권한 부여
5. **Phase 2 마무리**: 종합 테스트 및 `forja-llm` 여타 설정 통합
6. **Phase 2 마무리**: 종합 테스트 및 `forja-llm`의 나머지 프리셋 완성

> 승인이 완료되면 가장 먼저 **`forja-memory/plan.md`** 를 별도로 상세 작성하여 메모리 스토어 기획에 돌입할 수 있습니다.
