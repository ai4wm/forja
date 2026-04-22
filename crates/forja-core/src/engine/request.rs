use super::Engine;
use crate::prompt::assemble_system_prompt;
use crate::types::{Message, Role};

impl Engine {
    pub(crate) fn push_message(&mut self, message: Message) {
        self.total_tokens =
            self.total_tokens
                .saturating_add(crate::context::token_counter::count_message_tokens(
                    &message,
                    &self.context_model,
                ));
        self.conversation_history.push(message);
    }

    pub(crate) fn request_messages(&self) -> Vec<Message> {
        let mut messages = Vec::new();
        let prompt = assemble_system_prompt(
            &self.mode_state,
            &self.assistant_name,
            &self.user_title,
            self.system_prompt.as_deref().unwrap_or_default(),
            "",
            self.tool_prompt.as_deref().unwrap_or_default(),
            self.turn_tone_context.as_deref().unwrap_or_default(),
            self.turn_relationship_context
                .as_deref()
                .unwrap_or_default(),
            self.turn_knowledge_context.as_deref().unwrap_or_default(),
            #[cfg(feature = "memory")]
            self.turn_memory_context.as_deref().unwrap_or_default(),
            #[cfg(not(feature = "memory"))]
            "",
        );
        if !prompt.trim().is_empty() {
            messages.push(Message::text(Role::System, prompt, None));
        }

        messages.extend(self.conversation_history.clone());
        messages
    }
}
