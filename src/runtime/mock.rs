use async_trait::async_trait;
use forja_core::error::Result;
use forja_core::{Content, Message, Role, ToolDefinition};
use forja_core::traits::LlmProvider;
use std::pin::Pin;
use tokio_stream::{Stream, StreamExt};

// Mock LLM used for local testing without a real API key.
pub(crate) struct MockLlmProvider;

#[async_trait]
impl LlmProvider for MockLlmProvider {
    async fn chat(
        &self,
        messages: &[Message],
        _tools: Option<&[ToolDefinition]>,
    ) -> Result<Message> {
        let last = messages
            .iter()
            .rev()
            .find(|message| message.role == Role::User)
            .map(|message| match &message.content {
                Content::Text { text, .. } => text.clone(),
                _ => "(no text)".to_string(),
            })
            .unwrap_or_default();

        if last.contains("Analyze the emotional state of the conversation below and respond with JSON only.") {
            return Ok(Message::text(
                Role::Assistant,
                r#"{"mood":"neutral","intensity":1,"reason":"mock mode","tone_instruction":"Reply in a balanced, respectful tone."}"#,
                None,
            ));
        }

        if last.contains("Write one natural greeting sentence.") {
            return Ok(Message::text(Role::Assistant, "NONE", None));
        }

        if last.contains("Also, if there are unfinished tasks or a useful daily summary") {
            return Ok(Message::text(Role::Assistant, "NONE", None));
        }

        if last.contains("Summarize the daily memory.md records in max 3 lines.") {
            return Ok(Message::text(Role::Assistant, "Mock summary", None));
        }

        if last.contains("Below is the user's recent memory and knowledge base.") {
            return Ok(Message::text(Role::Assistant, "NONE", None));
        }

        Ok(Message::text(
            Role::Assistant,
            format!(
                "[MockLLM] Received message: '{last}' (configure a real API key to get a live response.)"
            ),
            None,
        ))
    }

    async fn stream(
        &self,
        messages: &[Message],
        _tools: Option<&[ToolDefinition]>,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<String>> + Send>>> {
        let last = messages
            .iter()
            .rev()
            .find(|message| message.role == Role::User)
            .map(|message| match &message.content {
                Content::Text { text, .. } => text.clone(),
                _ => "(no text)".to_string(),
            })
            .unwrap_or_default();

        let response = format!(
            "[MockStream] Received message: '{last}' (streaming effect test...)"
        );
        let tokens: Vec<String> = response
            .split(' ')
            .map(|token| format!("{token} "))
            .collect();
        let stream = tokio_stream::iter(tokens).map(Ok);

        Ok(Box::pin(stream))
    }
}
