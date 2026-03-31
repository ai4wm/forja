use super::agents::default_debate_agents;
use super::types::DebatePhase;
use super::{DebateConfig, DebateEngine};
use crate::error::{ForjaError, Result};
use crate::traits::LlmProvider;
use crate::types::{Content, Message, Role, ToolDefinition};
use async_trait::async_trait;
use std::pin::Pin;
use std::sync::Arc;
use tokio_stream::Stream;

#[test]
fn test_default_config_creates_five_agents() {
    assert_eq!(default_debate_agents().len(), 5);
}

#[tokio::test]
async fn test_debate_phase_counts_and_result_shape() {
    let provider: Arc<dyn LlmProvider> = Arc::new(MockDebateProvider);
    let engine = DebateEngine::new(default_debate_agents(), DebateConfig::default());

    let result = engine
        .run_debate("Should we add Discord integration?", &provider, None)
        .await
        .expect("debate should succeed");

    let diverge_count = result
        .transcript
        .iter()
        .filter(|message| message.phase == DebatePhase::Diverge)
        .count();
    let conflict_count = result
        .transcript
        .iter()
        .filter(|message| message.phase == DebatePhase::Conflict)
        .count();
    let converge_count = result
        .transcript
        .iter()
        .filter(|message| message.phase == DebatePhase::Converge)
        .count();

    assert_eq!(diverge_count, 10);
    assert_eq!(conflict_count, 15);
    assert_eq!(converge_count, 1);
    assert_eq!(result.total_rounds, 6);
    assert!(!result.summary.trim().is_empty());
    assert!(!result.task_list.is_empty());
    assert!(result
        .transcript
        .iter()
        .all(|message| matches!(message.phase, DebatePhase::Diverge | DebatePhase::Conflict | DebatePhase::Converge)));
}

struct MockDebateProvider;

#[async_trait]
impl LlmProvider for MockDebateProvider {
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
        } else if prompt_text.contains("Phase: CONVERGE") {
            [
                "We should stage Discord integration behind a clear adapter boundary.",
                "The lowest-risk path is to validate gateway and permission handling first.",
                "The work should land as a small sequence of prioritized implementation tasks.",
                "- Add Discord gateway adapter | Architecture | 2 | 1",
                "- Implement Discord channel auth flow | Build | 4 | 2",
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
