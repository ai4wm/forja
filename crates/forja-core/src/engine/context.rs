use super::Engine;
use crate::context::compressor::{
    compress_history_for_total,
    emergency_compress_history,
    CompressionOutcome,
    DEFAULT_MAX_CONTEXT_TOKENS,
};
use crate::context::token_counter::count_messages_tokens;
use crate::context::SummaryCallback;
use crate::error::Result;

impl Engine {
    pub fn with_context_settings(
        mut self,
        max_context_tokens: usize,
        model: String,
    ) -> Self {
        self.max_context_tokens = max_context_tokens;
        self.context_model = model;
        self
    }

    pub fn with_context_summary_callback(
        mut self,
        summary_callback: SummaryCallback,
    ) -> Self {
        self.context_summary_callback = Some(summary_callback);
        self
    }

    pub(super) async fn compress_context(&mut self) -> Result<CompressionOutcome> {
        let total_request_tokens = self.request_total_tokens();
        let warning_threshold = self.max_context_tokens * 80 / 100;

        if total_request_tokens >= warning_threshold && !self.context_warning_emitted {
            eprintln!("[Context] Token usage at 80%, compression soon");
            self.context_warning_emitted = true;
        }

        let outcome = compress_history_for_total(
            &mut self.conversation_history,
            total_request_tokens,
            &self.context_model,
            self.max_context_tokens,
            self.context_summary_callback.as_ref(),
        )
        .await?;

        self.recalculate_total_tokens();

        if self.request_total_tokens() < warning_threshold {
            self.context_warning_emitted = false;
        }

        Ok(outcome)
    }

    pub(super) async fn emergency_compress_context(&mut self) -> Result<()> {
        eprintln!("[Context] Emergency compression triggered");
        emergency_compress_history(
            &mut self.conversation_history,
            &self.context_model,
            self.context_summary_callback.as_ref(),
        )
        .await?;
        self.recalculate_total_tokens();
        self.context_warning_emitted = false;
        Ok(())
    }

    pub(super) fn clear_conversation_history(&mut self) {
        self.conversation_history.clear();
        self.total_tokens = 0;
        self.context_warning_emitted = false;
    }

    pub(super) fn recalculate_total_tokens(&mut self) {
        self.total_tokens = count_messages_tokens(&self.conversation_history, &self.context_model);
    }

    fn request_total_tokens(&self) -> usize {
        count_messages_tokens(&self.request_messages(), &self.context_model)
    }
}

impl Default for EngineContextDefaults {
    fn default() -> Self {
        Self {
            max_context_tokens: DEFAULT_MAX_CONTEXT_TOKENS,
            context_model: "cl100k_base".to_string(),
        }
    }
}

pub(super) struct EngineContextDefaults {
    pub max_context_tokens: usize,
    pub context_model: String,
}
