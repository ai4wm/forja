use async_trait::async_trait;
use chrono::{Duration, Local, TimeZone};
use forja_core::emotion::EmotionEngine;
use forja_core::error::{ForjaError, Result};
use forja_core::traits::MemoryStore;
use forja_core::types::MemoryEntry;
use forja_core::{Channel, Content, Engine, LlmProvider, Message, Role, ToolDefinition};
use std::collections::VecDeque;
use std::future::pending;
use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::{Mutex, oneshot};
use tokio_stream::Stream;

fn collect_texts(messages: &[Message]) -> String {
    messages
        .iter()
        .filter_map(|message| match &message.content {
            Content::Text { text, .. } => Some(format!("{:?}:{text}", message.role)),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn build_memory_line(date: chrono::NaiveDate, time: &str, role: &str, content: &str) -> String {
    format!(
        "--- {} ---\n{} | {} | {}",
        date.format("%Y-%m-%d"),
        time,
        role,
        content
    )
}

enum ProviderStep {
    Text(String),
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

    async fn chat_texts(&self) -> Vec<String> {
        self.chat_requests
            .lock()
            .await
            .iter()
            .map(|messages| collect_texts(messages))
            .collect()
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
    saved_entries: Mutex<Vec<MemoryEntry>>,
}

impl RecordingMemoryStore {
    fn new(contents: &str) -> Self {
        Self {
            contents: Mutex::new(contents.to_string()),
            saved_entries: Mutex::new(Vec::new()),
        }
    }

    async fn saved_entries(&self) -> Vec<MemoryEntry> {
        self.saved_entries.lock().await.clone()
    }
}

#[async_trait]
impl MemoryStore for RecordingMemoryStore {
    async fn save(&self, entry: &MemoryEntry) -> Result<()> {
        self.saved_entries.lock().await.push(entry.clone());
        Ok(())
    }

    async fn load_all(&self) -> Result<String> {
        Ok(self.contents.lock().await.clone())
    }

    async fn flush(&self) -> Result<()> {
        Ok(())
    }
}

#[test]
fn detect_signals_marks_first_session_today_and_late_night() {
    let engine = EmotionEngine::new();
    let now = Local
        .with_ymd_and_hms(2026, 3, 29, 1, 30, 0)
        .single()
        .unwrap();

    let signals = engine.detect_signals(
        &[Message::text(Role::User, "Keep moving on this bug", None)],
        "",
        now,
    );

    assert!(signals.iter().any(|signal| signal == "late_night_detected"));
    assert!(signals.iter().any(|signal| signal == "first_session_today"));
}

#[test]
fn detect_signals_marks_long_absence_from_old_memory() {
    let engine = EmotionEngine::new();
    let now = Local
        .with_ymd_and_hms(2026, 3, 29, 10, 0, 0)
        .single()
        .unwrap();
    let old_day = now.date_naive() - Duration::days(5);
    let memory = build_memory_line(old_day, "12:00", "user", "Returning after a break");

    let signals = engine.detect_signals(&[], &memory, now);

    assert!(signals.iter().any(|signal| signal == "long_absence_detected"));
}

#[test]
fn detect_signals_marks_high_frequency_for_dense_recent_activity() {
    let engine = EmotionEngine::new();
    let now = Local
        .with_ymd_and_hms(2026, 3, 29, 10, 0, 0)
        .single()
        .unwrap();
    let today = now.date_naive();
    let memory = format!(
        "{}\n09:20 | assistant | Another quick update\n09:30 | user | Keep going\n09:40 | assistant | Applied another change\n09:50 | user | Check the next failure",
        build_memory_line(today, "09:10", "user", "Rapid follow-up")
    );

    let signals = engine.detect_signals(&[], &memory, now);

    assert!(signals.iter().any(|signal| signal == "high_frequency_detected"));
}

#[test]
fn detect_signals_marks_frustration_from_recent_messages() {
    let engine = EmotionEngine::new();
    let now = Local
        .with_ymd_and_hms(2026, 3, 29, 10, 0, 0)
        .single()
        .unwrap();

    let signals = engine.detect_signals(
        &[Message::text(
            Role::User,
            "This keeps failing with the same error and it is getting frustrating",
            None,
        )],
        "",
        now,
    );

    assert!(signals.iter().any(|signal| signal == "frustration_detected"));
}

#[tokio::test]
async fn emotion_signals_are_injected_into_system_prompt() {
    let provider = Arc::new(ScriptedProvider::new(vec![ProviderStep::Text(
        "main response".to_string(),
    )]));
    let memory_store = Arc::new(RecordingMemoryStore::new(
        "--- 2026-03-24 ---\n10:00 | user | Earlier work log",
    ));
    let channel = Arc::new(QueueChannel::new(vec![Message::text(
        Role::User,
        "This error keeps failing again",
        None,
    )]));
    let mut engine = Engine::new(provider.clone(), channel.clone())
        .with_system_prompt("base system prompt".to_string())
        .with_memory(memory_store)
        .with_emotion(EmotionEngine::new());
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

    let requests = provider.chat_texts().await;
    let main_request = &requests[0];

    assert!(main_request.contains("base system prompt"));
    assert!(main_request.contains("# Emotion Signals"));
    assert!(main_request.contains("frustration_detected"));
    assert!(main_request.contains("long_absence_detected"));
}

#[tokio::test]
async fn emotion_detection_does_not_add_extra_memory_entries() {
    let provider = Arc::new(ScriptedProvider::new(vec![ProviderStep::Text(
        "assistant reply".to_string(),
    )]));
    let memory_store = Arc::new(RecordingMemoryStore::new(""));
    let channel = Arc::new(QueueChannel::new(vec![Message::text(
        Role::User,
        "Continue with the next step",
        None,
    )]));
    let mut engine = Engine::new(provider, channel.clone())
        .with_system_prompt("base system prompt".to_string())
        .with_memory(memory_store.clone())
        .with_emotion(EmotionEngine::new());
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
    let saved_entries = memory_store.saved_entries().await;

    assert_eq!(sent, vec!["assistant reply".to_string()]);
    assert!(!saved_entries.iter().any(|entry| entry.tags.iter().any(|tag| tag == "system")));
}
