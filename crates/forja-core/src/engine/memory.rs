use super::Engine;
use crate::error::Result;
use crate::types::{Content, MemoryEntry, Message};
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

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
        self.compress_context().await?;
        Ok(())
    }

    pub(super) async fn flush_memory_store(&self) {
        let Some(memory) = &self.memory else {
            return;
        };

        if let Err(error) = memory.flush().await {
            eprintln!("[Memory] flush failed: {error}");
        }
    }

    async fn build_turn_memory_context(&self, user_msg: &Message) -> Option<String> {
        let Some(memory) = &self.memory else {
            return None;
        };

        let startup = match memory.load_startup_context().await {
            Ok(contents) => contents,
            Err(error) => {
                eprintln!("[Memory] load_startup_context failed: {error}");
                String::new()
            }
        };
        let relevant = match &user_msg.content {
            Content::Text { text, .. } => match memory.load_relevant(text).await {
                Ok(contents) => contents,
                Err(error) => {
                    eprintln!("[Memory] load_relevant failed: {error}");
                    String::new()
                }
            },
            _ => String::new(),
        };

        let contents = [startup.trim(), relevant.trim()]
            .into_iter()
            .filter(|section| !section.is_empty())
            .collect::<Vec<_>>()
            .join("\n\n");

        if contents.is_empty() {
            return None;
        }

        Some(format_memory_context(&contents))
    }

    pub(super) async fn load_memory_contents_or_empty(&self) -> String {
        let Some(memory) = &self.memory else {
            return String::new();
        };

        match memory.load_all().await {
            Ok(contents) => contents,
            Err(error) => {
                eprintln!("[Memory] load_all failed: {error}");
                String::new()
            }
        }
    }
}

fn format_memory_context(contents: &str) -> String {
    format!(
        "[memory - Structured Persistent Memory]\n\n## Mandatory Rules (NEVER violate)\n1. You have a rolling memory system. The records below are real past conversations and durable notes.\n2. When asked \"do you remember?\", if the information exists below, answer \"Yes, I remember.\"\n3. NEVER use phrases like \"current session\", \"provided in this conversation\", or \"I cannot browse past records.\"\n4. Only say \"I don't have that in my records\" if the information is truly absent below.\n5. Do NOT downplay your memory capabilities. The records below ARE your memory.\n\n## Memory context:\n{contents}"
    )
}
