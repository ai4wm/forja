use super::Engine;
use crate::types::{Content, Message};

impl Engine {
    pub(super) async fn refresh_turn_knowledge_context(&mut self, user_msg: &Message) {
        self.turn_knowledge_context = None;

        let Some(knowledge) = &self.knowledge else {
            return;
        };
        let Content::Text { text, .. } = &user_msg.content else {
            return;
        };

        match knowledge.detect_topic(text, self.provider.as_ref()).await {
            Ok(Some(topic_entry)) => {
                if let Err(error) = knowledge.save_entry(&topic_entry) {
                    eprintln!("[Knowledge] save_entry failed: {error}");
                }
            }
            Ok(None) => {}
            Err(error) => {
                eprintln!("[Knowledge] detect_topic failed: {error}");
            }
        }

        self.turn_knowledge_context = match knowledge.load_relevant(text) {
            Ok(context) if !context.trim().is_empty() => Some(context),
            Ok(_) => None,
            Err(error) => {
                eprintln!("[Knowledge] load_relevant failed: {error}");
                None
            }
        };
    }

    pub(super) fn clear_turn_knowledge_context(&mut self) {
        self.turn_knowledge_context = None;
    }
}
