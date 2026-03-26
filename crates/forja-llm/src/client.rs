use crate::config::LlmConfig;
use crate::models::{ChatCompletionMessage, ChatCompletionRequest, ChatCompletionResponse};

use async_trait::async_trait;
use forja_core::error::{ForjaError, Result};
use forja_core::traits::LlmProvider;
use forja_core::types::{Content, Message, Role, ToolDefinition};
use reqwest::header::{HeaderMap, HeaderValue};
use std::pin::Pin;
use tokio_stream::Stream;

/// Generic LlmClient using the OpenAI Chat Completions format.
///
/// Receives base_url, api_key, model, and headers through LlmConfig
/// to communicate with multiple foundation models such as OpenAI,
/// Anthropic, DeepSeek, GLM, and local Ollama.
#[cfg(feature = "anthropic")]
pub struct LlmClient {
    client: reqwest::Client,
    config: LlmConfig,
}

impl LlmClient {
    /// Builds the HTTP client from config and creates a new instance.
    pub fn new(config: LlmConfig) -> Result<Self> {
        let mut headers = HeaderMap::new();

        // 1. Authorization: Bearer {API_KEY}
        let auth_val = format!("Bearer {}", config.api_key);
        if let Ok(val) = HeaderValue::from_str(&auth_val) {
            headers.insert("Authorization", val);
        }

        // 2. Apply custom extra headers such as vendor-specific auth/version headers.
        for (k, v) in &config.extra_headers {
            if let Ok(hdr_name) = reqwest::header::HeaderName::from_bytes(k.as_bytes())
                && let Ok(hdr_val) = HeaderValue::from_str(v) {
                    headers.insert(hdr_name, hdr_val);
                }
        }

        // Content-Type is already set by the builder, but keep it explicit.
        headers.insert("content-type", HeaderValue::from_static("application/json"));

        let client = reqwest::Client::builder()
            .default_headers(headers)
            .build()
            .map_err(|e| ForjaError::Internal(e.to_string()))?;

        Ok(Self { client, config })
    }

    /// Converts `forja_core::Message` slices into an OpenAI-style request payload.
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
                    Content::Text { text, thought_signature: _ } => {
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
                    Content::ToolCall { call_id, tool_name, arguments, reasoning_content, thought_signature: _ } => {
                        ChatCompletionMessage {
                            role: "assistant".to_string(),
                            content: None, // Reserved for plain text content in assistant messages
                            reasoning_content: reasoning_content.clone(), // Preserve Moonshot reasoning content
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

    /// Builds a payload for the Responses API (/v1/responses).
    fn prepare_responses_payload(
        &self,
        messages: &[Message],
        tools: Option<&[ToolDefinition]>,
        stream: bool,
    ) -> serde_json::Value {
        let mut instructions = String::new();
        let input: Vec<serde_json::Value> = messages
            .iter()
            .filter_map(|m| {
                match &m.content {
                    Content::Text { text, thought_signature: _ } if m.role == Role::System => {
                        if !instructions.is_empty() {
                            instructions.push('\n');
                        }
                        instructions.push_str(text);
                        None
                    }
                    Content::Text { text, thought_signature: _ } => {
                        let role = match m.role {
                            Role::User => "user",
                            Role::Assistant => "assistant",
                            Role::Tool => "tool",
                            Role::System => unreachable!(), // Filtered out above
                        };
                        Some(serde_json::json!({
                            "role": role,
                            "content": text,
                        }))
                    }
                    Content::ToolCall { call_id, tool_name, arguments, .. } => {
                        Some(serde_json::json!({
                            "type": "function_call",
                            "call_id": call_id,
                            "name": tool_name,
                            "arguments": arguments.to_string(),
                        }))
                    }
                    Content::ToolResult { call_id, result } => {
                        Some(serde_json::json!({
                            "type": "function_call_output",
                            "call_id": call_id,
                            "output": result.to_string(),
                        }))
                    }
                }
            })
            .collect();

        let api_tools: Option<Vec<serde_json::Value>> = tools.map(|ts| {
            ts.iter().map(|t| {
                serde_json::json!({
                    "type": "function",
                    "name": t.name,
                    "description": t.description,
                    "parameters": t.parameters,
                })
            }).collect()
        });

        let mut payload = serde_json::json!({
            "model": self.config.model,
            "instructions": if instructions.is_empty() {
                "You are a helpful assistant.".to_string()
            } else {
                instructions
            },
            "input": input,
            "stream": stream,
            "store": false,
        });

        if let Some(t) = api_tools {
            payload["tools"] = serde_json::json!(t);
        }

        payload
    }

    /// Builds a payload for Gemini Native API (v1internal:streamGenerateContent).
    fn prepare_gemini_native_payload(
        &self,
        messages: &[Message],
        tools: Option<&[ToolDefinition]>,
    ) -> serde_json::Value {
        // 1. Extract system_instruction
        let mut system_parts: Vec<serde_json::Value> = Vec::new();
        
        // 2. Convert messages into Gemini Native "contents" format
        let mut contents: Vec<serde_json::Value> = Vec::new();
        
        for m in messages {
            match (&m.role, &m.content) {
                (Role::System, Content::Text { text, .. }) => {
                    system_parts.push(serde_json::json!({"text": text}));
                }
                (Role::User, Content::Text { text, .. }) => {
                    contents.push(serde_json::json!({
                        "role": "user",
                        "parts": [{"text": text}]
                    }));
                }
                (Role::Assistant, Content::Text { text, thought_signature }) => {
                    let mut part = serde_json::json!({"text": text});
                    if let Some(ts) = thought_signature {
                        part["thoughtSignature"] = serde_json::json!(ts);
                    }
                    contents.push(serde_json::json!({
                        "role": "model",
                        "parts": [part]
                    }));
                }
                (Role::Assistant, Content::ToolCall { call_id, tool_name, arguments, thought_signature, .. }) => {
                    let mut part = serde_json::json!({
                        "functionCall": {
                            "id": call_id,
                            "name": tool_name,
                            "args": arguments
                        }
                    });
                    if let Some(ts) = thought_signature {
                        part["thoughtSignature"] = serde_json::json!(ts);
                    }
                    contents.push(serde_json::json!({
                        "role": "model",
                        "parts": [part]
                    }));
                }
                (Role::Tool, Content::ToolResult { call_id, result }) => {
                    // Gemini expects tool results as a user-side functionResponse part.
                    contents.push(serde_json::json!({
                        "role": "user",
                        "parts": [{
                            "functionResponse": {
                                "name": call_id,
                                "response": {"result": result.to_string()}
                            }
                        }]
                    }));
                }
                _ => {}
            }
        }
        
        // 3. Convert tool definitions into Gemini functionDeclarations format
        let gemini_tools: Option<Vec<serde_json::Value>> = tools.map(|ts| {
            let decls: Vec<serde_json::Value> = ts.iter().map(|t| {
                serde_json::json!({
                    "name": t.name,
                    "description": t.description,
                    "parameters": t.parameters
                })
            }).collect();
            vec![serde_json::json!({"functionDeclarations": decls})]
        });
        
        // 4. Assemble the final payload
        let mut payload = serde_json::json!({
            "contents": contents,
            "generationConfig": {},
            "systemInstruction": {
                "parts": if system_parts.is_empty() {
                    vec![serde_json::json!({"text": "You are a helpful assistant."})]
                } else {
                    system_parts
                }
            }
        });
        
        if let Some(t) = gemini_tools {
            payload["tools"] = serde_json::json!(t);
        }
        
        payload
    }
}

#[async_trait]
impl LlmProvider for LlmClient {
    async fn chat(&self, messages: &[Message], tools: Option<&[ToolDefinition]>) -> Result<Message> {
        if self.config.use_gemini_native_api {
            let inner = self.prepare_gemini_native_payload(messages, tools);
            let project = std::env::var("FORJA_GEMINI_PROJECT")
                .map_err(|_| ForjaError::LlmError("Project ID missing... please run forja login gemini again".to_string()))?;
            let payload = serde_json::json!({
                "request": inner,
                "model": self.config.model,
                "project": project,
            });
            let endpoint = format!(
                "{}/v1internal:streamGenerateContent?alt=sse",
                self.config.base_url
            );

            let mut response = self
                .client
                .post(&endpoint)
                .json(&payload)
                .send()
                .await
                .map_err(|e| ForjaError::LlmError(e.to_string()))?;

            if !response.status().is_success() {
                let status = response.status();
                let text = response.text().await.unwrap_or_default();
                eprintln!("[GEMINI-ERROR] Http {}: {}", status, &text[..text.len().min(300)]);
                return Err(ForjaError::LlmError(format!("Http {}: {}", status, text)));
            }

                        let mut raw = String::new();
            while let Some(chunk) = response.chunk().await
                .map_err(|e| ForjaError::LlmError(e.to_string()))? 
            {
                let chunk_str = String::from_utf8_lossy(&chunk);
                raw.push_str(&chunk_str);
                
                // Stop once finishReason appears in the response
                if raw.contains("\"finishReason\"") {
                    break;
                }
            }
            let mut collected_text = String::new();
            let mut last_thought_signature: Option<String> = None;
            for line in raw.lines() {
                if let Some(data) = line.strip_prefix("data: ")
                    && let Ok(ev) = serde_json::from_str::<serde_json::Value>(data)
                {
                    let candidates = ev.get("response")
                        .and_then(|r| r.get("candidates"))
                        .or_else(|| ev.get("candidates"));

                    if let Some(candidates) = candidates.and_then(|c| c.as_array())
                        && let Some(candidate) = candidates.first()
                    {
                        let parts = candidate
                            .get("content")
                            .and_then(|c| c.get("parts"))
                            .and_then(|p| p.as_array());

                        if let Some(parts) = parts {
                            for part in parts {
                                // Detect tool calls
                                if let Some(fc) = part.get("functionCall") {
                                    let call_id = fc.get("id")
                                        .or_else(|| fc.get("name"))
                                        .and_then(|v| v.as_str())
                                        .unwrap_or_default()
                                        .to_string();
                                    let name = fc.get("name")
                                        .and_then(|v| v.as_str())
                                        .unwrap_or_default()
                                        .to_string();
                                    let args = fc.get("args")
                                        .cloned()
                                        .unwrap_or(serde_json::json!({}));

                                    let ts = part.get("thoughtSignature")
                                        .and_then(|v| v.as_str())
                                        .map(|s| s.to_string());

                                    return Ok(Message::tool_call_with_reasoning(
                                        &call_id, &name, args, None, ts,
                                    ));
                                }

                                // Collect text parts, skipping thoughts
                                if part.get("thought").and_then(|t| t.as_bool()).unwrap_or(false) {
                                    continue;
                                }
                                if let Some(ts) = part.get("thoughtSignature").and_then(|t| t.as_str()) {
                                    last_thought_signature = Some(ts.to_string());
                                }
                                if let Some(text) = part.get("text").and_then(|t| t.as_str()) {
                                    collected_text.push_str(text);
                                }
                            }
                        }
                    }
                }
            }
            return Ok(Message::text(Role::Assistant, collected_text, last_thought_signature));
        }

        if self.config.use_responses_api {
            // Force stream=true (Codex backend-api requirement)
            let payload = self.prepare_responses_payload(messages, tools, true);
            let endpoint = format!("{}/codex/responses", self.config.base_url);

            let response = self
                .client
                .post(&endpoint)
                .json(&payload)
                .header("Accept", "text/event-stream")
                .send()
                .await
                .map_err(|e| ForjaError::LlmError(e.to_string()))?;

            if !response.status().is_success() {
                let status = response.status();
                let text = response.text().await.unwrap_or_default();
                eprintln!("[GEMINI-ERROR] Http {}: {}", status, &text[..text.len().min(300)]);
                return Err(ForjaError::LlmError(format!("Http {}: {}", status, text)));
            }

            // Read and parse the full SSE text body
                        let raw = response.text().await
                .map_err(|e| ForjaError::LlmError(e.to_string()))?;

            

            let mut collected_text = String::new();
            let mut last_item_id = String::new();
            let mut last_tool_name = String::new();
            for line in raw.lines() {
                if let Some(data) = line.strip_prefix("data: ")
                    && let Ok(ev) = serde_json::from_str::<serde_json::Value>(data)
                {
                    match ev["type"].as_str() {
                        Some("response.output_item.added") => {
                            if let Some(id) = ev["item"]["id"].as_str() {
                                last_item_id = id.to_string();
                            }
                            // Tool name may be included here
                            if let Some(name) = ev["item"]["name"].as_str() {
                                last_tool_name = name.to_string();
                            }
                        }
                        Some("response.output_text.delta") => {
                            if let Some(d) = ev["delta"].as_str() {
                                collected_text.push_str(d);
                            }
                        }
                        Some("response.function_call_arguments.done") => {
                            let call_id = ev["call_id"].as_str()
                                .or_else(|| ev["item_id"].as_str())
                                .unwrap_or(&last_item_id)
                                .to_string();
                            let name = ev["name"].as_str()
                                .map(|s| s.to_string())
                                .filter(|s| !s.is_empty())
                                .unwrap_or_else(|| last_tool_name.clone());

                            let args_str = ev["arguments"].as_str().unwrap_or("{}");
                            let args = serde_json::from_str(args_str).unwrap_or(serde_json::json!({}));
                            return Ok(Message::tool_call_with_reasoning(&call_id, &name, args, None, None));
                        }
                        Some("response.completed") | Some("response.failed") => break,
                        _ => {}
                    }
                }
            }

            return Ok(Message::text(Role::Assistant, collected_text, None));
        }

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
                eprintln!("[GEMINI-ERROR] Http {}: {}", status, &text[..text.len().min(300)]);
                return Err(ForjaError::LlmError(format!("Http {}: {}", status, text)));
        }

        let response_text = response.text().await
            .map_err(|e| ForjaError::LlmError(format!("Failed to get response text: {}", e)))?;

        let parsed: ChatCompletionResponse = serde_json::from_str(&response_text)
            .map_err(|e| ForjaError::LlmError(format!("JSON parsing error: {}. Raw: {}", e, response_text)))?;

        // Extract the first choice message object
        let choice = parsed
            .choices
            .into_iter()
            .next()
            .ok_or_else(|| ForjaError::LlmError("Empty response from LLM".into()))?;

        let chat_msg = choice.message
            .ok_or_else(|| ForjaError::LlmError("Missing message in LLM response".into()))?;

        // 1. Check for tool calls
        if let Some(tool_calls) = chat_msg.tool_calls.clone()
            && let Some(tool_call) = tool_calls.first() {
                let args_json: serde_json::Value = serde_json::from_str(&tool_call.function.arguments)
                    .unwrap_or(serde_json::json!({}));
                
                return Ok(Message::tool_call_with_reasoning(
                    &tool_call.id,
                    &tool_call.function.name,
                    args_json,
                    chat_msg.reasoning_content,
                    None,
                ));
            }

        // 2. Otherwise return plain text
        let content = chat_msg.content.unwrap_or_default();
        Ok(Message::text(Role::Assistant, content, None))
    }

    #[cfg(feature = "anthropic")]
    async fn stream(
        &self,
        messages: &[Message],
        tools: Option<&[ToolDefinition]>,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<String>> + Send>>> {
        let (endpoint, payload) = if self.config.use_gemini_native_api {
            let inner = self.prepare_gemini_native_payload(messages, tools);
            let project = std::env::var("FORJA_GEMINI_PROJECT")
                .unwrap_or_else(|_| "outstanding-sanctum-knhr3".to_string());
            let payload = serde_json::json!({
                "request": inner,
                "model": self.config.model,
                "project": project,
            });
            let endpoint = format!("{}/v1internal:streamGenerateContent?alt=sse", self.config.base_url);
            (endpoint, payload)
        } else if self.config.use_responses_api {
            let payload = self.prepare_responses_payload(messages, tools, true);
            let endpoint = format!("{}/codex/responses", self.config.base_url);
            (endpoint, payload)
        } else {
            let payload = serde_json::to_value(self.prepare_payload(messages, tools, true))
                .map_err(|e| ForjaError::Internal(e.to_string()))?;
            let endpoint = format!("{}/chat/completions", self.config.base_url);
            (endpoint, payload)
        };

        let mut response = self.client.post(&endpoint)
            .json(&payload)
            .header("Accept", "text/event-stream")
            .send()
            .await
            .map_err(|e| ForjaError::LlmError(e.to_string()))?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            eprintln!("[STREAM-ERROR] Http {}: {}", status, &text[..text.len().min(300)]);
            return Err(ForjaError::LlmError(format!("Http {}: {}", status, text)));
        }

        let is_gemini = self.config.use_gemini_native_api;
        let is_responses = self.config.use_responses_api;

        let stream = async_stream::stream! {
            let mut buffer = String::new();
            while let Ok(Some(chunk)) = response.chunk().await {
                let chunk_str = String::from_utf8_lossy(&chunk);
                buffer.push_str(&chunk_str);

                while let Some(pos) = buffer.find('\n') {
                    let line = buffer[..pos].trim().to_string();
                    buffer = buffer[pos + 1..].to_string();

                    if line.is_empty() { continue; }
                    if line == "data: [DONE]" { break; }

                    if let Some(data) = line.strip_prefix("data: ")
                        && let Ok(ev) = serde_json::from_str::<serde_json::Value>(data)
                    {
                        if is_gemini {
                            let candidates = ev.get("response")
                                .and_then(|r| r.get("candidates"))
                                .or_else(|| ev.get("candidates"));
                            if let Some(candidates) = candidates.and_then(|c| c.as_array())
                                && let Some(candidate) = candidates.first()
                            {
                                if candidate.get("finishReason").is_some() { break; }
                                if let Some(parts) = candidate.get("content").and_then(|c| c.get("parts")).and_then(|p| p.as_array()) {
                                    for part in parts {
                                        if part.get("thought").and_then(|t| t.as_bool()).unwrap_or(false) { continue; }
                                        if let Some(text) = part.get("text").and_then(|t| t.as_str()) {
                                            yield Ok(text.to_string());
                                        }
                                    }
                                }
                            }
                        } else if is_responses {
                            match ev["type"].as_str() {
                                Some("response.output_text.delta") => {
                                    if let Some(delta) = ev["delta"].as_str() {
                                        yield Ok(delta.to_string());
                                    }
                                }
                                Some("response.completed") | Some("response.failed") => break,
                                _ => {}
                            }
                        } else {
                            // Default OpenAI format
                            if let Some(text) = ev["choices"][0]["delta"]["content"].as_str() {
                                yield Ok(text.to_string());
                            }
                        }
                    }
                }
            }
        };

        Ok(Box::pin(stream))
    }

    // Fallback when the streaming feature is disabled
    #[cfg(not(feature = "anthropic"))]
    async fn stream(
        &self,
        _messages: &[Message],
        _tools: Option<&[ToolDefinition]>,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<String>> + Send>>> {
        Err(ForjaError::LlmError("streaming feature is not enabled. check cargo features.".into()))
    }
}
