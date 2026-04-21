use crate::error::Result;
use crate::types::{MemoryEntry, Message, ToolDefinition};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
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
    async fn load_startup_context(&self) -> Result<String> {
        self.load_all().await
    }
    async fn load_relevant(&self, query: &str) -> Result<String> {
        let _ = query;
        self.load_all().await
    }
    async fn flush(&self) -> Result<()>;

    async fn run_dream(&self, trigger: DreamTrigger) -> Result<DreamRunOutcome> {
        let _ = trigger;
        Ok(DreamRunOutcome {
            status: DreamRunStatus::Skipped,
            summary: "dream is not supported by this memory store".to_string(),
            archived_topics: Vec::new(),
            merged_topics: Vec::new(),
            split_topics: Vec::new(),
            completed_at: None,
        })
    }

    async fn latest_dream_timestamp(&self) -> Result<Option<u64>> {
        Ok(None)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DreamTrigger {
    Idle,
    Manual,
    Shutdown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DreamRunStatus {
    Completed,
    Skipped,
    AbortedConflict,
    Recovered,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DreamRunOutcome {
    pub status: DreamRunStatus,
    pub summary: String,
    pub archived_topics: Vec<String>,
    pub merged_topics: Vec<String>,
    pub split_topics: Vec<String>,
    pub completed_at: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TelegramConnectionStatus {
    Connected,
    Disconnected,
    Reconnecting,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VoiceChannelStatus {
    Disabled,
    Listening,
    Speaking,
    Unavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum NotificationLevel {
    Info,
    Warning,
    Critical,
}

impl NotificationLevel {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Info => "info",
            Self::Warning => "warning",
            Self::Critical => "critical",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NotificationTopic {
    Task,
    Autonomy,
    Skill,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct NotificationState {
    pub enabled: bool,
    pub min_level: NotificationLevel,
    pub notify_tasks: bool,
    pub notify_autonomy: bool,
    pub notify_skills: bool,
    pub notify_errors: bool,
}

impl Default for NotificationState {
    fn default() -> Self {
        Self {
            enabled: true,
            min_level: NotificationLevel::Info,
            notify_tasks: true,
            notify_autonomy: true,
            notify_skills: true,
            notify_errors: true,
        }
    }
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

    fn shutdown(&self) {}

    /// Whether the current input source is CLI.
    fn is_cli_source(&self) -> bool { false }

    /// Cancel typing state such as spinners or typing indicators.
    async fn cancel_typing(&self) {}

    /// Print a CLI/log line after clearing transient typing UI such as spinners.
    async fn log_line(&self, text: &str) {
        let _ = text;
    }

    async fn send_notification(&self, text: &str) -> Result<bool> {
        let _ = text;
        Ok(false)
    }

    async fn send_notification_with_level(
        &self,
        text: &str,
        _topic: NotificationTopic,
        _level: NotificationLevel,
    ) -> Result<bool> {
        self.send_notification(text).await
    }

    fn telegram_status(&self) -> Option<TelegramConnectionStatus> {
        None
    }

    fn supports_voice(&self) -> bool {
        false
    }

    fn voice_status(&self) -> Option<VoiceChannelStatus> {
        None
    }

    async fn set_voice_enabled(&self, enabled: bool) -> Result<VoiceChannelStatus> {
        let _ = enabled;
        Ok(VoiceChannelStatus::Unavailable)
    }

    fn notification_state(&self) -> Option<NotificationState> {
        None
    }

    async fn set_notifications_enabled(&self, enabled: bool) -> Result<NotificationState> {
        let _ = enabled;
        Ok(NotificationState::default())
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

