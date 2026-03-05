use thiserror::Error;

#[derive(Error, Debug)]
pub enum ForjaError {
    #[error("LLM provider error: {0}")]
    LlmError(String),

    #[error("Memory store error: {0}")]
    MemoryError(String),

    #[error("Channel error: {0}")]
    ChannelError(String),

    #[error("Tool execution error: {0}")]
    ToolError(String),

    #[error("Internal engine error: {0}")]
    Internal(String),

    #[error("Max tool call depth exceeded ({0})")]
    MaxDepthExceeded(usize),

    #[error("Storage error: {0}")]
    Storage(String),

    #[error("Serialization error: {0}")]
    Serialization(String),

    #[error("Deserialization error: {0}")]
    Deserialization(String),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, ForjaError>;
