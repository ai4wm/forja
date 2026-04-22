use async_trait::async_trait;
use forja_core::error::{ForjaError, Result};
use forja_core::gateway::adapter::{ChannelAdapter, CliAdapter};
use forja_core::traits::Channel;
use forja_core::types::{Content, Message, Role};
use std::io::Write;
use tokio::io::{self, AsyncBufReadExt, BufReader};

/// Local CLI channel implementation using stdin and stdout.
pub struct CliChannel;

pub fn process_line(line: &str, buffer: &mut String) -> bool {
    if let Some(content) = line.strip_suffix('\\') {
        buffer.push_str(content);
        buffer.push('\n');
        return true;
    }

    buffer.push_str(line);
    false
}

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
    /// Waits for user input from the terminal and returns it as a User role message.
    async fn receive(&self) -> Result<Message> {
        // Print prompt (using std io::Write + flush instead of async)
        print!("> ");
        if let Err(e) = std::io::stdout().flush() {
            return Err(ForjaError::ChannelError(format!(
                "Stdout flush failed: {}",
                e
            )));
        }

        let stdin = io::stdin();
        let mut reader = BufReader::new(stdin);
        let mut buffer = String::new();

        loop {
            let mut line = String::new();
            let bytes_read = reader
                .read_line(&mut line)
                .await
                .map_err(|e| ForjaError::ChannelError(format!("Failed to read stdin: {}", e)))?;

            if bytes_read == 0 {
                // EOF reached (e.g., Ctrl+D)
                return Err(ForjaError::ChannelError("EOF reached".to_string()));
            }

            let trimmed = line.trim_end_matches(['\r', '\n']);
            if process_line(trimmed, &mut buffer) {
                print!("... ");
                if let Err(e) = std::io::stdout().flush() {
                    return Err(ForjaError::ChannelError(format!(
                        "Stdout flush failed: {}",
                        e
                    )));
                }
                continue;
            }

            break;
        }

        // Empty input is returned as-is; engine can retry.
        let adapter = CliAdapter;
        let raw = Message::text(Role::User, buffer, None);
        Ok(adapter.from_envelope(adapter.to_envelope(raw)))
    }

    /// Displays engine-generated messages (Assistant or System) to the terminal.
    async fn send(&self, msg: Message) -> Result<()> {
        let adapter = CliAdapter;
        let msg = adapter.from_envelope(adapter.to_envelope(msg));
        match msg.role {
            Role::Assistant => {
                // Print Assistant text only (ToolCalls are handled internally)
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
                // Tool results: print for debug/confirmation
                if let Content::ToolResult { call_id, result } = msg.content {
                    println!("🔧 [Tool call '{}' result]: {}", call_id, result);
                }
            }
            Role::User => {
                // User messages already displayed in terminal, no-op
            }
        }
        Ok(())
    }

    fn is_cli_source(&self) -> bool {
        true
    }

    async fn cancel_typing(&self) {
        print!("\r\x1b[K");
        let _ = std::io::stdout().flush();
    }

    async fn log_line(&self, text: &str) {
        print!("\r\x1b[K");
        println!("{}", text);
        let _ = std::io::stdout().flush();
    }
}
