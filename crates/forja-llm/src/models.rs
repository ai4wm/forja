use serde::{Deserialize, Serialize};

/// OpenAI Chat Completions 포맷의 요청 본문 (Payload)
#[derive(Serialize, Debug)]
pub struct ChatCompletionRequest<'a> {
    pub model: &'a str,
    pub messages: Vec<ChatCompletionMessage>,
    pub max_tokens: u32,
    pub stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<serde_json::Value>>,
}

/// 단일 메시지 객체
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

/// OpenAI Chat Completions 포맷의 응답 본문 (일반 & 전체)
#[derive(Deserialize, Debug)]
pub struct ChatCompletionResponse {
    pub id: Option<String>,
    pub choices: Vec<Choice>,
}

/// 응답 내 개별 선택지 (Choice)
#[derive(Deserialize, Debug)]
pub struct Choice {
    /// 일반(non-streaming) 요청 시 반환되는 완성된 텍스트.
    pub message: Option<ChatMessage>,
    
    /// 스트리밍(SSE) 요청 시 델타(조각) 데이터가 담기는 위치.
    pub delta: Option<ChatDelta>,
}

/// Choice 내의 완전한 텍스트 메시지
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
    pub arguments: String, // JSON 문자열로 옴
}

/// 스트리밍 조각 데이터
#[derive(Deserialize, Debug)]
pub struct ChatDelta {
    pub content: Option<String>,
}
