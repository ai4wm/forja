use async_trait::async_trait;
use forja_core::error::{ForjaError, Result};
use forja_core::{Channel, Engine, LlmProvider, Message, Role, ToolDefinition};
use forja_core::traits::MemoryStore;
use forja_core::types::MemoryEntry;
use forja_memory::MarkdownMemoryStore;
use std::future::pending;
use std::io::Write;
use std::path::PathBuf;
use std::pin::Pin;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
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
        tags: vec!["phase13a".to_string()],
        metadata: Default::default(),
    }
}

struct TestProvider;

#[async_trait]
impl LlmProvider for TestProvider {
    async fn chat(
        &self,
        _messages: &[Message],
        _tools: Option<&[ToolDefinition]>,
    ) -> Result<Message> {
        Ok(Message::text(Role::Assistant, "mock response", None))
    }

    async fn stream(
        &self,
        _messages: &[Message],
        _tools: Option<&[ToolDefinition]>,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<String>> + Send>>> {
        Err(ForjaError::LlmError("stream not used in this test".to_string()))
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
async fn save_then_search_returns_saved_entry() {
    let base_dir = unique_temp_dir("round_trip");
    let store = MarkdownMemoryStore::new(&base_dir).await.unwrap();

    store
        .save(&entry(
            "entry-1",
            "phase13a memory activation should preserve this text",
            1,
        ))
        .await
        .unwrap();

    let results = store.search("activation preserve", 5).await.unwrap();

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].id, "entry-1");
    assert!(results[0].content.contains("phase13a memory activation"));

    let _ = tokio::fs::remove_dir_all(&base_dir).await;
}

#[tokio::test]
async fn archive_keeps_latest_session_and_searches_archived_entries() {
    let base_dir = unique_temp_dir("archive");
    let store = MarkdownMemoryStore::new(&base_dir).await.unwrap();

    store
        .save(&entry("entry-1", "alpha archived context", 1))
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(20)).await;

    store
        .save(&entry("entry-2", "beta archived context", 2))
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(20)).await;

    store
        .save(&entry("entry-3", "gamma latest context", 3))
        .await
        .unwrap();

    store.archive_old_files(1).await.unwrap();

    let sessions_count = std::fs::read_dir(base_dir.join("sessions"))
        .unwrap()
        .count();
    let fragments_count = std::fs::read_dir(base_dir.join("fragments"))
        .unwrap()
        .count();

    assert_eq!(sessions_count, 1);
    assert_eq!(fragments_count, 2);

    let results = store.search("alpha", 5).await.unwrap();

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].id, "entry-1");

    let _ = tokio::fs::remove_dir_all(&base_dir).await;
}

#[tokio::test]
async fn engine_memory_saves_distinct_entries_for_fast_consecutive_turns() {
    let base_dir = unique_temp_dir("engine_ids");
    let store = Arc::new(MarkdownMemoryStore::new(&base_dir).await.unwrap());
    let channel = Arc::new(QueueChannel::new(vec![
        Message::text(Role::User, "first turn", None),
        Message::text(Role::User, "second turn", None),
    ]));
    let provider = Arc::new(TestProvider);

    while SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_millis()
        > 100
    {
        tokio::time::sleep(Duration::from_millis(5)).await;
    }

    let mut engine = Engine::new(provider, channel.clone()).with_memory(store);
    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();

    let run_handle = tokio::spawn(async move {
        engine
            .run(async {
                let _ = shutdown_rx.await;
            })
            .await
            .unwrap();
    });

    for _ in 0..100 {
        if channel.sent_count().await >= 2 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    shutdown_tx.send(()).unwrap();
    run_handle.await.unwrap();

    let sessions_count = std::fs::read_dir(base_dir.join("sessions"))
        .unwrap()
        .count();

    assert_eq!(sessions_count, 4);

    let _ = tokio::fs::remove_dir_all(&base_dir).await;
}

#[tokio::test]
async fn mock_runtime_creates_memory_session_files() {
    let home_dir = unique_temp_dir("runtime_home");
    std::fs::create_dir_all(&home_dir).unwrap();
    let memory_dir = home_dir.join(".forja").join("memory");

    let mut child = Command::new(env!("CARGO_BIN_EXE_forja"))
        .current_dir(&home_dir)
        .env("FORJA_USE_MOCK", "1")
        .env("FORJA_PROVIDER", "ollama")
        .env("FORJA_MODEL", "qwen3.5:9b")
        .env("FORJA_MEMORY_DIR", &memory_dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();

    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(b"phase13a runtime memory test\n")
        .unwrap();
    child.stdin.as_mut().unwrap().flush().unwrap();

    let sessions_dir = memory_dir.join("sessions");
    let mut created = false;

    for _ in 0..50 {
        if sessions_dir.exists() {
            let file_count = std::fs::read_dir(&sessions_dir).unwrap().count();
            if file_count >= 1 {
                created = true;
                break;
            }
        }

        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    let _ = child.kill();
    let _ = child.wait();

    assert!(created);

    let _ = tokio::fs::remove_dir_all(&home_dir).await;
}
