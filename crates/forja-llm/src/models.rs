use serde::{Deserialize, Serialize};

/// OpenAI Chat Completions request payload.
#[derive(Serialize, Debug)]
pub struct ChatCompletionRequest<'a> {
    pub model: &'a str,
    pub messages: Vec<ChatCompletionMessage>,
    pub max_tokens: u32,
    pub stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<serde_json::Value>>,
}

/// Single message object.
#[derive(Serialize, Debug)]
pub struct ChatCompletionMessage {
    pub role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

/// OpenAI Chat Completions response body.
#[derive(Deserialize, Debug)]
pub struct ChatCompletionResponse {
    pub id: Option<String>,
    pub choices: Vec<Choice>,
}

/// Individual response choice.
#[derive(Deserialize, Debug)]
pub struct Choice {
    /// Completed text returned by a regular non-streaming request.
    pub message: Option<ChatMessage>,

    /// Delta chunk returned during streaming (SSE) requests.
    pub delta: Option<ChatDelta>,
}

/// Completed text message inside a choice.
#[derive(Deserialize, Debug)]
pub struct ChatMessage {
    pub role: Option<String>,
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_content: Option<String>,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct ToolCall {
    pub id: String,
    #[serde(rename = "type")]
    pub call_type: String,
    pub function: ToolFunction,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct ToolFunction {
    pub name: String,
    pub arguments: String, // Arrives as a JSON string
}

/// Streaming delta payload.
#[derive(Deserialize, Debug)]
pub struct ChatDelta {
    pub content: Option<String>,
}
