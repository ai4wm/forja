use super::agents::default_debate_agents;
use super::{CreationRunContext, DebateConfig, DebateEngine};
use crate::audit::logger::AuditLogger;
use crate::budget::{manager::BudgetManager, BudgetMode};
use crate::error::{ForjaError, Result};
use crate::traits::LlmProvider;
use crate::types::{Content, Message, Role, ToolDefinition};
use async_trait::async_trait;
use std::pin::Pin;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio_stream::Stream;

fn temp_db_path(label: &str) -> std::path::PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    std::env::temp_dir().join(format!("forja_creation_{label}_{nanos}.db"))
}

#[tokio::test]
async fn debate_includes_combination_and_mutation_phases() {
    let provider: Arc<dyn LlmProvider> = Arc::new(ExpandedDebateProvider::default());
    let engine = DebateEngine::new(default_debate_agents(), DebateConfig::default());
    let context = CreationRunContext {
        inter_call_delay: Duration::ZERO,
        ..CreationRunContext::default()
    };

    let result = engine
        .run_debate_with_context(
            "Should we redesign the runtime architecture and release plan?",
            &provider,
            None,
            Some(context),
        )
        .await
        .expect("expanded debate should succeed");

    assert!(result
        .transcript
        .iter()
        .any(|message| message.phase.label() == "Combination"));
    assert!(result
        .transcript
        .iter()
        .any(|message| message.phase.label() == "Mutation"));
    assert_eq!(result.active_agent_count, 5);
    assert_eq!(result.total_rounds, 8);
}

#[tokio::test]
async fn debate_retries_transient_provider_failures_via_ralf() {
    let provider_impl = Arc::new(FlakyDebateProvider::default());
    let provider: Arc<dyn LlmProvider> = provider_impl.clone();
    let engine = DebateEngine::new(default_debate_agents(), DebateConfig::default());
    let context = CreationRunContext {
        inter_call_delay: Duration::ZERO,
        ..CreationRunContext::default()
    };

    let result = engine
        .run_debate_with_context(
            "Should we keep the current release process?",
            &provider,
            None,
            Some(context),
        )
        .await
        .expect("ralf-backed debate should recover from a transient failure");

    assert!(!result.summary.trim().is_empty());
    assert!(provider_impl.calls.load(Ordering::SeqCst) >= 2);
}

#[tokio::test]
async fn debate_returns_bounded_fallback_task_when_no_valid_task_lines_exist() {
    let provider: Arc<dyn LlmProvider> = Arc::new(NoTaskLineProvider);
    let engine = DebateEngine::new(default_debate_agents(), DebateConfig::default());
    let context = CreationRunContext {
        inter_call_delay: Duration::ZERO,
        ..CreationRunContext::default()
    };

    let result = engine
        .run_debate_with_context(
            "Should we attempt a risky migration?",
            &provider,
            None,
            Some(context),
        )
        .await
        .expect("debate should still succeed");

    assert_eq!(result.task_list.len(), 1);
    assert!(result.task_list[0].name.to_lowercase().contains("review"));
}

#[tokio::test]
async fn debate_enforces_budget_for_scoped_creation_agents() {
    let provider: Arc<dyn LlmProvider> = Arc::new(ExpandedDebateProvider::default());
    let budget_path = temp_db_path("creation_budget");
    let budget_manager = Arc::new(BudgetManager::new(&budget_path).unwrap());
    budget_manager.register_agent("creation/architect", 1).unwrap();
    budget_manager.register_agent("creation/critic", 1).unwrap();
    budget_manager.register_agent("creation/builder", 1).unwrap();
    budget_manager.register_agent("creation/researcher", 1).unwrap();
    budget_manager.register_agent("creation/synthesizer", 1).unwrap();
    let audit_logger = AuditLogger::new(&budget_path).unwrap();
    let engine = DebateEngine::new(default_debate_agents(), DebateConfig::default());
    let context = CreationRunContext {
        budget_manager: Some(budget_manager),
        budget_mode: BudgetMode::Enforce,
        inter_call_delay: Duration::ZERO,
        ..CreationRunContext::default()
    };

    let error = engine
        .run_debate_with_context(
            "Should we expand the creation engine?",
            &provider,
            Some(&audit_logger),
            Some(context),
        )
        .await
        .expect_err("budget enforcement should block oversized scoped creation-agent usage");

    assert!(error.to_string().to_lowercase().contains("budget"));
    let _ = std::fs::remove_file(budget_path);
}

#[derive(Default)]
struct ExpandedDebateProvider;

#[async_trait]
impl LlmProvider for ExpandedDebateProvider {
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
            "Yes, and... split the work into smaller components.".to_string()
        } else if prompt_text.contains("Phase: CONFLICT") {
            "Failure probability: 20%. Alternative: stage the rollout.".to_string()
        } else if prompt_text.contains("Phase: COMBINATION") {
            "TRIZ blend: combine runtime and debate telemetry into one execution surface.".to_string()
        } else if prompt_text.contains("Phase: MUTATION") {
            "Mutation: invert the failure path and turn retries into explicit operator tasks.".to_string()
        } else if prompt_text.contains("Phase: CONVERGE") {
            [
                "We should keep the creation engine inside the main runtime.",
                "The safest path is to reuse the existing budget and retry policies.",
                "The work should land as a small sequence of executable tasks.",
                "- Add combination stage execution | Architecture | 4 | 1",
                "- Add mutation stage execution | Build | 4 | 1",
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
struct FlakyDebateProvider {
    calls: AtomicUsize,
}

#[async_trait]
impl LlmProvider for FlakyDebateProvider {
    async fn chat(
        &self,
        messages: &[Message],
        _tools: Option<&[ToolDefinition]>,
    ) -> Result<Message> {
        let call_index = self.calls.fetch_add(1, Ordering::SeqCst);
        if call_index == 0 {
            return Err(ForjaError::LlmError("transient provider failure".to_string()));
        }

        ExpandedDebateProvider.chat(messages, None).await
    }

    async fn stream(
        &self,
        _messages: &[Message],
        _tools: Option<&[ToolDefinition]>,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<String>> + Send>>> {
        Err(ForjaError::LlmError("stream not used".to_string()))
    }
}

struct NoTaskLineProvider;

#[async_trait]
impl LlmProvider for NoTaskLineProvider {
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

        let response = if prompt_text.contains("Phase: CONVERGE") {
            [
                "We should avoid shipping this blindly.",
                "The current proposal still needs operator review.",
                "Manual review is safer than guessing invalid tasks.",
            ]
            .join("\n")
        } else {
            ExpandedDebateProvider::default()
                .chat(messages, None)
                .await?
                .content_text()
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

trait MessageTextExt {
    fn content_text(self) -> String;
}

impl MessageTextExt for Message {
    fn content_text(self) -> String {
        match self.content {
            Content::Text { text, .. } => text,
            _ => String::new(),
        }
    }
}
