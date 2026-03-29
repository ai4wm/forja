use crate::error::Result;
use crate::types::{MemoryEntry, Message, ToolDefinition};
use async_trait::async_trait;
use std::pin::Pin;
use tokio_stream::Stream;

/// LLM provider implemented in forja-llm, e.g. Anthropic or OpenAI.
#[async_trait]
pub trait LlmProvider: Send + Sync {
    /// Single response, optionally including tool-call information.
    async fn chat(&self, messages: &[Message], tools: Option<&[ToolDefinition]>) -> Result<Message>;

    /// Token streaming response, optionally including tool definitions.
    async fn stream(
        &self,
        messages: &[Message],
        tools: Option<&[ToolDefinition]>,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<String>> + Send>>>;
}

/// Memory store implemented in forja-memory, such as markdown files or vector DBs.
#[async_trait]
pub trait MemoryStore: Send + Sync {
    async fn save(&self, entry: &MemoryEntry) -> Result<()>;
    async fn load_all(&self) -> Result<String>;
    async fn flush(&self) -> Result<()>;
}

/// Input/output channel implemented in forja-channel, such as CLI or Telegram.
///
/// # Design note: `&self` vs `&mut self`
/// The channel trait uses `&self`. Implementations that need mutable state,
/// such as CLI stdin, can rely on interior mutability like `Mutex<BufReader<Stdin>>`.
/// This allows sharing through `Arc<dyn Channel>` and fits multi-agent scenarios.
#[async_trait]
pub trait Channel: Send + Sync {
    async fn receive(&self) -> Result<Message>;
    async fn send(&self, message: Message) -> Result<()>;
    
    /// Whether the current input source is CLI.
    fn is_cli_source(&self) -> bool { false }

    /// Cancel typing state such as spinners or typing indicators.
    async fn cancel_typing(&self) {}

    /// Print a CLI/log line after clearing transient typing UI such as spinners.
    async fn log_line(&self, text: &str) {
        let _ = text;
    }
}

/// Tool implemented in forja-tools, such as shell or file operations.
///
/// # Design note: args type
/// MCP is JSON-based, so args are received as `serde_json::Value`.
/// This makes structured inputs with names and types natural to handle.
#[async_trait]
pub trait Tool: Send + Sync {
    /// Unique tool name, for example "shell" or "file_read".
    fn name(&self) -> &str;

    /// Returns the tool specification, including JSON Schema.
    fn definition(&self) -> ToolDefinition;

    /// Executes the tool with structured JSON arguments.
    async fn execute(&self, args: serde_json::Value) -> Result<serde_json::Value>;
}

