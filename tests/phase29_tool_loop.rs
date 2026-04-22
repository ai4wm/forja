use async_trait::async_trait;
use forja_core::error::{ForjaError, Result};
use forja_core::traits::{LlmProvider, LlmStreamEvent, Tool};
use forja_core::{Channel, Content, Engine, Message, Role, ToolDefinition};
use serde_json::{Value, json};
use std::future::pending;
use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio_stream::Stream;

struct QueueChannel {
    messages: Mutex<Vec<Message>>,
    sent_messages: Mutex<Vec<Message>>,
}

impl QueueChannel {
    fn new(messages: Vec<Message>) -> Self {
        Self {
            messages: Mutex::new(messages.into_iter().rev().collect()),
            sent_messages: Mutex::new(Vec::new()),
        }
    }

    async fn sent_texts(&self) -> Vec<String> {
        self.sent_messages
            .lock()
            .await
            .iter()
            .filter_map(|message| match &message.content {
                Content::Text { text, .. } => Some(text.clone()),
                _ => None,
            })
            .collect()
    }

    async fn sent_count(&self) -> usize {
        self.sent_messages.lock().await.len()
    }
}

#[async_trait]
impl Channel for QueueChannel {
    async fn receive(&self) -> Result<Message> {
        if let Some(message) = self.messages.lock().await.pop() {
            Ok(message)
        } else {
            pending::<Result<Message>>().await
        }
    }

    async fn send(&self, message: Message) -> Result<()> {
        self.sent_messages.lock().await.push(message);
        Ok(())
    }

    fn is_cli_source(&self) -> bool {
        true
    }
}

struct StaticWeatherTool;

#[async_trait]
impl Tool for StaticWeatherTool {
    fn name(&self) -> &str {
        "get_weather"
    }

    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: self.name().to_string(),
            description: "Return current weather for a city".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "city": { "type": "string" }
                },
                "required": ["city"]
            }),
        }
    }

    async fn execute(&self, args: Value) -> Result<Value> {
        let city = args
            .get("city")
            .and_then(Value::as_str)
            .ok_or_else(|| ForjaError::ToolError("missing city".to_string()))?;

        Ok(json!({
            "city": city,
            "condition": "sunny",
            "temperature_c": 24
        }))
    }
}

#[derive(Default)]
struct ToolLoopProvider {
    chat_requests: Mutex<Vec<Vec<Message>>>,
}

impl ToolLoopProvider {
    async fn last_chat_request(&self) -> Vec<Message> {
        self.chat_requests
            .lock()
            .await
            .last()
            .cloned()
            .unwrap_or_default()
    }
}

#[async_trait]
impl LlmProvider for ToolLoopProvider {
    async fn chat(
        &self,
        messages: &[Message],
        _tools: Option<&[ToolDefinition]>,
    ) -> Result<Message> {
        self.chat_requests.lock().await.push(messages.to_vec());

        let saw_tool_result = messages.iter().any(|message| {
            matches!(
                &message.content,
                Content::ToolResult { result, .. }
                    if result.get("city").and_then(Value::as_str) == Some("Seoul")
            )
        });

        if saw_tool_result {
            Ok(Message::text(
                Role::Assistant,
                "서울은 현재 맑고 24°C입니다.",
                None,
            ))
        } else {
            Err(ForjaError::LlmError(
                "chat should only be called after tool execution".to_string(),
            ))
        }
    }

    async fn stream(
        &self,
        _messages: &[Message],
        _tools: Option<&[ToolDefinition]>,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<String>> + Send>>> {
        Err(ForjaError::LlmError(
            "plain text stream path should not be used in this test".to_string(),
        ))
    }

    async fn stream_events(
        &self,
        _messages: &[Message],
        _tools: Option<&[ToolDefinition]>,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<LlmStreamEvent>> + Send>>> {
        let events = vec![
            Ok(LlmStreamEvent::Text("조회하겠습니다.".to_string())),
            Ok(LlmStreamEvent::ToolCall(Message::tool_call(
                "call-weather",
                "get_weather",
                json!({ "city": "Seoul" }),
                None,
            ))),
        ];
        Ok(Box::pin(tokio_stream::iter(events)))
    }
}

#[tokio::test]
async fn streaming_tool_call_executes_tool_and_reinjects_result() {
    let provider = Arc::new(ToolLoopProvider::default());
    let channel = Arc::new(QueueChannel::new(vec![Message::text(
        Role::User,
        "서울 날씨 알려줘",
        None,
    )]));
    let mut engine = Engine::new(provider.clone(), channel.clone())
        .with_assistant_profile("Forja".to_string(), "User".to_string())
        .with_system_prompt("You are Forja.".to_string());
    engine.register_tool(Arc::new(StaticWeatherTool));

    engine
        .run_streaming(async {
            for _ in 0..100 {
                if channel.sent_count().await >= 1 {
                    break;
                }
                tokio::time::sleep(tokio::time::Duration::from_millis(20)).await;
            }
        })
        .await
        .unwrap();

    let sent_texts = channel.sent_texts().await;
    assert_eq!(sent_texts, vec!["서울은 현재 맑고 24°C입니다.".to_string()]);

    let last_chat_request = provider.last_chat_request().await;
    assert!(last_chat_request.iter().any(|message| {
        matches!(
            &message.content,
            Content::ToolResult { call_id, result }
                if call_id == "call-weather"
                    && result.get("city").and_then(Value::as_str) == Some("Seoul")
                    && message.metadata.get("tool_name") == Some(&json!("get_weather"))
        )
    }));
}
