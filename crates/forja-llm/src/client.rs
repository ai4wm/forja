use crate::config::LlmConfig;
use crate::models::{ChatCompletionMessage, ChatCompletionRequest, ChatCompletionResponse};

use async_trait::async_trait;
use forja_core::error::{ForjaError, Result};
use forja_core::traits::LlmProvider;
use forja_core::types::{Content, Message, Role, ToolDefinition};
use reqwest::header::{HeaderMap, HeaderValue};
use std::pin::Pin;
use tokio_stream::Stream;

#[cfg(feature = "anthropic")]
use reqwest_eventsource::{Event, EventSource};

/// OpenAI Chat Completions 포맷을 사용하는 범용 LlmClient
///
/// LlmConfig를 통해 base_url, api_key, model, 헤더 등을 동적으로 외부 주입받아
/// 다양한 파운데이션 모델(OpenAI, Anthropic, DeepSeek, GLM, 로컬 Ollama)과 통신합니다.
pub struct LlmClient {
    client: reqwest::Client,
    config: LlmConfig,
}

impl LlmClient {
    /// 설정을 받아 HTTP 클라이언트를 빌드하며 인스턴스를 생성합니다.
    pub fn new(config: LlmConfig) -> Result<Self> {
        let mut headers = HeaderMap::new();

        // 1. Authorization: Bearer {API_KEY} (대부분의 OpenAI 호환 스펙의 기본 표준 방식)
        let auth_val = format!("Bearer {}", config.api_key);
        if let Ok(val) = HeaderValue::from_str(&auth_val) {
            headers.insert("Authorization", val);
        }

        // 2. Custom Extra Headers 반영
        // Anthropic의 `x-api-key`, `anthropic-version` 처럼 각 벤더별 고유 헤더 값을 삽입.
        for (k, v) in &config.extra_headers {
            if let Ok(hdr_name) = reqwest::header::HeaderName::from_bytes(k.as_bytes())
                && let Ok(hdr_val) = HeaderValue::from_str(v) {
                    headers.insert(hdr_name, hdr_val);
                }
        }

        // Content-Type은 builder 기본값이지만 강제로 명시
        headers.insert("content-type", HeaderValue::from_static("application/json"));

        let client = reqwest::Client::builder()
            .default_headers(headers)
            .build()
            .map_err(|e| ForjaError::Internal(e.to_string()))?;

        Ok(Self { client, config })
    }

    /// `forja_core::Message` 배열을 OpenAI ChatCompletion 요청(Post Body) 포맷으로 변환.
    fn prepare_payload<'a>(
        &'a self,
        messages: &'a [Message],
        tools: Option<&[ToolDefinition]>,
        stream: bool,
    ) -> ChatCompletionRequest<'a> {
        let chat_msgs: Vec<ChatCompletionMessage> = messages
            .iter()
            .map(|m| {
                match &m.content {
                    Content::Text { text } => {
                        let role = match m.role {
                            Role::System => "system",
                            Role::User => "user",
                            Role::Assistant => "assistant",
                            Role::Tool => "tool",
                        };
                        ChatCompletionMessage {
                            role: role.to_string(),
                            content: Some(text.clone()),
                            reasoning_content: None,
                            tool_calls: None,
                            tool_call_id: None,
                        }
                    }
                    Content::ToolCall { call_id, tool_name, arguments, reasoning_content } => {
                        ChatCompletionMessage {
                            role: "assistant".to_string(),
                            content: None, // 일반 응답 내용 (비원시적 모델은 추론을 여기에 담을 수 있음, 우선 None 유지)
                            reasoning_content: reasoning_content.clone(), // Moonshot 요구사항 대응
                            tool_calls: Some(vec![crate::models::ToolCall {
                                id: call_id.clone(),
                                call_type: "function".to_string(),
                                function: crate::models::ToolFunction {
                                    name: tool_name.clone(),
                                    arguments: arguments.to_string(),
                                }
                            }]),
                            tool_call_id: None,
                        }
                    }
                    Content::ToolResult { call_id, result } => {
                        ChatCompletionMessage {
                            role: "tool".to_string(),
                            content: Some(result.to_string()),
                            reasoning_content: None,
                            tool_calls: None,
                            tool_call_id: Some(call_id.clone()),
                        }
                    }
                }
            })
            .collect();

        let api_tools = tools.map(|ts| {
            ts.iter().map(|t| {
                serde_json::json!({
                    "type": "function",
                    "function": {
                        "name": t.name,
                        "description": t.description,
                        "parameters": t.parameters,
                    }
                })
            }).collect()
        });

        ChatCompletionRequest {
            model: &self.config.model,
            messages: chat_msgs,
            max_tokens: self.config.max_tokens,
            stream,
            tools: api_tools,
        }
    }
}

#[async_trait]
impl LlmProvider for LlmClient {
    async fn chat(&self, messages: &[Message], tools: Option<&[ToolDefinition]>) -> Result<Message> {
        let payload = self.prepare_payload(messages, tools, false);
        
        let endpoint = format!("{}/chat/completions", self.config.base_url);

        let response = self
            .client
            .post(&endpoint)
            .json(&payload)
            .send()
            .await
            .map_err(|e| ForjaError::LlmError(e.to_string()))?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(ForjaError::LlmError(format!("Http {}: {}", status, text)));
        }

        let response_text = response.text().await
            .map_err(|e| ForjaError::LlmError(format!("Failed to get response text: {}", e)))?;

        let parsed: ChatCompletionResponse = serde_json::from_str(&response_text)
            .map_err(|e| ForjaError::LlmError(format!("JSON parsing error: {}. Raw: {}", e, response_text)))?;

        // Choices 배열에서 첫 번째 항목의 message 객체 추출
        let choice = parsed
            .choices
            .into_iter()
            .next()
            .ok_or_else(|| ForjaError::LlmError("Empty response from LLM".into()))?;

        let chat_msg = choice.message
            .ok_or_else(|| ForjaError::LlmError("Missing message in LLM response".into()))?;

        // 1. Tool Calls가 있는지 확인
        if let Some(tool_calls) = chat_msg.tool_calls.clone()
            && let Some(tool_call) = tool_calls.first() {
                let args_json: serde_json::Value = serde_json::from_str(&tool_call.function.arguments)
                    .unwrap_or(serde_json::json!({}));
                
                return Ok(Message::tool_call_with_reasoning(
                    &tool_call.id,
                    &tool_call.function.name,
                    args_json,
                    chat_msg.reasoning_content,
                ));
            }

        // 2. 없으면 일반 텍스트
        let content = chat_msg.content.unwrap_or_default();
        Ok(Message::text(Role::Assistant, content))
    }

    #[cfg(feature = "anthropic")] // TODO: 향후 sse 혹은 stream 피쳐로 이름을 변경하는 것이 의미상 적절함. 현재는 plan.md대로 anthropic 사용
    async fn stream(
        &self,
        messages: &[Message],
        tools: Option<&[ToolDefinition]>,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<String>> + Send>>> {
        let payload = self.prepare_payload(messages, tools, true);
        let endpoint = format!("{}/chat/completions", self.config.base_url);

        let request_builder = self.client.post(&endpoint).json(&payload);

        let mut event_source = EventSource::new(request_builder)
            .map_err(|e| ForjaError::LlmError(e.to_string()))?;

        let stream = async_stream::stream! {
            while let Some(event_res) = tokio_stream::StreamExt::next(&mut event_source).await {
                match event_res {
                    Ok(Event::Message(msg)) => {
                        // OpenAI 스펙상 "[DONE]" 메세지는 스트리밍 종료 신호.
                        if msg.data == "[DONE]" {
                            break;
                        }

                        // SSE payload delta 구조 분해 추출
                        if let Ok(parsed) = serde_json::from_str::<ChatCompletionResponse>(&msg.data)
                            && let Some(choice) = parsed.choices.into_iter().next()
                                && let Some(delta) = choice.delta
                                    && let Some(text) = delta.content {
                                        yield Ok(text);
                                    }
                    }
                    Ok(Event::Open) => continue,
                    Err(e) => {
                        // 종료, 타임아웃, 예외 발생
                        yield Err(ForjaError::LlmError(format!("Stream error: {}", e)));
                        break;
                    }
                }
            }
        };

        Ok(Box::pin(stream))
    }

    // 스트림 피쳐가 꺼진 환경의 Fallback 코드
    #[cfg(not(feature = "anthropic"))]
    async fn stream(
        &self,
        _messages: &[Message],
        _tools: Option<&[ToolDefinition]>,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<String>> + Send>>> {
        Err(ForjaError::LlmError("streaming feature is not enabled. check cargo features.".into()))
    }
}
