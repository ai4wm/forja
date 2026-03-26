use super::Engine;
use crate::serendipity::SerendipityEngine;
use crate::types::{Content, Message};
use chrono::Local;

impl Engine {
    pub fn with_serendipity(mut self, serendipity: SerendipityEngine) -> Self {
        self.serendipity = Some(serendipity);
        self
    }

    pub(super) fn begin_user_turn(&mut self) {
        self.turn_count = self.turn_count.saturating_add(1);
    }

    pub(super) async fn maybe_append_serendipity_to_message(
        &mut self,
        response: Message,
    ) -> Message {
        let Message {
            id,
            role,
            content,
            timestamp,
            metadata,
        } = response;

        let content = match content {
            Content::Text {
                text,
                thought_signature,
            } => Content::Text {
                text: self.maybe_append_serendipity_to_text(text).await,
                thought_signature,
            },
            other => other,
        };

        Message {
            id,
            role,
            content,
            timestamp,
            metadata,
        }
    }

    pub(super) async fn maybe_append_serendipity_to_text(&mut self, text: String) -> String {
        let Some(serendipity) = &self.serendipity else {
            return text;
        };
        if !serendipity.should_trigger(self.turn_count, self.last_serendipity_triggered_at) {
            return text;
        }

        #[cfg(feature = "memory")]
        let memory = self.load_memory_contents_or_empty().await;
        #[cfg(not(feature = "memory"))]
        let memory = String::new();

        let knowledge = if let Some(knowledge) = &self.knowledge {
            match knowledge.load_all_context() {
                Ok(knowledge) => knowledge,
                Err(error) => {
                    eprintln!("[Serendipity] load_all_context failed: {error}");
                    String::new()
                }
            }
        } else {
            String::new()
        };
        self.last_serendipity_triggered_at = Some(Local::now());

        let insight = match serendipity
            .generate_insight(&memory, &knowledge, self.provider.as_ref())
            .await
        {
            Ok(Some(insight)) => insight,
            Ok(None) => return text,
            Err(error) => {
                eprintln!("[Serendipity] generate_insight failed: {error}");
                return text;
            }
        };

        format!("{text}\n\n참고: {insight}")
    }
}
