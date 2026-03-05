use async_trait::async_trait;

/// 시스템 명령어나 위험한 작업을 실행하기 전
/// 사용자에게 명시적인 승인(Confirmation)을 묻는 추상 인터페이스입니다.
#[async_trait]
pub trait ConfirmationHandler: Send + Sync {
    /// 해당 명령어(또는 내용)을 실행할지 여부를 반환합니다.
    /// true 이면 승인, false 이면 거절을 뜻합니다.
    async fn confirm(&self, cmd: &str) -> bool;
}

/// CLI 타겟 환경을 위한 기본 구현체.
/// 표준 입출력(Stdin/Stdout)을 사용하여 [y/N] 프롬프트를 띄웁니다.
pub struct StdinConfirmation;

impl StdinConfirmation {
    pub fn new() -> Self {
        Self
    }
}

impl Default for StdinConfirmation {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ConfirmationHandler for StdinConfirmation {
    async fn confirm(&self, cmd: &str) -> bool {
        // tokio::io를 사용하지 않고도 블로킹 I/O로 빠르게 처리 가능
        // LLM이 멈춘 상태이고 터미널 독점 입력을 기다려야 하니 큰 문제는 없음
        println!("\n⚠️  [SECURITY] The AI wants to execute the following command:");
        println!("> {}", cmd);
        println!("Allow? [y/N]: ");

        let mut input = String::new();
        if std::io::stdin().read_line(&mut input).is_ok() {
            // "y", "Y", "yes" 등에 반응 (기본은 N - 거절 정책)
            return input.trim().eq_ignore_ascii_case("y") || input.trim().eq_ignore_ascii_case("yes");
        }
        
        false
    }
}
