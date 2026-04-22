use super::Engine;
use crate::emotion::EmotionEngine;
use crate::emotion::MoodState;
use crate::emotion::RelationshipContext;
use crate::types::{Message, Role};

impl Engine {
    pub fn with_emotion(mut self, emotion: EmotionEngine) -> Self {
        self.emotion = Some(emotion);
        self
    }

    pub(super) async fn refresh_turn_emotion_context(&mut self) {
        self.turn_tone_context = None;
        self.turn_relationship_context = None;
        let recent_messages = self.recent_emotion_messages();
        let provider = self.provider.clone();

        let Some(emotion) = &mut self.emotion else {
            return;
        };

        let previous = emotion.current.clone();
        let analyzed = match emotion.analyze(&recent_messages, provider.as_ref()).await {
            Ok(mood) => mood,
            Err(error) => {
                eprintln!("[Emotion] analyze failed: {error}");
                previous.clone()
            }
        };

        #[cfg(feature = "memory")]
        {
            let memory_contents = self.load_memory_contents_or_empty().await;
            self.turn_relationship_context = RelationshipContext::build_context(&memory_contents);
        }

        self.turn_tone_context = Some(analyzed.tone_section());

        if analyzed.has_changed_from(&previous) {
            self.persist_mood_change(&analyzed).await;
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
    async fn persist_mood_change(&self, mood: &MoodState) {
        #[cfg(feature = "memory")]
        {
            use crate::types::MemoryEntry;
            use std::time::{SystemTime, UNIX_EPOCH};
            use uuid::Uuid;

            let Some(memory) = &self.memory else {
                return;
            };

            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            let entry = MemoryEntry {
                id: format!("system_mood_{}_{}", now, Uuid::new_v4()),
                timestamp: now,
                tags: vec!["system".to_string()],
                content: mood.to_memory_tag(),
                score: 0.0,
                metadata: Default::default(),
            };

            if let Err(error) = memory.save(&entry).await {
                eprintln!("[Emotion] failed to save mood tag: {error}");
            }
        }

        #[cfg(not(feature = "memory"))]
        let _ = mood;
    }
}
