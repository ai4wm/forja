use super::{Engine, SlashHandler, TuiHandler, ANSI_RESET};
use crate::autonomy::AutonomyExecutionRuntime;
use crate::context::token_counter::count_message_tokens;
use crate::error::Result;
use crate::skill::SkillRegistry;
use crate::traits::{Channel, LlmProvider, Tool};
#[cfg(feature = "memory")]
use crate::traits::MemoryStore;
use std::sync::Arc;

use super::context::EngineContextDefaults;

impl Engine {
    pub fn new(provider: Arc<dyn LlmProvider>, channel: Arc<dyn Channel>) -> Self {
        let context_defaults = EngineContextDefaults::default();
        let (heartbeat_sender, heartbeat_receiver) = tokio::sync::mpsc::channel(32);

        Self {
            provider,
            channel,
            tools: std::collections::HashMap::new(),
            conversation_history: Vec::new(),
            total_tokens: 0,
            max_context_tokens: context_defaults.max_context_tokens,
            context_model: context_defaults.context_model,
            context_warning_emitted: false,
            context_summary_callback: None,
            budget_manager: None,
            budget_mode: crate::budget::BudgetMode::Monitor,
            current_agent_id: "default".to_string(),
            creation_engine: None,
            autonomy: None,
            autonomy_runtime: None,
            audit_logger: None,
            heartbeat_scheduler: None,
            heartbeat_sender,
            heartbeat_receiver: Some(heartbeat_receiver),
            ralf_config: crate::ralf::RalfConfig::default(),
            system_prompt: None,
            tool_prompt: None,
            assistant_name: String::new(),
            user_title: String::new(),
            slash_handler: None,
            dashboard_handler: None,
            tui_handler: None,
            skill_registry: None,
            mode_state: crate::mode::ModeState::default(),
            emotion: None,
            turn_tone_context: None,
            turn_relationship_context: None,
            knowledge: None,
            turn_knowledge_context: None,
            serendipity: None,
            turn_count: 0,
            last_serendipity_triggered_at: None,
            #[cfg(feature = "memory")]
            memory: None,
            #[cfg(feature = "memory")]
            turn_memory_context: None,
            #[cfg(feature = "memory")]
            dream_runtime: None,
            #[cfg(feature = "memory")]
            dream_state: None,
        }
    }

    pub fn with_system_prompt(mut self, prompt: String) -> Self {
        self.system_prompt = Some(prompt);
        self
    }

    #[cfg(feature = "memory")]
    pub fn with_memory(mut self, memory: Arc<dyn MemoryStore>) -> Self {
        self.memory = Some(memory);
        self
    }

    pub fn with_slash_handler(mut self, handler: SlashHandler) -> Self {
        self.slash_handler = Some(handler);
        self
    }

    pub fn with_tui_handler(mut self, handler: TuiHandler) -> Self {
        self.tui_handler = Some(handler);
        self
    }

    pub fn with_autonomy_runtime(mut self, autonomy_runtime: AutonomyExecutionRuntime) -> Self {
        self.autonomy_runtime = Some(autonomy_runtime);
        self
    }

    pub fn with_skill_registry(mut self, skill_registry: Arc<SkillRegistry>) -> Self {
        self.skill_registry = Some(skill_registry);
        self
    }

    pub fn register_tool(&mut self, tool: Arc<dyn Tool>) {
        self.tools.insert(tool.name().to_string(), tool);
    }

    pub fn swap_provider(&mut self, new_provider: Arc<dyn LlmProvider>) {
        self.provider = new_provider;
    }

    pub fn slash_response(&self, text: &str) -> Option<&'static str> {
        if text.trim_start().starts_with('/') {
            Some("")
        } else {
            None
        }
    }

    pub(crate) async fn log_cli_stage(&self, color: &str, text: &str) {
        if self.channel.is_cli_source() {
            self.channel
                .log_line(&format!("{color}\u{2022} {text}{ANSI_RESET}"))
                .await;
        }
    }

    #[cfg(feature = "runtime")]
    pub(crate) fn apply_system_prompt_update(
        &mut self,
        next_system_prompt: Option<String>,
        reset_history: bool,
    ) {
        self.system_prompt = next_system_prompt.clone();

        if reset_history {
            self.clear_conversation_history();
        }
        if let Some(system_prompt) = next_system_prompt {
            self.system_prompt = Some(system_prompt);
        }
    }

    pub fn shutdown(&mut self) {
        self.stop_heartbeat_runtime();
        self.autonomy = None;
        self.clear_conversation_history();
        self.clear_turn_knowledge_context();
        self.clear_turn_emotion_context();
        #[cfg(feature = "memory")]
        self.clear_turn_memory_context();
        #[cfg(feature = "memory")]
        self.reset_dream_state();
        self.turn_count = 0;
        self.last_serendipity_triggered_at = None;

        let (heartbeat_sender, heartbeat_receiver) = tokio::sync::mpsc::channel(32);
        self.heartbeat_sender = heartbeat_sender;
        self.heartbeat_receiver = Some(heartbeat_receiver);
    }

    pub(crate) fn build_text_response(&self, text: &str) -> crate::types::Message {
        crate::types::Message::text(crate::types::Role::Assistant, text, None)
    }

    pub(crate) fn track_response_usage(&mut self, response: &crate::types::Message) -> Result<()> {
        self.record_current_agent_usage(count_message_tokens(response, &self.context_model))
    }
}
