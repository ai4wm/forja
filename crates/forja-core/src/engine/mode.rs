use super::Engine;
use crate::mode::{detect_role, ModeState};
use crate::types::{Content, Message};

impl Engine {
    pub fn with_mode(mut self, mode_state: ModeState) -> Self {
        self.mode_state = mode_state;
        self
    }

    pub fn with_tool_prompt(mut self, prompt: String) -> Self {
        self.tool_prompt = Some(prompt);
        self
    }

    pub(super) fn refresh_turn_role(&mut self, user_msg: &Message) {
        if self.mode_state.role != crate::mode::Role::Auto {
            self.mode_state.update_detected_role(self.mode_state.role);
            return;
        }

        let Content::Text { text, .. } = &user_msg.content else {
            self.mode_state.update_detected_role(crate::mode::Role::Default);
            return;
        };

        self.mode_state.update_detected_role(detect_role(text));
    }
}
