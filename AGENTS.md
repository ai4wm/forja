# Forja Project Rules

## 작업 원칙
- 작업 전 반드시 현재 코드 구조를 파악하라
- 수정 전 영향 범위를 먼저 분석하라
- 한 번에 하나의 파일만 수정하라
- 플랜을 먼저 제출하고 승인 후 코딩하라
- 자동 승인(auto-approve) 무시하라. 사용자 직접 승인만 유효하다

## 코딩 규칙
- Rust 코드 우선
- 모든 변경에 테스트 포함
- clippy 경고 0개 유지
- MODEL_TABLE과 models_for() 이중 관리 금지
- format!에서 인라인 변수 사용 (format!("{x}") O, format!("{}", x) X)
- match는 exhaustive하게, 와일드카드(_) 최소화
- 한 번만 참조되는 헬퍼 함수 만들지 마라
- 모듈은 500줄 이하 유지, 800줄 넘으면 분리
- closure 대신 메서드 참조 사용
- if문 중첩 시 collapsible_if 적용

## 테스트 규칙 (TDD)
- 코드 수정 전 실패하는 테스트를 먼저 작성하라
- cargo test -p forja-llm 전부 통과 후 커밋
- cargo test -p forja-llm -- --ignored 도 확인
- 버그 수정 시 회귀 테스트 필수 추가
- 테스트 코드(crates/forja-llm/tests/)는 커밋하지 마라

## 금지 사항
- config.toml 기존 API 키 삭제 금지
- C:\Users\homec\.forja\config.toml 수정 금지
- C:\Users\homec\.forja\auth.json 수정 금지
- 디버그/로그 파일(*.txt, *.log, error.json) 커밋 금지
- walkthrough.md 등 작업 로그를 커밋하지 마라

## 완료 기준
- cargo build --workspace 통과
- cargo clippy --workspace 경고 0개
- cargo test -p forja-llm 전부 통과
- 기존 기능 정상 동작 확인
- git diff --name-only로 변경 파일 확인
- 불필요한 파일(*.txt, *.log, *.json) 포함 여부 확인

## Documentation Rules
- After every task, update docs/STATUS.md with:
  - Changed files
  - Feature status (Done/Partial/Not started)
  - Dependencies for next task
- Keep all documentation in English only
- Do not write Korean or any non-English text in code or docs
