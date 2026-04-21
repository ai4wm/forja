use super::Engine;
use crate::creation::{CreationRunContext, DebateEngine, DebateResult};
use crate::error::{ForjaError, Result};
use crate::types::{Message, Role};

impl Engine {
    pub fn with_creation_engine(mut self, creation_engine: DebateEngine) -> Self {
        self.creation_engine = Some(creation_engine);
        self
    }

    pub(crate) async fn run_debate_command(&mut self, topic: &str) -> Result<DebateResult> {
        let Some(creation_engine) = &self.creation_engine else {
            return Err(ForjaError::Internal(
                "creation engine is not configured".to_string(),
            ));
        };

        let provider = self.provider.clone();
        let audit_logger = self.audit_logger.clone();
        let channel = self.channel.clone();
        let mut callback = move |message: &crate::creation::DebateMessage| -> std::pin::Pin<
            Box<dyn std::future::Future<Output = Result<()>> + Send>,
        > {
            let channel = channel.clone();
            let text = format!(
                "[{}][R{}] {}: {}",
                message.phase.label(),
                message.round,
                message.role,
                message.content
            );
            Box::pin(async move {
                channel
                    .send(Message::text(Role::Assistant, text, None))
                    .await
            })
        };

        let run_context = CreationRunContext {
            ralf_config: self.ralf_config.clone(),
            budget_manager: self.budget_manager.clone(),
            budget_mode: self.budget_mode,
            ..CreationRunContext::default()
        };
        let result = creation_engine
            .run_debate_with_callback(
                topic,
                &provider,
                audit_logger.as_deref(),
                Some(&mut callback),
                Some(run_context),
            )
            .await?;
        self.record_current_agent_usage(result.total_tokens)?;
        Ok(result)
    }
}
