# Forja Project Rules

## 작업 원칙
- 작업 전 반드시 현재 코드 구조를 파악하라
- 수정 전 영향 범위를 먼저 분석하라
- 한 번에 하나의 파일만 수정하라
- 플랜을 먼저 제출하고 승인 후 코딩하라

## 코딩 규칙
- Rust 코드 우선
- 모든 변경에 테스트 포함
- clippy 경고 0개 유지
- MODEL_TABLE과 models_for() 이중 관리 금지

## 금지 사항
- crates/forja-channel/src/multi.rs 수정 금지
- crates/forja-channel/src/telegram.rs 수정 금지
- config.toml 기존 API 키 삭제 금지
- engine.rs의 run_streaming, stream_step_with_tools UI 출력 로직 변경 금지

## 완료 기준
- cargo build --workspace 통과
- cargo clippy --workspace 경고 0개
- 기존 기능 정상 동작 확인
- git diff --name-only로 변경 파일 확인
