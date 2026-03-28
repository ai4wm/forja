use async_trait::async_trait;
use chrono::{Duration, Local};
use forja_core::error::{ForjaError, Result};
use forja_core::serendipity::SerendipityEngine;
use forja_core::traits::MemoryStore;
use forja_core::types::MemoryEntry;
use forja_core::{Channel, Content, Engine, LlmProvider, Message, Role, ToolDefinition};
use std::collections::VecDeque;
use std::future::pending;
use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::{Mutex, oneshot};
use tokio_stream::Stream;

enum ProviderStep {
    Text(String),
    Error(String),
}

struct ScriptedProvider {
    steps: Mutex<VecDeque<ProviderStep>>,
    chat_requests: Mutex<Vec<Vec<Message>>>,
}

impl ScriptedProvider {
    fn new(steps: Vec<ProviderStep>) -> Self {
        Self {
            steps: Mutex::new(steps.into()),
            chat_requests: Mutex::new(Vec::new()),
        }
    }
}

#[async_trait]
impl LlmProvider for ScriptedProvider {
    async fn chat(
        &self,
        messages: &[Message],
        _tools: Option<&[ToolDefinition]>,
    ) -> Result<Message> {
        self.chat_requests.lock().await.push(messages.to_vec());

        match self.steps.lock().await.pop_front().unwrap() {
            ProviderStep::Text(text) => Ok(Message::text(Role::Assistant, text, None)),
            ProviderStep::Error(error) => Err(ForjaError::LlmError(error)),
        }
    }

    async fn stream(
        &self,
        _messages: &[Message],
        _tools: Option<&[ToolDefinition]>,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<String>> + Send>>> {
        Err(ForjaError::LlmError(
            "stream not used in this test".to_string(),
        ))
    }
}

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
}

struct RecordingMemoryStore {
    contents: Mutex<String>,
}

impl RecordingMemoryStore {
    fn new(contents: &str) -> Self {
        Self {
            contents: Mutex::new(contents.to_string()),
        }
    }
}

#[async_trait]
impl MemoryStore for RecordingMemoryStore {
    async fn save(&self, _entry: &MemoryEntry) -> Result<()> {
        Ok(())
    }

    async fn load_all(&self) -> Result<String> {
        Ok(self.contents.lock().await.clone())
    }

    async fn flush(&self) -> Result<()> {
        Ok(())
    }
}

#[tokio::test]
async fn generate_insight_returns_single_suggestion() {
    let provider = ScriptedProvider::new(vec![ProviderStep::Text(
        "You left the knowledge tests unfinished yesterday.".to_string(),
    )]);
    let engine = SerendipityEngine::with_interval(5);

    let insight = engine
        .generate_insight("Recent memory", "Known projects", &provider)
        .await
        .unwrap();

    assert_eq!(
        insight,
        Some("You left the knowledge tests unfinished yesterday.".to_string())
    );
}

#[tokio::test]
async fn generate_insight_returns_none_for_none_response() {
    let provider = ScriptedProvider::new(vec![ProviderStep::Text("NONE".to_string())]);
    let engine = SerendipityEngine::with_interval(5);

    let insight = engine
        .generate_insight("Recent memory", "Known projects", &provider)
        .await
        .unwrap();

    assert_eq!(insight, None);
}

#[tokio::test]
async fn generate_insight_returns_none_on_provider_failure() {
    let provider = ScriptedProvider::new(vec![ProviderStep::Error(
        "serendipity failed".to_string(),
    )]);
    let engine = SerendipityEngine::with_interval(5);

    let insight = engine
        .generate_insight("Recent memory", "Known projects", &provider)
        .await
        .unwrap();

    assert_eq!(insight, None);
}

#[test]
fn should_trigger_on_fifth_turn_by_default_interval() {
    let engine = SerendipityEngine::with_interval(5);

    assert!(engine.should_trigger(5, None));
}

#[test]
fn should_not_trigger_before_interval() {
    let engine = SerendipityEngine::with_interval(5);

    assert!(!engine.should_trigger(3, None));
}

#[test]
fn should_trigger_after_ten_minutes_even_before_interval() {
    let engine = SerendipityEngine::with_interval(5);
    let last_triggered = Some(Local::now() - Duration::minutes(11));

    assert!(engine.should_trigger(3, last_triggered));
}

#[tokio::test]
async fn serendipity_appends_suggestion_to_response_end() {
    let provider = Arc::new(ScriptedProvider::new(vec![
        ProviderStep::Text("assistant reply".to_string()),
        ProviderStep::Text("Resume Phase 16 from the remaining knowledge tests.".to_string()),
    ]));
    let memory_store = Arc::new(RecordingMemoryStore::new(
        "--- 2026-03-26 ---\n09:00 | user | Finish the remaining knowledge tests",
    ));
    let channel = Arc::new(QueueChannel::new(vec![Message::text(
        Role::User,
        "What should I do next?",
        None,
    )]));
    let mut engine = Engine::new(provider, channel.clone())
        .with_system_prompt("base system prompt".to_string())
        .with_memory(memory_store)
        .with_serendipity(SerendipityEngine::with_interval(1));
    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();

    let run_handle = tokio::spawn(async move {
        engine
            .run(async {
                let _ = shutdown_rx.await;
            })
            .await
            .unwrap();
    });

    for _ in 0..50 {
        if channel.sent_count().await >= 1 {
            break;
        }
        tokio::time::sleep(tokio::time::Duration::from_millis(20)).await;
    }

    shutdown_tx.send(()).unwrap();
    run_handle.await.unwrap();

    let sent = channel.sent_texts().await;
    assert_eq!(
        sent,
        vec!["assistant reply\n\nResume Phase 16 from the remaining knowledge tests.".to_string()]
    );
}

#[tokio::test]
async fn serendipity_failure_keeps_main_response_intact() {
    let provider = Arc::new(ScriptedProvider::new(vec![
        ProviderStep::Text("assistant fallback reply".to_string()),
        ProviderStep::Error("serendipity failed".to_string()),
    ]));
    let memory_store = Arc::new(RecordingMemoryStore::new(
        "--- 2026-03-26 ---\n09:00 | user | Remember unfinished work",
    ));
    let channel = Arc::new(QueueChannel::new(vec![Message::text(
        Role::User,
        "Keep responding even if serendipity fails",
        None,
    )]));
    let mut engine = Engine::new(provider, channel.clone())
        .with_system_prompt("base system prompt".to_string())
        .with_memory(memory_store)
        .with_serendipity(SerendipityEngine::with_interval(1));
    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();

    let run_handle = tokio::spawn(async move {
        engine
            .run(async {
                let _ = shutdown_rx.await;
            })
            .await
            .unwrap();
    });

    for _ in 0..50 {
        if channel.sent_count().await >= 1 {
            break;
        }
        tokio::time::sleep(tokio::time::Duration::from_millis(20)).await;
    }

    shutdown_tx.send(()).unwrap();
    run_handle.await.unwrap();

    let sent = channel.sent_texts().await;
    assert_eq!(sent, vec!["assistant fallback reply".to_string()]);
}

