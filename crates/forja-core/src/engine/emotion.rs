use super::Engine;
use crate::emotion::{MoodState, RelationshipContext};
use crate::types::{MemoryEntry, Message, Role};
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

impl Engine {
    pub(super) async fn refresh_turn_emotion_context(&mut self) {
        self.turn_tone_context = None;
        self.turn_relationship_context = None;
        let recent_messages = self.recent_emotion_messages();
        let provider = self.provider.clone();

        let Some(emotion) = &mut self.emotion else {
            return;
        };

        let previous = emotion.current.clone();
        let analyzed = match emotion
            .analyze(&recent_messages, provider.as_ref())
            .await
        {
            Ok(mood) => mood,
            Err(error) => {
                eprintln!("[Emotion] analyze failed: {error}");
                previous.clone()
            }
        };

        self.turn_tone_context = Some(format!("[tone]\n{}", analyzed.tone_instruction));

        let patterns = RelationshipContext::detect_patterns(&self.load_emotion_memory_contents().await);
        if !patterns.is_empty() {
            self.turn_relationship_context = Some(format!(
                "[relationship]\n{}",
                patterns.join("\n")
            ));
        }

        if mood_has_changed(&previous, &analyzed) {
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

    #[cfg(feature = "memory")]
    async fn load_emotion_memory_contents(&self) -> String {
        let Some(memory) = &self.memory else {
            return String::new();
        };

        match memory.load_all().await {
            Ok(contents) => contents,
            Err(error) => {
                eprintln!("[Emotion] load_all failed: {error}");
                String::new()
            }
        }
    }

    #[cfg(not(feature = "memory"))]
    async fn load_emotion_memory_contents(&self) -> String {
        String::new()
    }

    async fn persist_mood_change(&self, mood: &MoodState) {
        #[cfg(feature = "memory")]
        {
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

fn mood_has_changed(previous: &MoodState, next: &MoodState) -> bool {
    previous.mood != next.mood
        || previous.intensity != next.intensity
        || previous.reason != next.reason
}
