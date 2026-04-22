use async_trait::async_trait;
use chrono::{Datelike, Local, TimeZone};
use forja_core::error::{ForjaError, Result};
use forja_core::traits::MemoryStore;
use forja_core::types::MemoryEntry;
use forja_core::{Channel, Content, Engine, LlmProvider, Message, Role, ToolDefinition};
use forja_memory::MarkdownMemoryStore;
use std::future::pending;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::{Mutex, oneshot};
use tokio_stream::Stream;

fn unique_temp_dir(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    std::env::temp_dir().join(format!("forja_{name}_{nanos}"))
}

fn memory_entry(id: &str, role: &str, content: &str, timestamp: u64) -> MemoryEntry {
    MemoryEntry {
        id: id.to_string(),
        content: content.to_string(),
        score: 0.0,
        timestamp,
        tags: vec![role.to_string()],
        metadata: Default::default(),
    }
}

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

fn write_legacy_session(path: &Path, id: &str, timestamp: u64, role: &str, content: &str) {
    let body =
        format!("---\nid: {id}\ntimestamp: {timestamp}\ntags:\n  - {role}\n---\n{content}\n");
    std::fs::write(path, body).unwrap();
}

struct RecordingProvider {
    chat_requests: Mutex<Vec<Vec<Message>>>,
}

impl RecordingProvider {
    fn new() -> Self {
        Self {
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
impl LlmProvider for RecordingProvider {
    async fn chat(
        &self,
        messages: &[Message],
        _tools: Option<&[ToolDefinition]>,
    ) -> Result<Message> {
        self.chat_requests.lock().await.push(messages.to_vec());
        Ok(Message::text(Role::Assistant, "chat response", None))
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
    log_lines: Mutex<Vec<String>>,
}

impl QueueChannel {
    fn new(messages: Vec<Message>) -> Self {
        Self {
            messages: Mutex::new(messages.into_iter().rev().collect()),
            sent_messages: Mutex::new(Vec::new()),
            log_lines: Mutex::new(Vec::new()),
        }
    }

    async fn sent_count(&self) -> usize {
        self.sent_messages.lock().await.len()
    }

    async fn logged_lines(&self) -> Vec<String> {
        self.log_lines.lock().await.clone()
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

    async fn log_line(&self, text: &str) {
        self.log_lines.lock().await.push(text.to_string());
    }

    fn is_cli_source(&self) -> bool {
        true
    }
}

#[tokio::test]
async fn restart_loads_memory_md_into_engine_prompt() {
    let base_dir = unique_temp_dir("phase13f_restart");
    let memory_path = base_dir.join("memory.md");
    let today = chrono::Local::now().date_naive();
    let old_day = today - chrono::Duration::days(4);

    let initial_store = MarkdownMemoryStore::new(&memory_path).await.unwrap();
    initial_store
        .save(&memory_entry("entry-1", "user", "I prefer oolong tea.", 60))
        .await
        .unwrap();
    initial_store
        .save(&memory_entry(
            "entry-2",
            "assistant",
            "You previously said you like oolong tea.",
            120,
        ))
        .await
        .unwrap();
    initial_store
        .save(&memory_entry(
            "entry-3",
            "user",
            "Project Atlas uses Rust for the dashboard.",
            Local
                .with_ymd_and_hms(old_day.year(), old_day.month(), old_day.day(), 9, 0, 0)
                .single()
                .unwrap()
                .timestamp() as u64,
        ))
        .await
        .unwrap();
    drop(initial_store);

    let restarted_store = Arc::new(MarkdownMemoryStore::new(&memory_path).await.unwrap());
    let provider = Arc::new(RecordingProvider::new());
    let channel = Arc::new(QueueChannel::new(vec![Message::text(
        Role::User,
        "What tea do I like?",
        None,
    )]));
    let mut engine = Engine::new(provider.clone(), channel.clone())
        .with_assistant_profile("Forja".to_string(), "User".to_string())
        .with_system_prompt("base system prompt".to_string())
        .with_memory(restarted_store);
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
    assert_eq!(requests.len(), 1);
    assert!(requests[0].contains("System:You are Forja, a personal AI assistant."));
    assert!(requests[0].contains("base system prompt"));
    assert!(requests[0].contains("[memory - Structured Persistent Memory]"));
    assert!(requests[0].contains("[memory index - Topic Index]"));
    assert!(requests[0].contains("oolong tea"));
    assert!(requests[0].contains("[memory topics - Relevant Topic Memory]"));
    assert!(requests[0].contains("User:What tea do I like?"));

    let _ = tokio::fs::remove_dir_all(&base_dir).await;
}

#[tokio::test]
async fn run_logs_korean_stage_messages_in_order() {
    let base_dir = unique_temp_dir("phase13f_stage_logs");
    let memory_path = base_dir.join("memory.md");
    let memory_store = Arc::new(MarkdownMemoryStore::new(&memory_path).await.unwrap());
    let provider = Arc::new(RecordingProvider::new());
    let channel = Arc::new(QueueChannel::new(vec![Message::text(
        Role::User,
        "Recall my memory",
        None,
    )]));
    let mut engine = Engine::new(provider, channel.clone())
        .with_assistant_profile("Forja".to_string(), "User".to_string())
        .with_system_prompt("base system prompt".to_string())
        .with_memory(memory_store);
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

    let logged_lines = channel.logged_lines().await;

    assert_eq!(
        logged_lines,
        vec![
            "\u{1b}[36m• Loading emotion context...\u{1b}[0m".to_string(),
            "\u{1b}[33m• Loading knowledge...\u{1b}[0m".to_string(),
            "\u{1b}[35m• Loading memory...\u{1b}[0m".to_string(),
            "\u{1b}[32m• Calling LLM...\u{1b}[0m".to_string(),
        ]
    );

    let _ = tokio::fs::remove_dir_all(&base_dir).await;
}

#[tokio::test]
async fn run_streaming_logs_korean_stage_messages_in_order() {
    let base_dir = unique_temp_dir("phase13f_stream_stage_logs");
    let memory_path = base_dir.join("memory.md");
    let memory_store = Arc::new(MarkdownMemoryStore::new(&memory_path).await.unwrap());
    let provider = Arc::new(RecordingProvider::new());
    let channel = Arc::new(QueueChannel::new(vec![Message::text(
        Role::User,
        "Recall my memory",
        None,
    )]));
    let mut engine = Engine::new(provider, channel.clone())
        .with_assistant_profile("Forja".to_string(), "User".to_string())
        .with_system_prompt("base system prompt".to_string())
        .with_memory(memory_store);
    let channel_for_shutdown = channel.clone();

    engine
        .run_streaming(async move {
            for _ in 0..50 {
                if channel_for_shutdown.sent_count().await >= 1 {
                    break;
                }
                tokio::time::sleep(tokio::time::Duration::from_millis(20)).await;
            }
        })
        .await
        .unwrap();

    let logged_lines = channel.logged_lines().await;

    assert_eq!(
        logged_lines,
        vec![
            "\u{1b}[36m• Loading emotion context...\u{1b}[0m".to_string(),
            "\u{1b}[33m• Loading knowledge...\u{1b}[0m".to_string(),
            "\u{1b}[35m• Loading memory...\u{1b}[0m".to_string(),
            "\u{1b}[32m• Calling LLM...\u{1b}[0m".to_string(),
            "\u{1b}[32m• Calling LLM...\u{1b}[0m".to_string(),
        ]
    );

    let _ = tokio::fs::remove_dir_all(&base_dir).await;
}

#[tokio::test]
async fn empty_memory_md_starts_without_errors() {
    let base_dir = unique_temp_dir("phase13f_empty");
    let memory_path = base_dir.join("memory.md");

    let store = MarkdownMemoryStore::new(&memory_path).await.unwrap();
    let contents = store.load_all().await.unwrap();

    assert_eq!(contents, "");
    assert!(base_dir.join("index.md").exists());
    assert!(base_dir.join("topics").exists());
    assert!(base_dir.join("daily").exists());

    let _ = tokio::fs::remove_dir_all(&base_dir).await;
}

#[tokio::test]
async fn legacy_sessions_are_migrated_to_memory_md_once() {
    let base_dir = unique_temp_dir("phase13f_migrate");
    let sessions_dir = base_dir.join("sessions");
    std::fs::create_dir_all(&sessions_dir).unwrap();

    write_legacy_session(
        &sessions_dir.join("001.md"),
        "legacy-1",
        60,
        "user",
        "legacy user message",
    );
    write_legacy_session(
        &sessions_dir.join("002.md"),
        "legacy-2",
        120,
        "assistant",
        "legacy assistant message",
    );

    let memory_path = base_dir.join("memory.md");
    let store = MarkdownMemoryStore::new(&memory_path).await.unwrap();
    let contents = store.load_all().await.unwrap();

    assert!(contents.contains("| user | legacy user message"));
    assert!(contents.contains("| assistant | legacy assistant message"));
    assert!(!base_dir.join("sessions").exists());
    assert!(base_dir.join("sessions.bak").exists());

    let _ = tokio::fs::remove_dir_all(&base_dir).await;
}
