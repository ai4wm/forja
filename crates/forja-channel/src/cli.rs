use async_trait::async_trait;
use forja_core::error::{ForjaError, Result};
use forja_core::traits::Channel;
use forja_core::types::{Content, Message, Role};
use std::io::Write;
use tokio::io::{self, AsyncBufReadExt, BufReader};

/// 표준 입력(stdin)과 표준 출력(stdout)을 사용하는 로컬 CLI 채널 구현체.
pub struct CliChannel;

impl CliChannel {
    pub fn new() -> Self {
        Self
    }
}

impl Default for CliChannel {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Channel for CliChannel {
    /// 터미널에서 사용자의 입력을 대기하고 읽어들여 User Role의 메시지로 반환합니다.
    async fn receive(&self) -> Result<Message> {
        // 프롬프트 출력 (비동기 대신 표준 io::Write 사용 후 flush)
        print!("> ");
        if let Err(e) = std::io::stdout().flush() {
            return Err(ForjaError::ChannelError(format!("Stdout flush failed: {}", e)));
        }

        let stdin = io::stdin();
        let mut reader = BufReader::new(stdin);
        let mut line = String::new();

        let bytes_read = reader.read_line(&mut line).await
            .map_err(|e| ForjaError::ChannelError(format!("Failed to read stdin: {}", e)))?;

        if bytes_read == 0 {
            // EOF 도달 시 (예: Ctrl+D)
            return Err(ForjaError::ChannelError("EOF reached".to_string()));
        }

        let trimmed = line.trim().to_string();
        
        // 빈 입력은 무시하고 에러를 뱉기보단 다시 재귀호출 또는 래핑할 수 있으나,
        // 여기서는 엔진이 바로 재시도할 수 있도록 빈 메시지를 리턴합니다.
        Ok(Message::text(Role::User, trimmed, None))
    }

    /// 엔진이 생성한 메시지(Assistant 또는 System)를 채널(터미널)에 시각적으로 출력합니다.
    async fn send(&self, msg: Message) -> Result<()> {
        match msg.role {
            Role::Assistant => {
                // Assistant의 순수 텍스트 결과만 출력 (ToolCall은 보통 내부 처리됨)
                if let Content::Text { text, .. } = msg.content {
                    println!("\n🤖 Assistant: {}\n", text);
                }
            }
            Role::System => {
                if let Content::Text { text, .. } = msg.content {
                    println!("⚙️ System: {}", text);
                }
            }
            Role::Tool => {
                // 도구의 실행 결과는 일반적으로 디버그 또는 생략하지만, 확인용으로 출력
                if let Content::ToolResult { call_id, result } = msg.content {
                    println!("🔧 [Tool call '{}' result]: {}", call_id, result);
                }
            }
            Role::User => {
                // User가 보낸 메시지는 자기 자신의 터미널엔 이미 보였으므로 No-Op 처리
            }
        }
        Ok(())
    }
}
