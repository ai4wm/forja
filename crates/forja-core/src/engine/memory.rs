use super::Engine;
use crate::error::Result;
use crate::types::{Content, MemoryEntry, Message};
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

impl Engine {
    pub(super) async fn refresh_turn_memory_context(&mut self, _user_msg: &Message) {
        self.turn_memory_context = self.build_turn_memory_context().await;
    }

    pub(super) fn clear_turn_memory_context(&mut self) {
        self.turn_memory_context = None;
    }

    pub(super) async fn save_turn_memory_entries(
        &self,
        user_msg: &Message,
        assistant_text: Option<&str>,
    ) {
        let Some(memory) = &self.memory else {
            return;
        };

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        if let Content::Text { text, .. } = &user_msg.content {
            let entry = MemoryEntry {
                id: format!("user_{}_{}", now, user_msg.id),
                timestamp: now,
                tags: vec!["user".to_string()],
                content: text.clone(),
                score: 0.0,
                metadata: Default::default(),
            };

            if let Err(error) = memory.save(&entry).await {
                eprintln!("[Memory] failed to save user entry: {error}");
            }
        }

        if let Some(text) = assistant_text {
            let entry = MemoryEntry {
                id: format!("assistant_{}_{}", now + 1, Uuid::new_v4()),
                timestamp: now + 1,
                tags: vec!["assistant".to_string()],
                content: text.to_string(),
                score: 0.0,
                metadata: Default::default(),
            };

            if let Err(error) = memory.save(&entry).await {
                eprintln!("[Memory] failed to save assistant entry: {error}");
            }
        }
    }

    pub(super) async fn check_and_flush_context(&mut self) -> Result<()> {
        let estimated_tokens: usize = self
            .conversation_history
            .iter()
            .map(|message| message.content_text_len() / 4)
            .sum();

        if estimated_tokens > 32_000 {
            if let Some(memory) = &self.memory {
                memory.flush().await?;
            }
            let drain_count = self.conversation_history.len() / 2;
            self.conversation_history.drain(0..drain_count);
        }

        Ok(())
    }

    async fn build_turn_memory_context(&self) -> Option<String> {
        let memory = self.memory.as_ref()?;
        let contents = match memory.load_all().await {
            Ok(contents) => contents,
            Err(error) => {
                eprintln!("[Memory] load_all failed: {error}");
                return None;
            }
        };

        let trimmed = contents.trim();
        if trimmed.is_empty() {
            return None;
        }

        Some(format_memory_context(trimmed))
    }
}

fn format_memory_context(contents: &str) -> String {
    format!(
        "[memory.md]\n당신은 롤링 메모리 방식으로 동작합니다.\n모든 대화는 영구 저장됩니다.\n아래에 과거 대화 기록이 포함되어 있습니다.\n'세션'이라는 표현을 사용하지 마세요.\nUse this rolling conversation memory only when it is relevant.\n{contents}"
    )
}
