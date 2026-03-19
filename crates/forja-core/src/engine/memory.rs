use super::Engine;
use crate::error::Result;
use crate::types::{Content, MemoryEntry, Message};
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

const MEMORY_SEARCH_LIMIT: usize = 3;
const MAX_MEMORY_SNIPPET_CHARS: usize = 240;

impl Engine {
    pub(super) async fn refresh_turn_memory_context(&mut self, user_msg: &Message) {
        self.turn_memory_context = self.build_turn_memory_context(user_msg).await;
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

    async fn build_turn_memory_context(&self, user_msg: &Message) -> Option<String> {
        let query = match &user_msg.content {
            Content::Text { text, .. } => text.trim(),
            _ => return None,
        };

        if query.is_empty() {
            return None;
        }

        let memory = self.memory.as_ref()?;
        let entries = match memory.search(query, MEMORY_SEARCH_LIMIT).await {
            Ok(entries) => entries,
            Err(error) => {
                eprintln!("[Memory] search failed: {error}");
                return None;
            }
        };

        if entries.is_empty() {
            return None;
        }

        Some(format_memory_context(&entries))
    }
}

fn format_memory_context(entries: &[MemoryEntry]) -> String {
    let mut lines = vec![
        "Relevant memory context from prior conversation:".to_string(),
        "Use it only if it is relevant to the current request.".to_string(),
    ];

    for (index, entry) in entries.iter().enumerate() {
        lines.push(format!(
            "{}. [score={:.2}] {}",
            index + 1,
            entry.score,
            truncate_memory_content(&entry.content)
        ));
    }

    lines.join("\n")
}

fn truncate_memory_content(content: &str) -> String {
    let normalized = content.split_whitespace().collect::<Vec<_>>().join(" ");

    if normalized.chars().count() <= MAX_MEMORY_SNIPPET_CHARS {
        return normalized;
    }

    let snippet: String = normalized.chars().take(MAX_MEMORY_SNIPPET_CHARS).collect();
    format!("{snippet}...")
}
