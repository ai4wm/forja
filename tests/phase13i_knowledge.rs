use async_trait::async_trait;
use chrono::Local;
use forja_core::error::{ForjaError, Result};
use forja_core::knowledge::{KnowledgeManager, TopicEntry};
use forja_core::{Channel, Content, Engine, LlmProvider, Message, Role, ToolDefinition};
use std::collections::VecDeque;
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

#[tokio::test]
async fn detect_topic_parses_topic_entry_from_llm_json() {
    let base_dir = unique_temp_dir("phase13i_detect_json");
    let manager = KnowledgeManager::new(base_dir.clone());
    let provider = ScriptedProvider::new(vec![ProviderStep::Text(
        r#"{"topic":"projects","filename":"projects.md","entry":"Forja is a Rust AI agent engine"}"#.to_string(),
    )]);

    let topic = manager
        .detect_topic("Forja project details", &provider)
        .await
        .unwrap();

    assert_eq!(
        topic,
        Some(TopicEntry {
            topic: "projects".to_string(),
            filename: "projects.md".to_string(),
            entry: "Forja is a Rust AI agent engine".to_string(),
        })
    );

    let _ = tokio::fs::remove_dir_all(&base_dir).await;
}

#[tokio::test]
async fn detect_topic_returns_none_for_none_response() {
    let base_dir = unique_temp_dir("phase13i_detect_none");
    let manager = KnowledgeManager::new(base_dir.clone());
    let provider = ScriptedProvider::new(vec![ProviderStep::Text("NONE".to_string())]);

    let topic = manager
        .detect_topic("Just a casual greeting", &provider)
        .await
        .unwrap();

    assert_eq!(topic, None);

    let _ = tokio::fs::remove_dir_all(&base_dir).await;
}

#[tokio::test]
async fn detect_topic_returns_none_on_provider_failure() {
    let base_dir = unique_temp_dir("phase13i_detect_error");
    let manager = KnowledgeManager::new(base_dir.clone());
    let provider = ScriptedProvider::new(vec![ProviderStep::Error(
        "knowledge detection failed".to_string(),
    )]);

    let topic = manager
        .detect_topic("Remember this project choice", &provider)
        .await
        .unwrap();

    assert_eq!(topic, None);

    let _ = tokio::fs::remove_dir_all(&base_dir).await;
}

#[test]
fn save_entry_appends_to_existing_topic_file() {
    let base_dir = unique_temp_dir("phase13i_save_append");
    let manager = KnowledgeManager::new(base_dir.clone());
    let entry = TopicEntry {
        topic: "projects".to_string(),
        filename: "projects.md".to_string(),
        entry: "Forja phase 14 is active".to_string(),
    };

    manager.save_entry(&entry).unwrap();
    manager.save_entry(&entry).unwrap();

    let contents = std::fs::read_to_string(base_dir.join("projects.md")).unwrap();
    let today = Local::now().format("%Y-%m-%d").to_string();

    assert_eq!(contents.matches("Forja phase 14 is active").count(), 2);
    assert!(contents.contains(&today));

    let _ = std::fs::remove_dir_all(&base_dir);
}

#[test]
fn save_entry_creates_missing_topic_file() {
    let base_dir = unique_temp_dir("phase13i_save_create");
    let manager = KnowledgeManager::new(base_dir.clone());
    let entry = TopicEntry {
        topic: "infra".to_string(),
        filename: "infra.md".to_string(),
        entry: "S25 Ultra is connected via ADB".to_string(),
    };

    manager.save_entry(&entry).unwrap();

    let file_path = base_dir.join("infra.md");
    let contents = std::fs::read_to_string(&file_path).unwrap();

    assert!(file_path.exists());
    assert!(contents.contains("S25 Ultra is connected via ADB"));

    let _ = std::fs::remove_dir_all(&base_dir);
}

#[test]
fn load_relevant_reads_only_matching_files() {
    let base_dir = unique_temp_dir("phase13i_load_match");
    std::fs::create_dir_all(&base_dir).unwrap();
    std::fs::write(
        base_dir.join("projects.md"),
        "- [2026-03-26] Forja is a Rust AI agent engine\n",
    )
    .unwrap();
    std::fs::write(
        base_dir.join("infra.md"),
        "- [2026-03-26] S25 Ultra uses Tailscale\n",
    )
    .unwrap();
    let manager = KnowledgeManager::new(base_dir.clone());

    let loaded = manager.load_relevant("projects roadmap").unwrap();

    assert!(loaded.contains("## projects.md"));
    assert!(loaded.contains("Forja is a Rust AI agent engine"));
    assert!(!loaded.contains("## infra.md"));
    assert!(!loaded.contains("S25 Ultra uses Tailscale"));

    let _ = std::fs::remove_dir_all(&base_dir);
}

#[test]
fn load_relevant_returns_empty_when_no_file_matches() {
    let base_dir = unique_temp_dir("phase13i_load_empty");
    std::fs::create_dir_all(&base_dir).unwrap();
    std::fs::write(
        base_dir.join("people.md"),
        "- [2026-03-26] Alex handles release notes\n",
    )
    .unwrap();
    let manager = KnowledgeManager::new(base_dir.clone());

    let loaded = manager.load_relevant("database sharding plan").unwrap();

    assert!(loaded.is_empty());

    let _ = std::fs::remove_dir_all(&base_dir);
}

#[test]
fn list_files_creates_knowledge_directory_when_missing() {
    let base_dir = unique_temp_dir("phase13i_list_create");
    let manager = KnowledgeManager::new(base_dir.clone());

    let files = manager.list_files();

    assert!(base_dir.exists());
    assert!(files.is_empty());

    let _ = std::fs::remove_dir_all(&base_dir);
}

#[tokio::test]
async fn engine_injects_relevant_knowledge_into_system_prompt() {
    let base_dir = unique_temp_dir("phase13i_engine_context");
    let knowledge = Arc::new(KnowledgeManager::new(base_dir.clone()));
    let provider = Arc::new(ScriptedProvider::new(vec![
        ProviderStep::Text(
            r#"{"topic":"projects","filename":"projects.md","entry":"Forja tracks long-term project state"}"#.to_string(),
        ),
        ProviderStep::Text("assistant reply".to_string()),
    ]));
    let channel = Arc::new(QueueChannel::new(vec![Message::text(
        Role::User,
        "Share the projects status",
        None,
    )]));
    let mut engine = Engine::new(provider.clone(), channel.clone())
        .with_system_prompt("[identity.md]\nForja\n\n[user.md]\nOwner".to_string())
        .with_knowledge(knowledge);
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
    let main_request = &requests[1];

    assert!(main_request.contains("System:You are 황비서, a personal AI assistant."));
    assert!(main_request.contains("[identity.md]"));
    assert!(main_request.contains("[knowledge - Topic-based Persistent Knowledge]"));
    assert!(main_request.contains("## projects.md"));
    assert!(main_request.contains("Forja tracks long-term project state"));

    let _ = tokio::fs::remove_dir_all(&base_dir).await;
}

#[tokio::test]
async fn knowledge_detection_failure_does_not_block_main_response() {
    let base_dir = unique_temp_dir("phase13i_engine_fallback");
    let knowledge = Arc::new(KnowledgeManager::new(base_dir.clone()));
    let provider = Arc::new(ScriptedProvider::new(vec![
        ProviderStep::Error("knowledge detection failed".to_string()),
        ProviderStep::Text("assistant fallback reply".to_string()),
    ]));
    let channel = Arc::new(QueueChannel::new(vec![Message::text(
        Role::User,
        "Answer even if knowledge detection breaks",
        None,
    )]));
    let mut engine = Engine::new(provider, channel.clone())
        .with_system_prompt("base system prompt".to_string())
        .with_knowledge(knowledge);
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

    let _ = tokio::fs::remove_dir_all(&base_dir).await;
}
