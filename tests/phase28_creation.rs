use async_trait::async_trait;
use forja_core::creation::{DebateConfig, DebateEngine, agents::default_debate_agents};
use forja_core::engine::SlashCommandResult;
use forja_core::error::{ForjaError, Result};
use forja_core::traits::{Channel, LlmProvider};
use forja_core::{Content, Engine, Message, Role, ToolDefinition};
use std::future::pending;
use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::{Mutex, oneshot};
use tokio_stream::Stream;

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

    async fn sent_texts(&self) -> Vec<String> {
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

struct DebateProvider;

#[async_trait]
impl LlmProvider for DebateProvider {
    async fn chat(
        &self,
        messages: &[Message],
        _tools: Option<&[ToolDefinition]>,
    ) -> Result<Message> {
        let prompt_text = messages
            .iter()
            .filter_map(|message| match &message.content {
                Content::Text { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n");

        let response = if prompt_text.contains("Phase: DIVERGE") {
            "Yes, and... build a clearer creation pipeline.".to_string()
        } else if prompt_text.contains("Phase: CONFLICT") {
            "Failure probability: 25%. Alternative: stage the rollout.".to_string()
        } else if prompt_text.contains("Phase: COMBINATION") {
            "TRIZ blend: fuse debate and execution policy into one runtime path.".to_string()
        } else if prompt_text.contains("Phase: MUTATION") {
            "Mutation: invert the failure path and convert it into bounded operator tasks."
                .to_string()
        } else if prompt_text.contains("Phase: CONVERGE") {
            [
                "We should keep creation inside the main runtime.",
                "The safest path is to reuse existing retry and budget policies.",
                "The result should be converted into executable tasks.",
                "- Add combination stage execution | Architecture | 2 | 1",
                "- Add mutation stage execution | Build | 2 | 1",
            ]
            .join("\n")
        } else {
            return Err(ForjaError::LlmError("unknown debate phase".to_string()));
        };

        Ok(Message::text(Role::Assistant, response, None))
    }

    async fn stream(
        &self,
        _messages: &[Message],
        _tools: Option<&[ToolDefinition]>,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<String>> + Send>>> {
        Err(ForjaError::LlmError("stream not used".to_string()))
    }
}

#[tokio::test]
async fn runtime_debate_emits_combination_mutation_and_final_tasks() {
    let provider: Arc<dyn LlmProvider> = Arc::new(DebateProvider);
    let channel = Arc::new(QueueChannel::new(vec![Message::text(
        Role::User,
        "/debate expand the creation engine",
        None,
    )]));
    let debate_config = DebateConfig {
        diverge_rounds: 1,
        conflict_rounds: 1,
        combination_rounds: 1,
        mutation_rounds: 1,
        converge_rounds: 1,
        min_agents: 1,
        max_agents: 1,
        auto_team_sizing: false,
    };
    let agents = default_debate_agents().into_iter().take(1).collect();
    let mut engine = Engine::new(provider, channel.clone())
        .with_creation_engine(DebateEngine::new(agents, debate_config))
        .with_slash_handler(Arc::new(|text: &str, _, _| {
            text.trim()
                .strip_prefix("/debate ")
                .map(|topic| SlashCommandResult::Debate {
                    topic: topic.to_string(),
                })
        }));
    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
    let run = engine.run_streaming(async {
        let _ = shutdown_rx.await;
    });
    tokio::pin!(run);
    let monitor = async {
        for _ in 0..120 {
            let sent = channel.sent_texts().await;
            if sent.iter().any(|text| text.contains("[Debate Result]")) {
                break;
            }
            tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
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

    let sent = channel.sent_texts().await;
    assert!(sent.iter().any(|text| text.contains("[Combination][R1]")));
    assert!(sent.iter().any(|text| text.contains("[Mutation][R1]")));
    assert!(sent.iter().any(|text| text.contains("[Debate Result]")));
    assert!(
        sent.iter()
            .any(|text| text.contains("Add combination stage execution"))
    );
}
