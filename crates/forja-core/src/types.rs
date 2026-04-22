use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

/// Tool definition passed to the model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value, // JSON Schema
}

/// Sender role of a message.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Role {
    User,
    Assistant,
    System,
    Tool,
}

/// Message payload. Text, tool calls, and tool results are represented as enums.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Content {
    /// Plain text message.
    Text {
        text: String,
        /// Gemini 3 thoughtSignature that may be returned with a response.
        #[serde(skip_serializing_if = "Option::is_none")]
        thought_signature: Option<String>,
    },

    /// When the model requests a tool call.
    ToolCall {
        /// Call ID used to match results.
        call_id: String,
        /// Tool name, for example "shell" or "file_read".
        tool_name: String,
        /// Structured JSON arguments.
        arguments: serde_json::Value,
        /// Optional reasoning content before the tool call.
        #[serde(skip_serializing_if = "Option::is_none")]
        reasoning_content: Option<String>,
        /// Gemini 3 functionCall signature that should be returned unchanged.
        #[serde(skip_serializing_if = "Option::is_none")]
        thought_signature: Option<String>,
    },

    /// Tool execution result returned to the model.
    ToolResult {
        /// Original call ID matching `ToolCall.call_id`.
        call_id: String,
        /// JSON result payload.
        result: serde_json::Value,
    },
}

/// Single message unit flowing through the system.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub id: String,
    pub role: Role,
    pub content: Content,
    pub timestamp: u64,
    /// Extensible metadata such as tokens, model name, or channel routing info.
    #[serde(default)]
    pub metadata: HashMap<String, serde_json::Value>,
}

impl Message {
    /// Helper for creating text messages.
    pub fn text(role: Role, text: impl Into<String>, thought_signature: Option<String>) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            role,
            content: Content::Text {
                text: text.into(),
                thought_signature,
            },
            timestamp: now(),
            metadata: HashMap::new(),
        }
    }

    /// Helper for creating tool-call messages.
    pub fn tool_call(
        call_id: impl Into<String>,
        tool_name: impl Into<String>,
        arguments: serde_json::Value,
        thought_signature: Option<String>,
    ) -> Self {
        Self::tool_call_with_reasoning(call_id, tool_name, arguments, None, thought_signature)
    }

    /// Helper for creating tool calls with reasoning content and thought signature.
    pub fn tool_call_with_reasoning(
        call_id: impl Into<String>,
        tool_name: impl Into<String>,
        arguments: serde_json::Value,
        reasoning_content: Option<String>,
        thought_signature: Option<String>,
    ) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            role: Role::Assistant,
            content: Content::ToolCall {
                call_id: call_id.into(),
                tool_name: tool_name.into(),
                arguments,
                reasoning_content,
                thought_signature,
            },
            timestamp: now(),
            metadata: HashMap::new(),
        }
    }

    /// Helper for creating tool-result messages.
    pub fn tool_result(call_id: impl Into<String>, result: serde_json::Value) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            role: Role::Tool,
            content: Content::ToolResult {
                call_id: call_id.into(),
                result,
            },
            timestamp: now(),
            metadata: HashMap::new(),
        }
    }

    /// Adds metadata as key-value pairs via builder pattern.
    pub fn with_metadata(mut self, key: impl Into<String>, value: serde_json::Value) -> Self {
        self.metadata.insert(key.into(), value);
        self
    }

    /// Returns an approximate text length for token estimation.
    pub fn content_text_len(&self) -> usize {
        match &self.content {
            Content::Text { text, .. } => text.len(),
            Content::ToolCall {
                tool_name,
                arguments,
                ..
            } => tool_name.len() + arguments.to_string().len(),
            Content::ToolResult { result, .. } => result.to_string().len(),
        }
    }
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Memory retrieval item including score and timestamp for hybrid ranking.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEntry {
    pub id: String,
    pub content: String,
    pub score: f64,
    pub timestamp: u64,
    pub tags: Vec<String>,
    #[serde(default)]
    pub metadata: HashMap<String, serde_json::Value>,
}
