use async_trait::async_trait;
use chrono::{Duration as ChronoDuration, Local};
use forja_core::engine::{DreamRuntimeConfig, SlashCommandResult};
use forja_core::error::{ForjaError, Result};
use forja_core::traits::{DreamRunStatus, DreamTrigger, MemoryStore};
use forja_core::types::MemoryEntry;
use forja_core::{Channel, Content, Engine, LlmProvider, Message, Role, ToolDefinition};
use forja_memory::MarkdownMemoryStore;
use std::future::pending;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};
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

fn write_topic_file(base_dir: &Path, name: &str, lines: &[String]) {
    let body = std::iter::once(format!("# Topic: {}", name.trim_end_matches(".md")))
        .chain(lines.iter().cloned())
        .collect::<Vec<_>>()
        .join("\n");
    std::fs::create_dir_all(base_dir.join("topics")).unwrap();
    std::fs::write(base_dir.join("topics").join(name), format!("{body}\n")).unwrap();
}

fn write_daily_file(base_dir: &Path, days_ago: i64, lines: &[&str]) {
    let date = (Local::now().date_naive() - ChronoDuration::days(days_ago))
        .format("%Y-%m-%d")
        .to_string();
    std::fs::create_dir_all(base_dir.join("daily")).unwrap();
    std::fs::write(
        base_dir.join("daily").join(format!("{date}.md")),
        format!("{}\n", lines.join("\n")),
    )
    .unwrap();
}

fn topic_line(days_ago: i64, text: &str) -> String {
    let date = (Local::now().date_naive() - ChronoDuration::days(days_ago))
        .format("%Y-%m-%d")
        .to_string();
    format!("- [{date} 12:00] user | {text}")
}

fn dream_log_path(base_dir: &Path) -> PathBuf {
    let date = Local::now().format("%Y-%m-%d").to_string();
    base_dir.join("dreams").join(format!("{date}.md"))
}

struct RecordingProvider;

#[async_trait]
impl LlmProvider for RecordingProvider {
    async fn chat(
        &self,
        _messages: &[Message],
        _tools: Option<&[ToolDefinition]>,
    ) -> Result<Message> {
        Ok(Message::text(Role::Assistant, "ok", None))
    }

    async fn stream(
        &self,
        _messages: &[Message],
        _tools: Option<&[ToolDefinition]>,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<String>> + Send>>> {
        Err(ForjaError::LlmError(
            "stream not used in dream tests".to_string(),
        ))
    }
}

struct QueueChannel {
    messages: Mutex<Vec<Message>>,
    sent_messages: Mutex<Vec<String>>,
}

impl QueueChannel {
    fn new(messages: Vec<Message>) -> Self {
        Self {
            messages: Mutex::new(messages.into_iter().rev().collect()),
            sent_messages: Mutex::new(Vec::new()),
        }
    }

    async fn sent_messages(&self) -> Vec<String> {
        self.sent_messages.lock().await.clone()
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
        let text = match message.content {
            Content::Text { text, .. } => text,
            _ => String::new(),
        };
        self.sent_messages.lock().await.push(text);
        Ok(())
    }
}

#[derive(Default)]
struct DreamRecordingStore {
    dream_calls: Mutex<Vec<DreamTrigger>>,
    sleep_for: Duration,
    latest_dream_unix_secs: AtomicU64,
}

impl DreamRecordingStore {
    fn with_sleep(sleep_for: Duration) -> Self {
        Self {
            sleep_for,
            ..Self::default()
        }
    }

    fn with_latest_dream(unix_secs: u64) -> Self {
        Self {
            latest_dream_unix_secs: AtomicU64::new(unix_secs),
            ..Self::default()
        }
    }

    async fn dream_calls(&self) -> Vec<DreamTrigger> {
        self.dream_calls.lock().await.clone()
    }
}

#[async_trait]
impl MemoryStore for DreamRecordingStore {
    async fn save(&self, _entry: &MemoryEntry) -> Result<()> {
        Ok(())
    }

    async fn load_all(&self) -> Result<String> {
        Ok(String::new())
    }

    async fn flush(&self) -> Result<()> {
        Ok(())
    }

    async fn latest_dream_timestamp(&self) -> Result<Option<u64>> {
        let value = self.latest_dream_unix_secs.load(Ordering::SeqCst);
        Ok((value > 0).then_some(value))
    }

    async fn run_dream(
        &self,
        trigger: DreamTrigger,
    ) -> Result<forja_core::traits::DreamRunOutcome> {
        self.dream_calls.lock().await.push(trigger);
        if !self.sleep_for.is_zero() {
            tokio::time::sleep(self.sleep_for).await;
        }
        let completed_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        self.latest_dream_unix_secs
            .store(completed_at, Ordering::SeqCst);
        Ok(forja_core::traits::DreamRunOutcome {
            status: DreamRunStatus::Completed,
            summary: format!("completed {trigger:?} dream"),
            archived_topics: Vec::new(),
            merged_topics: Vec::new(),
            split_topics: Vec::new(),
            completed_at: Some(completed_at),
        })
    }
}

#[tokio::test]
async fn markdown_memory_store_creates_dream_directory_on_init() {
    let base_dir = unique_temp_dir("phase20_dream_layout");
    let memory_path = base_dir.join("memory.md");

    let _store = MarkdownMemoryStore::new(&memory_path).await.unwrap();

    assert!(base_dir.join("dreams").exists());

    let _ = tokio::fs::remove_dir_all(&base_dir).await;
}

#[tokio::test]
async fn manual_dream_archives_stale_topics_and_appends_dream_log() {
    let base_dir = unique_temp_dir("phase20_dream_stale");
    let memory_path = base_dir.join("memory.md");
    let store = MarkdownMemoryStore::new(&memory_path).await.unwrap();

    write_topic_file(
        &base_dir,
        "obsolete-topic.md",
        &[topic_line(40, "Old project note with no recent evidence.")],
    );
    store.flush().await.unwrap();

    let first = store.run_dream(DreamTrigger::Manual).await.unwrap();
    let first_log = std::fs::read_to_string(dream_log_path(&base_dir)).unwrap();

    assert_eq!(first.status, DreamRunStatus::Completed);
    assert!(!base_dir.join("topics").join("obsolete-topic.md").exists());
    assert!(first_log.contains("obsolete-topic"));

    let second = store.run_dream(DreamTrigger::Manual).await.unwrap();
    let second_log = std::fs::read_to_string(dream_log_path(&base_dir)).unwrap();

    assert_eq!(second.status, DreamRunStatus::Completed);
    assert!(second_log.len() > first_log.len());

    let _ = tokio::fs::remove_dir_all(&base_dir).await;
}

#[tokio::test]
async fn manual_dream_keeps_old_topics_with_recent_daily_evidence() {
    let base_dir = unique_temp_dir("phase20_dream_recent_daily");
    let memory_path = base_dir.join("memory.md");
    let store = MarkdownMemoryStore::new(&memory_path).await.unwrap();

    write_topic_file(
        &base_dir,
        "project-atlas.md",
        &[topic_line(
            40,
            "Project Atlas historical implementation note.",
        )],
    );
    write_daily_file(
        &base_dir,
        2,
        &["10:10 | user | Project Atlas still matters for the dashboard refactor."],
    );
    store.flush().await.unwrap();

    let outcome = store.run_dream(DreamTrigger::Manual).await.unwrap();

    assert_eq!(outcome.status, DreamRunStatus::Completed);
    assert!(base_dir.join("topics").join("project-atlas.md").exists());
    assert!(
        !std::fs::read_to_string(dream_log_path(&base_dir))
            .unwrap()
            .contains("archived stale topic: project-atlas")
    );

    let _ = tokio::fs::remove_dir_all(&base_dir).await;
}

#[tokio::test]
async fn manual_dream_merges_duplicate_topics_by_slug_overlap() {
    let base_dir = unique_temp_dir("phase20_dream_merge");
    let memory_path = base_dir.join("memory.md");
    let store = MarkdownMemoryStore::new(&memory_path).await.unwrap();

    write_topic_file(
        &base_dir,
        "project-atlas.md",
        &[topic_line(2, "Project Atlas delivery note.")],
    );
    write_topic_file(
        &base_dir,
        "atlas-project.md",
        &[topic_line(2, "Atlas project duplicate note.")],
    );
    store.flush().await.unwrap();

    let outcome = store.run_dream(DreamTrigger::Manual).await.unwrap();
    let active_names = std::fs::read_dir(base_dir.join("topics"))
        .unwrap()
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.file_name().to_string_lossy().to_string())
        .collect::<Vec<_>>();
    let archived_names = std::fs::read_dir(base_dir.join("archive"))
        .unwrap()
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.file_name().to_string_lossy().to_string())
        .collect::<Vec<_>>();

    assert_eq!(outcome.status, DreamRunStatus::Completed);
    assert_eq!(outcome.merged_topics.len(), 1);
    assert_eq!(
        active_names
            .iter()
            .filter(|name| name.starts_with("project-atlas") || name.starts_with("atlas-project"))
            .count(),
        1
    );
    assert_eq!(archived_names.len(), 1);

    let _ = tokio::fs::remove_dir_all(&base_dir).await;
}

#[tokio::test]
async fn manual_dream_splits_topics_that_exceed_budget() {
    let base_dir = unique_temp_dir("phase20_dream_split");
    let memory_path = base_dir.join("memory.md");
    let store = MarkdownMemoryStore::new(&memory_path).await.unwrap();
    let large_topic = (0..120)
        .map(|index| {
            topic_line(
                1,
                &format!(
                    "Project Atlas oversized note {index} {}",
                    "alpha ".repeat(14)
                ),
            )
        })
        .collect::<Vec<_>>();

    write_topic_file(&base_dir, "project-atlas.md", &large_topic);
    store.flush().await.unwrap();

    let outcome = store.run_dream(DreamTrigger::Manual).await.unwrap();
    let shard_count = std::fs::read_dir(base_dir.join("topics"))
        .unwrap()
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.file_name().to_string_lossy().to_string())
        .filter(|name| name.starts_with("project-atlas"))
        .count();
    let index = std::fs::read_to_string(base_dir.join("index.md")).unwrap();

    assert_eq!(outcome.status, DreamRunStatus::Completed);
    assert!(shard_count > 1);
    assert!(index.contains("project-atlas"));
    assert!(index.contains("shards="));

    let _ = tokio::fs::remove_dir_all(&base_dir).await;
}

#[tokio::test]
async fn engine_starts_idle_dream_in_background() {
    let provider = Arc::new(RecordingProvider);
    let channel = Arc::new(QueueChannel::new(Vec::new()));
    let memory = Arc::new(DreamRecordingStore::default());
    let mut engine = Engine::new(provider, channel)
        .with_memory(memory.clone())
        .with_dream_runtime(DreamRuntimeConfig {
            enabled: true,
            idle_after: Duration::from_millis(30),
            shutdown_after: Duration::from_secs(3600),
        });
    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
    let run = engine.run_streaming(async {
        let _ = shutdown_rx.await;
    });
    tokio::pin!(run);
    let monitor = async {
        for _ in 0..50 {
            if !memory.dream_calls().await.is_empty() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        shutdown_tx.send(()).unwrap();
    };
    tokio::pin!(monitor);
    tokio::select! {
        result = &mut run => {
            result.unwrap();
        }
        _ = &mut monitor => {
            run.await.unwrap();
        }
    }

    assert_eq!(memory.dream_calls().await, vec![DreamTrigger::Idle]);
}

#[tokio::test]
async fn engine_rejects_manual_dream_when_one_is_already_running() {
    let provider = Arc::new(RecordingProvider);
    let channel = Arc::new(QueueChannel::new(vec![
        Message::text(Role::User, "/dream", None),
        Message::text(Role::User, "/dream", None),
    ]));
    let memory = Arc::new(DreamRecordingStore::with_sleep(Duration::from_millis(120)));
    let mut engine = Engine::new(provider, channel.clone())
        .with_memory(memory.clone())
        .with_dream_runtime(DreamRuntimeConfig {
            enabled: true,
            idle_after: Duration::from_secs(3600),
            shutdown_after: Duration::from_secs(3600),
        })
        .with_slash_handler(Arc::new(|text: &str, _, _| {
            if text.trim() == "/dream" {
                Some(SlashCommandResult::Dream)
            } else {
                None
            }
        }));
    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
    let run = engine.run_streaming(async {
        let _ = shutdown_rx.await;
    });
    tokio::pin!(run);
    let monitor = async {
        tokio::time::sleep(Duration::from_millis(80)).await;
        shutdown_tx.send(()).unwrap();
    };
    tokio::pin!(monitor);
    tokio::select! {
        result = &mut run => {
            result.unwrap();
        }
        _ = &mut monitor => {
            run.await.unwrap();
        }
    }

    let sent = channel.sent_messages().await;
    assert!(sent.iter().any(|message| message.contains("Dream started")));
    assert!(
        sent.iter()
            .any(|message| message.contains("already in progress"))
    );
    assert_eq!(memory.dream_calls().await, vec![DreamTrigger::Manual]);
}

#[tokio::test]
async fn engine_runs_shutdown_dream_when_last_run_is_stale() {
    let provider = Arc::new(RecordingProvider);
    let channel = Arc::new(QueueChannel::new(Vec::new()));
    let stale_timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        .saturating_sub(7200);
    let memory = Arc::new(DreamRecordingStore::with_latest_dream(stale_timestamp));
    let mut engine = Engine::new(provider, channel)
        .with_memory(memory.clone())
        .with_dream_runtime(DreamRuntimeConfig {
            enabled: true,
            idle_after: Duration::from_secs(3600),
            shutdown_after: Duration::from_secs(3600),
        });

    engine.run(async {}).await.unwrap();

    assert_eq!(memory.dream_calls().await, vec![DreamTrigger::Shutdown]);
}
