use super::Engine;
use crate::emotion::EmotionEngine;
use crate::types::{Message, Role};
use chrono::Local;

impl Engine {
    pub fn with_emotion(mut self, emotion: EmotionEngine) -> Self {
        self.emotion = Some(emotion);
        self
    }

    pub(super) async fn refresh_turn_emotion_context(&mut self) {
        self.turn_tone_context = None;
        self.turn_relationship_context = None;

        let Some(emotion) = &self.emotion else {
            return;
        };

        let recent_messages = self.recent_emotion_messages();

        #[cfg(feature = "memory")]
        let memory_contents = self.load_memory_contents_or_empty().await;

        #[cfg(not(feature = "memory"))]
        let memory_contents = String::new();

        let signals = emotion.detect_signals(&recent_messages, &memory_contents, Local::now());
        if !signals.is_empty() {
            self.turn_tone_context = Some(signals.join("\n"));
        }
    }

    pub(super) fn clear_turn_emotion_context(&mut self) {
        self.turn_tone_context = None;
        self.turn_relationship_context = None;
    }

    fn recent_emotion_messages(&self) -> Vec<Message> {
        self.conversation_history
            .iter()
            .filter(|message| matches!(message.role, Role::User | Role::Assistant))
            .cloned()
            .collect()
    }
}
