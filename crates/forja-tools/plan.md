# forja-tools 구현 계획

## 1. 개요
`forja-tools`는 Forja 엔진이 외부 환경과 상호작용하기 위한 도구(Tool) 모음을 제공합니다. Phase 2 목표에 따라 **파일 조작(File)**, **웹 크롤링(Web)**, **시스템 명령어 실행(Shell)** 도구를 구현합니다. 

특히 보안 강화를 위해, Shell 명령어 실행 시 **사용자 승인(Confirmation) 또는 화이트리스트 기반의 확인 절차**를 필수로 거치도록 설계합니다.

---

## 2. Tool Trait 구현 방식 (인터페이스 연동)

모든 도구는 `forja_core::traits::Tool`을 구현해야 합니다. JSON 직렬화를 통해 MCP 프로토콜 및 LLM Function Calling 규격과 자연스럽게 호환되도록 구성합니다.

```rust
use async_trait::async_trait;
use forja_core::traits::Tool;
use forja_core::error::Result;
use serde_json::Value;

// Tool trait 원본 (forja_core 참고용)
// pub trait Tool: Send + Sync {
//     fn name(&self) -> &str;
//     async fn execute(&self, args: Value) -> Result<Value>;
// }
```

---

## 3. 도구별 세부 설계

### 3.1. FileTool (파일 조작 체계)
로컬 파일시스템을 제어하기 위한 범용 파일 I/O 도구입니다. 운영체제의 제약이나 권한 문제를 처리해야 합니다.
- `name`: `"file_tool"`
- **기능 분기**: `args["action"]` 필드에 따라 분기
    - `read`: 특정 절대/상대 경로의 파일 내용 텍스트 반환
    - `write`: 전체 텍스트 내용 덮어쓰기 생성
    - *(참고: 데이터 구조의 문맥을 파악한 형상 `modify` 단축 수정 기능은 Phase 3으로 이동)*

### 3.2. WebTool (웹 스크래핑 체계)
간단한 웹문서 정보를 수집하기 위한 도구입니다. 무거운 브라우저 자동화(Headless) 대신 `reqwest`를 이용해 가볍게 출발합니다.
- `name`: `"web_tool"`
- **기능 분기**: 
    - URL을 받아 단순히 `GET` 요청 수행
    - HTTP body 텍스트 반환 (필요시 정규식 등으로 HTML 태그를 날리는 경량화 처리)

### 3.3. ShellTool (명령어 실행 및 보안 체계) ★ 핵심
운영체제의 CLI 파이프를 이용해 명령어를 실행합니다. **엔진 폭주 방지 방어벽**이 필수적으로 갖춰져야 합니다.

- `name`: `"shell_tool"`
- **보안 메커니즘 설계**:
  - LLM이 호출하려는 전체 시스템 명령어 권한을 통제하기 위해, **추상화된 ConfirmationHandler Trait**를 주입받아 사용합니다.
  - 실행 전 차단 목록(명시적 금지 명령어)이나 정책 검사 단계를 통과해야 합니다.
  - 이를 통과하더라도 `handler.confirm(cmd).await`를 호출해 승인을 득한 뒤에만 CLI를 실행합니다. 거절 시 `Error::ToolError("User rejected the command")` 형태로 반환합니다.

**[코드 스니펫: ConfirmationHandler 추상화 및 CLI 구현체 예시]**
```rust
#[async_trait]
pub trait ConfirmationHandler: Send + Sync {
    async fn confirm(&self, cmd: &str) -> bool;
}

/// CLI 환경용 표준입출력(Stdin) 확인 구현체
pub struct StdinConfirmation;

#[async_trait]
impl ConfirmationHandler for StdinConfirmation {
    async fn confirm(&self, cmd: &str) -> bool {
        // tokio::io 또는 std::io를 사용하여 
        // [y/N] 입력을 받고, y 응답인 경우만 true 반환 
        println!("\n⚠️ [SECURITY] The AI wants to execute the following command:");
        println!("> {}\nAllow? [y/N]: ", cmd);
        
        let mut input = String::new();
        if std::io::stdin().read_line(&mut input).is_ok() {
            return input.trim().eq_ignore_ascii_case("y");
        }
        false
    }
}
```

---

## 4. 구현 진행 순서 (작업 Step)

1.  **Cargo.toml 셋업**: `reqwest` (web), `tokio` (fs 쓰기, process 생성), `serde_json`, `forja-core` 의존성 구성.
2.  **FileTool 구현 (`file.rs`)**: `tokio::fs` 기반 파일 읽기/쓰기 구현(명령 라우팅 및 파일 존재 여부 예외 처리).
3.  **WebTool 구현 (`web.rs`)**: `reqwest` HTTP GET 요청 래핑 및 본문 추출 로직 구성.
4.  **ShellTool 구현 (`shell.rs`)**: 핵심 기능. `std::process::Command` 또는 `tokio::process::Command` 래핑, 그리고 `request_confirmation` 로직(프롬프트) 작성.
5.  **통합 (`lib.rs`)**: 모듈들을 익스포트하여, `forja` main 앱 또는 `Engine`이 `Arc<dyn Tool>` 벡터로 등록할 수 있게끔 인터페이스 개방.
6.  **테스트**: 각 도구별 Mocking 응답 확인 및 Shell y/n 대기 엣지케이스 검증.
