use async_trait::async_trait;
use chrono::{Duration, Local, TimeZone};
use forja_core::emotion::{
    default_startup_greeting, generate_startup_greeting, EmotionEngine, MoodState,
    RelationshipContext,
};
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

fn mood_state(mood: &str, intensity: u8, reason: &str, tone_instruction: &str) -> MoodState {
    MoodState {
        mood: mood.to_string(),
        intensity,
        reason: reason.to_string(),
        tone_instruction: tone_instruction.to_string(),
        updated_at: Local
            .with_ymd_and_hms(2026, 3, 26, 12, 0, 0)
            .single()
            .unwrap(),
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

#[tokio::test]
async fn emotion_analyze_parses_json_response() {
    let provider = ScriptedProvider::new(vec![ProviderStep::Text(
        r#"{"mood":"focused","intensity":4,"reason":"집중 흐름 유지","tone_instruction":"짧고 또렷하게 도와주세요"}"#.to_string(),
    )]);
    let mut emotion = EmotionEngine::new(MoodState::neutral());

    let mood = emotion
        .analyze(
            &[Message::text(Role::User, "Phase 14까지 밀고 가자", None)],
            &provider,
        )
        .await
        .unwrap();

    assert_eq!(mood.mood, "focused");
    assert_eq!(mood.intensity, 4);
    assert_eq!(mood.reason, "집중 흐름 유지");
    assert_eq!(mood.tone_instruction, "짧고 또렷하게 도와주세요");
}

#[tokio::test]
async fn emotion_analyze_keeps_previous_state_on_invalid_json() {
    let previous = mood_state("concerned", 3, "어려운 구간 지속", "차분하게 안정감을 주세요");
    let provider = ScriptedProvider::new(vec![ProviderStep::Text("not-json".to_string())]);
    let mut emotion = EmotionEngine::new(previous.clone());

    let mood = emotion
        .analyze(
            &[Message::text(Role::User, "왜 계속 안되지", None)],
            &provider,
        )
        .await
        .unwrap();

    assert_eq!(mood, previous);
}

#[tokio::test]
async fn emotion_analyze_keeps_previous_state_on_provider_failure() {
    let previous = mood_state("happy", 2, "안정적인 대화", "밝지만 차분하게 답하세요");
    let provider = ScriptedProvider::new(vec![ProviderStep::Error("network fail".to_string())]);
    let mut emotion = EmotionEngine::new(previous.clone());

    let mood = emotion
        .analyze(
            &[Message::text(Role::User, "도와줘", None)],
            &provider,
        )
        .await
        .unwrap();

    assert_eq!(mood, previous);
}

#[test]
fn mood_tags_round_trip_through_memory_lines() {
    let mood = mood_state("excited", 5, "성과가 연속으로 누적", "함께 기세를 살려주세요");
    let tag = mood.to_memory_tag();
    let line = format!("12:00 | system | {tag}");
    let restored = EmotionEngine::restore_from_memory(&format!("--- 2026-03-26 ---\n{line}\n"))
        .unwrap();

    assert_eq!(tag, "[mood:excited:5:성과가 연속으로 누적]");
    assert_eq!(restored.mood, "excited");
    assert_eq!(restored.intensity, 5);
    assert_eq!(restored.reason, "성과가 연속으로 누적");
}

#[test]
fn relationship_detects_late_night_work() {
    let today = Local::now().date_naive();
    let memory = format!(
        "{}\n01:20 | assistant | 아직도 작업 중이네요\n02:10 | user | 네, 조금만 더 할게요",
        build_memory_line(today, "00:40", "user", "새벽에도 계속 작업합니다")
    );

    let patterns = RelationshipContext::detect_patterns(&memory);

    assert!(patterns.iter().any(|pattern| pattern.contains("건강 챙기세요")));
}

#[test]
fn relationship_detects_long_gap() {
    let old_day = Local::now().date_naive() - Duration::days(4);
    let memory = build_memory_line(old_day, "12:00", "user", "오랜만의 기록입니다");

    let patterns = RelationshipContext::detect_patterns(&memory);

    assert!(patterns.iter().any(|pattern| pattern.contains("오랜만이십니다")));
}

#[test]
fn relationship_detects_error_streak() {
    let today = Local::now().date_naive();
    let memory = format!(
        "{}\n10:05 | assistant | 또 에러가 났어요\n10:10 | user | 안돼, 계속 실패해",
        build_memory_line(today, "10:00", "user", "에러가 계속 납니다")
    );

    let patterns = RelationshipContext::detect_patterns(&memory);

    assert!(patterns.iter().any(|pattern| pattern.contains("잠깐 쉬었다")));
}

#[test]
fn relationship_detects_progress_streak() {
    let today = Local::now().date_naive();
    let memory = format!(
        "{}\n09:20 | assistant | commit까지 끝났네요",
        build_memory_line(today, "09:00", "user", "Phase 14 완료 직전입니다")
    );

    let patterns = RelationshipContext::detect_patterns(&memory);

    assert!(patterns.iter().any(|pattern| pattern.contains("진도가 정말 빠르십니다")));
}

#[tokio::test]
async fn tone_instruction_is_injected_into_system_prompt() {
    let provider = Arc::new(ScriptedProvider::new(vec![
        ProviderStep::Text(
            r#"{"mood":"focused","intensity":4,"reason":"집중","tone_instruction":"짧고 또렷하게 안내하세요"}"#.to_string(),
        ),
        ProviderStep::Text("main response".to_string()),
    ]));
    let memory_store = Arc::new(RecordingMemoryStore::new(
        "--- 2026-03-26 ---\n10:00 | user | 최근 commit이 계속 이어졌습니다",
    ));
    let channel = Arc::new(QueueChannel::new(vec![Message::text(
        Role::User,
        "오늘도 이어서 진행하자",
        None,
    )]));
    let mut engine = Engine::new(provider.clone(), channel.clone())
        .with_system_prompt("base system prompt".to_string())
        .with_memory(memory_store)
        .with_emotion(EmotionEngine::new(MoodState::neutral()));
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

    assert!(main_request.contains("System:You are Forja, a personal AI assistant."));
    assert!(main_request.contains("base system prompt"));
    assert!(main_request.contains("[tone]"));
    assert!(main_request.contains("짧고 또렷하게 안내하세요"));
    assert!(main_request.contains("[memory.md - Persistent Memory]"));
}

#[tokio::test]
async fn emotion_failures_do_not_block_main_response() {
    let provider = Arc::new(ScriptedProvider::new(vec![
        ProviderStep::Error("emotion analysis failed".to_string()),
        ProviderStep::Text("assistant fallback reply".to_string()),
    ]));
    let memory_store = Arc::new(RecordingMemoryStore::new(""));
    let channel = Arc::new(QueueChannel::new(vec![Message::text(
        Role::User,
        "응답은 계속 와야 해",
        None,
    )]));
    let mut engine = Engine::new(provider, channel.clone())
        .with_system_prompt("base system prompt".to_string())
        .with_memory(memory_store)
        .with_emotion(EmotionEngine::new(MoodState::neutral()));
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

#[tokio::test]
async fn mood_changes_are_saved_as_system_memory_tags() {
    let provider = Arc::new(ScriptedProvider::new(vec![
        ProviderStep::Text(
            r#"{"mood":"happy","intensity":3,"reason":"밝은 성과 연속","tone_instruction":"기분 좋게 맞춰주세요"}"#.to_string(),
        ),
        ProviderStep::Text("assistant reply".to_string()),
    ]));
    let memory_store = Arc::new(RecordingMemoryStore::new(""));
    let channel = Arc::new(QueueChannel::new(vec![Message::text(
        Role::User,
        "이번 단계도 끝내자",
        None,
    )]));
    let mut engine = Engine::new(provider, channel.clone())
        .with_system_prompt("base system prompt".to_string())
        .with_memory(memory_store.clone())
        .with_emotion(EmotionEngine::new(MoodState::neutral()));
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

    let saved_entries = memory_store.saved_entries().await;

    assert!(saved_entries
        .iter()
        .any(|entry| entry.tags.iter().any(|tag| tag == "system")
            && entry.content.contains("[mood:happy:3:밝은 성과 연속]")));
}

#[tokio::test]
async fn startup_greeting_uses_memory_context_when_available() {
    let provider = ScriptedProvider::new(vec![ProviderStep::Text(
        "주인님, 오늘도 늦게까지 하고 계셨네요.".to_string(),
    )]);
    let memory = format!(
        "{}\n01:10 | assistant | 잠깐 쉬고 하셔도 됩니다",
        build_memory_line(Local::now().date_naive(), "00:40", "user", "새벽 작업 기록")
    );

    let greeting = generate_startup_greeting(&provider, "황비서", "주인님", &memory, false)
        .await
        .unwrap();

    assert_eq!(greeting, Some("주인님, 오늘도 늦게까지 하고 계셨네요.".to_string()));
}

#[tokio::test]
async fn startup_greeting_falls_back_to_default_when_memory_is_empty() {
    let provider = ScriptedProvider::new(vec![]);
    let default_greeting = default_startup_greeting("주인님");

    let greeting = generate_startup_greeting(&provider, "황비서", "주인님", "", false)
        .await
        .unwrap();

    assert_eq!(greeting, Some(default_greeting));
}

#[tokio::test]
async fn startup_greeting_falls_back_to_default_on_provider_failure() {
    let provider = ScriptedProvider::new(vec![ProviderStep::Error("greeting failed".to_string())]);
    let default_greeting = default_startup_greeting("주인님");
    let memory = build_memory_line(Local::now().date_naive(), "09:00", "user", "최근 기록");

    let greeting = generate_startup_greeting(&provider, "황비서", "주인님", &memory, false)
        .await
        .unwrap();

    assert_eq!(greeting, Some(default_greeting));
}

#[tokio::test]
async fn startup_greeting_is_skipped_on_first_run() {
    let provider = ScriptedProvider::new(vec![ProviderStep::Text("ignored".to_string())]);
    let memory = build_memory_line(Local::now().date_naive(), "09:00", "user", "최근 기록");

    let greeting = generate_startup_greeting(&provider, "황비서", "주인님", &memory, true)
        .await
        .unwrap();

    assert_eq!(greeting, None);
}
