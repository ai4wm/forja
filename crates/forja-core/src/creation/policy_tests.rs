use super::agents::default_debate_agents;
use super::{CreationRunContext, DebateConfig, DebateEngine, DebatePhase};
use crate::audit::logger::AuditLogger;
use crate::error::{ForjaError, Result};
use crate::traits::LlmProvider;
use crate::types::{Content, Message, Role, ToolDefinition};
use async_trait::async_trait;
use std::pin::Pin;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::Mutex;
use tokio_stream::Stream;

fn temp_db_path(label: &str) -> std::path::PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    std::env::temp_dir().join(format!("forja_creation_policy_{label}_{nanos}.db"))
}

#[tokio::test]
async fn debate_scales_team_size_for_simple_and_complex_topics() {
    let provider: Arc<dyn LlmProvider> = Arc::new(PolicyProvider::default());
    let engine = DebateEngine::new(default_debate_agents(), DebateConfig::default());
    let context = CreationRunContext {
        inter_call_delay: Duration::ZERO,
        ..CreationRunContext::default()
    };

    let simple = engine
        .run_debate_with_context("small cleanup", &provider, None, Some(context.clone()))
        .await
        .unwrap();
    let complex = engine
        .run_debate_with_context(
            "Should we redesign the runtime architecture, security, retry, and budget strategy?",
            &provider,
            None,
            Some(context),
        )
        .await
        .unwrap();

    assert_eq!(simple.active_agent_count, 3);
    assert_eq!(complex.active_agent_count, 5);
}

#[tokio::test]
async fn debate_honors_custom_stage_counts() {
    let provider: Arc<dyn LlmProvider> = Arc::new(PolicyProvider::default());
    let engine = DebateEngine::new(
        default_debate_agents().into_iter().take(1).collect(),
        DebateConfig {
            diverge_rounds: 1,
            conflict_rounds: 1,
            combination_rounds: 2,
            mutation_rounds: 2,
            converge_rounds: 1,
            min_agents: 1,
            max_agents: 1,
            auto_team_sizing: false,
        },
    );
    let context = CreationRunContext {
        inter_call_delay: Duration::ZERO,
        ..CreationRunContext::default()
    };

    let result = engine
        .run_debate_with_context("custom round check", &provider, None, Some(context))
        .await
        .unwrap();

    assert_eq!(result.total_rounds, 7);
    assert_eq!(result.metadata.combination_rounds, 2);
    assert_eq!(result.metadata.mutation_rounds, 2);
    assert_eq!(
        result
            .transcript
            .iter()
            .filter(|message| message.phase == DebatePhase::Combination)
            .count(),
        2
    );
    assert_eq!(
        result
            .transcript
            .iter()
            .filter(|message| message.phase == DebatePhase::Mutation)
            .count(),
        2
    );
}

#[tokio::test]
async fn debate_bounds_reused_prompt_context() {
    let provider_impl = Arc::new(BoundedContextProvider::default());
    let provider: Arc<dyn LlmProvider> = provider_impl.clone();
    let engine = DebateEngine::new(
        default_debate_agents().into_iter().take(1).collect(),
        DebateConfig {
            diverge_rounds: 1,
            conflict_rounds: 1,
            combination_rounds: 1,
            mutation_rounds: 1,
            converge_rounds: 1,
            min_agents: 1,
            max_agents: 1,
            auto_team_sizing: false,
        },
    );
    let context = CreationRunContext {
        inter_call_delay: Duration::ZERO,
        max_prompt_context_chars: 200,
        ..CreationRunContext::default()
    };

    engine
        .run_debate_with_context("bounded prompt reuse", &provider, None, Some(context))
        .await
        .unwrap();

    let captured = provider_impl.captured_prompts.lock().await.clone();
    let combination_prompt = captured
        .iter()
        .find(|prompt| prompt.contains("Phase: COMBINATION"))
        .unwrap();
    let mutation_prompt = captured
        .iter()
        .find(|prompt| prompt.contains("Phase: MUTATION"))
        .unwrap();

    assert!(combination_prompt.len() < 1500);
    assert!(mutation_prompt.len() < 1500);
}

#[tokio::test]
async fn debate_redacts_and_truncates_audit_payloads() {
    let provider: Arc<dyn LlmProvider> = Arc::new(SecretProvider);
    let db_path = temp_db_path("audit");
    let audit_logger = AuditLogger::new(&db_path).unwrap();
    let engine = DebateEngine::new(
        default_debate_agents().into_iter().take(1).collect(),
        DebateConfig {
            diverge_rounds: 1,
            conflict_rounds: 0,
            combination_rounds: 0,
            mutation_rounds: 0,
            converge_rounds: 0,
            min_agents: 1,
            max_agents: 1,
            auto_team_sizing: false,
        },
    );
    let context = CreationRunContext {
        inter_call_delay: Duration::ZERO,
        max_logged_chars: 128,
        ..CreationRunContext::default()
    };

    engine
        .run_debate_with_context("audit safety", &provider, Some(&audit_logger), Some(context))
        .await
        .unwrap();

    let events = audit_logger.query_recent(10).unwrap();
    let content = events
        .iter()
        .find(|event| event.event_type == "debate_message")
        .and_then(|event| event.payload["content"].as_str())
        .unwrap();

    assert!(content.contains("[REDACTED]"));
    assert!(!content.contains("sk-secret-token"));
    assert!(content.chars().count() <= 128);
    let _ = std::fs::remove_file(db_path);
}

#[tokio::test]
async fn debate_returns_bounded_redacted_errors() {
    let provider: Arc<dyn LlmProvider> = Arc::new(FailingProvider);
    let engine = DebateEngine::new(
        default_debate_agents().into_iter().take(1).collect(),
        DebateConfig {
            diverge_rounds: 1,
            conflict_rounds: 0,
            combination_rounds: 0,
            mutation_rounds: 0,
            converge_rounds: 0,
            min_agents: 1,
            max_agents: 1,
            auto_team_sizing: false,
        },
    );
    let context = CreationRunContext {
        inter_call_delay: Duration::ZERO,
        max_logged_chars: 128,
        ..CreationRunContext::default()
    };

    let error = engine
        .run_debate_with_context("bounded error surface", &provider, None, Some(context))
        .await
        .expect_err("creation failure should surface as an error");
    let error_text = error.to_string();

    assert!(error_text.contains("[REDACTED]"));
    assert!(!error_text.contains("sk-secret-token"));
    assert!(error_text.chars().count() <= 180);
}

#[derive(Default)]
struct PolicyProvider;

#[async_trait]
impl LlmProvider for PolicyProvider {
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
            "Yes, and... ".to_string() + &"alpha ".repeat(400)
        } else if prompt_text.contains("Phase: CONFLICT") {
            "Failure probability: 20%. Alternative: ".to_string() + &"beta ".repeat(400)
        } else if prompt_text.contains("Phase: COMBINATION") {
            "TRIZ blend.".to_string()
        } else if prompt_text.contains("Phase: MUTATION") {
            "Mutation result.".to_string()
        } else if prompt_text.contains("Phase: CONVERGE") {
            [
                "We should keep creation inside the runtime.",
                "Reuse existing policies before adding new services.",
                "The output should become executable tasks.",
                "- Add creation stage orchestration | Architecture | 2 | 1",
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

#[derive(Default)]
struct BoundedContextProvider {
    captured_prompts: Mutex<Vec<String>>,
}

#[async_trait]
impl LlmProvider for BoundedContextProvider {
    async fn chat(
        &self,
        messages: &[Message],
        _tools: Option<&[ToolDefinition]>,
    ) -> Result<Message> {
        let prompt_text = messages
            .iter()
            .filter_map(|message| match &message.content {
                Content::Text { text, .. } => Some(text.clone()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n");
        self.captured_prompts.lock().await.push(prompt_text.clone());

        let response = if prompt_text.contains("Phase: DIVERGE") {
            "Yes, and... ".to_string() + &"alpha ".repeat(500)
        } else if prompt_text.contains("Phase: CONFLICT") {
            "Failure probability: 20%. Alternative: ".to_string() + &"beta ".repeat(500)
        } else if prompt_text.contains("Phase: COMBINATION") {
            "TRIZ blend.".to_string()
        } else if prompt_text.contains("Phase: MUTATION") {
            "Mutation result.".to_string()
        } else {
            [
                "We should keep creation inside the runtime.",
                "Reuse existing policies before adding new services.",
                "The output should become executable tasks.",
                "- Add creation stage orchestration | Architecture | 2 | 1",
            ]
            .join("\n")
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

struct SecretProvider;

#[async_trait]
impl LlmProvider for SecretProvider {
    async fn chat(
        &self,
        _messages: &[Message],
        _tools: Option<&[ToolDefinition]>,
    ) -> Result<Message> {
        Ok(Message::text(
            Role::Assistant,
            format!("Bearer sk-secret-token {}", "alpha ".repeat(200)),
            None,
        ))
    }

    async fn stream(
        &self,
        _messages: &[Message],
        _tools: Option<&[ToolDefinition]>,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<String>> + Send>>> {
        Err(ForjaError::LlmError("stream not used".to_string()))
    }
}

struct FailingProvider;

#[async_trait]
impl LlmProvider for FailingProvider {
    async fn chat(
        &self,
        _messages: &[Message],
        _tools: Option<&[ToolDefinition]>,
    ) -> Result<Message> {
        Err(ForjaError::LlmError(format!(
            "Bearer sk-secret-token {}",
            "alpha ".repeat(400)
        )))
    }

    async fn stream(
        &self,
        _messages: &[Message],
        _tools: Option<&[ToolDefinition]>,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<String>> + Send>>> {
        Err(ForjaError::LlmError("stream not used".to_string()))
    }
}
