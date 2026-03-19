use async_trait::async_trait;
use forja_core::error::Result;
use forja_core::traits::MemoryStore;
use forja_core::types::MemoryEntry;
use forja_core::{Channel, Content, Engine, LlmProvider, Message, Role, ToolDefinition};
use forja_memory::{MarkdownMemoryStore, MemoryLoadOptions};
use std::future::pending;
use std::path::PathBuf;
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

fn entry(id: &str, content: &str, timestamp: u64) -> MemoryEntry {
    MemoryEntry {
        id: id.to_string(),
        content: content.to_string(),
        score: 0.0,
        timestamp,
        tags: vec!["phase13c".to_string()],
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
        Err(forja_core::error::ForjaError::LlmError(
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

#[tokio::test]
async fn empty_memory_directory_starts_with_empty_index() {
    let base_dir = unique_temp_dir("phase13c_empty");
    let store = MarkdownMemoryStore::new_with_options(&base_dir, MemoryLoadOptions::default())
        .await
        .unwrap();

    let results = store.search("no files yet", 5).await.unwrap();

    assert!(results.is_empty());

    let _ = tokio::fs::remove_dir_all(&base_dir).await;
}

#[tokio::test]
async fn restart_loads_previous_session_entries_into_bootstrap_index() {
    let base_dir = unique_temp_dir("phase13c_restart");
    let initial_store = MarkdownMemoryStore::new(&base_dir).await.unwrap();
    initial_store
        .save(&entry(
            "entry-1",
            "bootstrap restart memory should be searchable after restart",
            1,
        ))
        .await
        .unwrap();
    drop(initial_store);

    let restarted_store =
        MarkdownMemoryStore::new_with_options(&base_dir, MemoryLoadOptions::default())
            .await
            .unwrap();

    let results = restarted_store
        .search("bootstrap searchable restart", 5)
        .await
        .unwrap();

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].id, "entry-1");

    let _ = tokio::fs::remove_dir_all(&base_dir).await;
}

#[tokio::test]
async fn bootstrap_search_uses_loaded_index_even_if_source_file_changes_after_startup() {
    let base_dir = unique_temp_dir("phase13c_bootstrap_cache");
    let initial_store = MarkdownMemoryStore::new(&base_dir).await.unwrap();
    initial_store
        .save(&entry(
            "entry-1",
            "loaded on startup and kept in bm25 cache",
            1,
        ))
        .await
        .unwrap();
    drop(initial_store);

    let restarted_store =
        MarkdownMemoryStore::new_with_options(&base_dir, MemoryLoadOptions::default())
            .await
            .unwrap();

    tokio::fs::remove_file(base_dir.join("sessions").join("entry-1.md"))
        .await
        .unwrap();

    let results = restarted_store.search("startup bm25 cache", 5).await.unwrap();

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].id, "entry-1");

    let _ = tokio::fs::remove_dir_all(&base_dir).await;
}

#[tokio::test]
async fn bootstrap_respects_max_files_limit() {
    let base_dir = unique_temp_dir("phase13c_max_files");
    let initial_store = MarkdownMemoryStore::new(&base_dir).await.unwrap();
    initial_store
        .save(&entry("entry-1", "oldest bootstrap term", 1))
        .await
        .unwrap();
    initial_store
        .save(&entry("entry-2", "middle bootstrap term", 2))
        .await
        .unwrap();
    initial_store
        .save(&entry("entry-3", "newest bootstrap term", 3))
        .await
        .unwrap();
    drop(initial_store);

    let restarted_store = MarkdownMemoryStore::new_with_options(
        &base_dir,
        MemoryLoadOptions {
            max_files: Some(2),
            recent_days: None,
        },
    )
    .await
    .unwrap();

    let old_results = restarted_store.search("oldest", 5).await.unwrap();
    let new_results = restarted_store.search("newest", 5).await.unwrap();

    assert!(old_results.is_empty());
    assert_eq!(new_results.len(), 1);
    assert_eq!(new_results[0].id, "entry-3");

    let _ = tokio::fs::remove_dir_all(&base_dir).await;
}

#[tokio::test]
async fn bootstrap_respects_recent_days_limit() {
    let base_dir = unique_temp_dir("phase13c_recent_days");
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let initial_store = MarkdownMemoryStore::new(&base_dir).await.unwrap();
    initial_store
        .save(&entry(
            "entry-1",
            "stale bootstrap term",
            now.saturating_sub(10 * 24 * 60 * 60),
        ))
        .await
        .unwrap();
    initial_store
        .save(&entry("entry-2", "recent bootstrap term", now))
        .await
        .unwrap();
    drop(initial_store);

    let restarted_store = MarkdownMemoryStore::new_with_options(
        &base_dir,
        MemoryLoadOptions {
            max_files: Some(500),
            recent_days: Some(3),
        },
    )
    .await
    .unwrap();

    let stale_results = restarted_store.search("stale", 5).await.unwrap();
    let recent_results = restarted_store.search("recent", 5).await.unwrap();

    assert!(stale_results.is_empty());
    assert_eq!(recent_results.len(), 1);
    assert_eq!(recent_results[0].id, "entry-2");

    let _ = tokio::fs::remove_dir_all(&base_dir).await;
}

#[tokio::test]
async fn engine_uses_bootstrapped_memory_after_restart() {
    let base_dir = unique_temp_dir("phase13c_engine");
    let initial_store = MarkdownMemoryStore::new(&base_dir).await.unwrap();
    initial_store
        .save(&entry(
            "entry-1",
            "The user likes oolong tea in prior sessions.",
            1,
        ))
        .await
        .unwrap();
    drop(initial_store);

    let restarted_store = Arc::new(
        MarkdownMemoryStore::new_with_options(
            &base_dir,
            MemoryLoadOptions {
                max_files: Some(500),
                recent_days: None,
            },
        )
        .await
        .unwrap(),
    );
    let provider = Arc::new(RecordingProvider::new());
    let channel = Arc::new(QueueChannel::new(vec![Message::text(
        Role::User,
        "What tea do I like?",
        None,
    )]));
    let mut engine = Engine::new(provider.clone(), channel.clone()).with_memory(restarted_store);
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
    assert!(requests[0].contains("Relevant memory context"));
    assert!(requests[0].contains("The user likes oolong tea in prior sessions."));

    let _ = tokio::fs::remove_dir_all(&base_dir).await;
}
