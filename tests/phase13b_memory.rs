use async_trait::async_trait;
use forja_core::error::Result;
use forja_core::traits::MemoryStore;
use forja_core::types::MemoryEntry;
use forja_core::{Channel, Content, Engine, LlmProvider, Message, Role, ToolDefinition};
use std::collections::HashMap;
use std::future::pending;
use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::{Mutex, oneshot};
use tokio_stream::Stream;

fn memory_entry(id: &str, content: &str, score: f64) -> MemoryEntry {
    MemoryEntry {
        id: id.to_string(),
        content: content.to_string(),
        score,
        timestamp: 1,
        tags: vec!["memory".to_string()],
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

struct SearchMemoryStore {
    by_query: HashMap<String, Vec<MemoryEntry>>,
}

impl SearchMemoryStore {
    fn new(by_query: HashMap<String, Vec<MemoryEntry>>) -> Self {
        Self { by_query }
    }
}

#[async_trait]
impl MemoryStore for SearchMemoryStore {
    async fn save(&self, _entry: &MemoryEntry) -> Result<()> {
        Ok(())
    }

    async fn search(&self, query: &str, _limit: usize) -> Result<Vec<MemoryEntry>> {
        Ok(self.by_query.get(query).cloned().unwrap_or_default())
    }

    async fn flush(&self) -> Result<()> {
        Ok(())
    }
}

struct RecordingProvider {
    chat_requests: Mutex<Vec<Vec<Message>>>,
    stream_requests: Mutex<Vec<Vec<Message>>>,
}

impl RecordingProvider {
    fn new() -> Self {
        Self {
            chat_requests: Mutex::new(Vec::new()),
            stream_requests: Mutex::new(Vec::new()),
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

    async fn stream_texts(&self) -> Vec<String> {
        self.stream_requests
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
        messages: &[Message],
        _tools: Option<&[ToolDefinition]>,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<String>> + Send>>> {
        self.stream_requests.lock().await.push(messages.to_vec());
        let chunks = vec![Ok("stream ".to_string()), Ok("response".to_string())];
        Ok(Box::pin(tokio_stream::iter(chunks)))
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
async fn chat_injects_memory_context_when_search_returns_matches() {
    let provider = Arc::new(RecordingProvider::new());
    let channel = Arc::new(QueueChannel::new(vec![Message::text(
        Role::User,
        "favorite color",
        None,
    )]));
    let memory = Arc::new(SearchMemoryStore::new(HashMap::from([(
        "favorite color".to_string(),
        vec![memory_entry("m1", "The user previously said blue is calming.", 3.2)],
    )])));

    let mut engine = Engine::new(provider.clone(), channel.clone())
        .with_system_prompt("base system prompt".to_string())
        .with_memory(memory);
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
    assert!(requests[0].contains("The user previously said blue is calming."));
    assert!(requests[0].contains("base system prompt"));
}

#[tokio::test]
async fn chat_keeps_existing_flow_when_search_returns_no_matches() {
    let provider = Arc::new(RecordingProvider::new());
    let channel = Arc::new(QueueChannel::new(vec![Message::text(
        Role::User,
        "no memory query",
        None,
    )]));
    let memory = Arc::new(SearchMemoryStore::new(HashMap::new()));

    let mut engine = Engine::new(provider.clone(), channel.clone())
        .with_system_prompt("base system prompt".to_string())
        .with_memory(memory);
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
    assert!(!requests[0].contains("Relevant memory context"));
    assert!(requests[0].contains("base system prompt"));
    assert!(requests[0].contains("no memory query"));
}

#[tokio::test]
async fn chat_memory_context_is_transient_per_turn() {
    let provider = Arc::new(RecordingProvider::new());
    let channel = Arc::new(QueueChannel::new(vec![
        Message::text(Role::User, "first query", None),
        Message::text(Role::User, "second query", None),
    ]));
    let memory = Arc::new(SearchMemoryStore::new(HashMap::from([(
        "first query".to_string(),
        vec![memory_entry("m1", "Memory for first turn only.", 4.5)],
    )])));

    let mut engine = Engine::new(provider.clone(), channel.clone()).with_memory(memory);
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
        tokio::time::sleep(tokio::time::Duration::from_millis(20)).await;
    }

    shutdown_tx.send(()).unwrap();
    run_handle.await.unwrap();

    let requests = provider.chat_texts().await;
    assert_eq!(requests.len(), 2);
    assert!(requests[0].contains("Relevant memory context"));
    assert!(requests[0].contains("Memory for first turn only."));
    assert!(!requests[1].contains("Relevant memory context"));
    assert!(!requests[1].contains("Memory for first turn only."));
}

#[tokio::test]
async fn streaming_path_injects_memory_context_too() {
    let provider = Arc::new(RecordingProvider::new());
    let channel = Arc::new(QueueChannel::new(vec![Message::text(
        Role::User,
        "stream query",
        None,
    )]));
    let memory = Arc::new(SearchMemoryStore::new(HashMap::from([(
        "stream query".to_string(),
        vec![memory_entry("m1", "Streaming should also see memory context.", 2.7)],
    )])));

    let mut engine = Engine::new(provider.clone(), channel.clone()).with_memory(memory);
    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();

    let run_handle = tokio::spawn(async move {
        engine
            .run_streaming(async {
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

    let requests = provider.stream_texts().await;
    assert_eq!(requests.len(), 1);
    assert!(requests[0].contains("Relevant memory context"));
    assert!(requests[0].contains("Streaming should also see memory context."));
}
