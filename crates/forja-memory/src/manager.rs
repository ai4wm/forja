use crate::compressor::{CompressedEntry, Compressor};
use crate::longterm::LongTermStore;
use crate::session::SessionBuffer;
use forja_core::error::Result;
use forja_core::types::{Content, Message, Role};
use std::path::Path;

const DEFAULT_TOKEN_THRESHOLD: usize = 4_000;
const DEFAULT_RECALL_LIMIT: usize = 5;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MemoryCommand {
    Stats,
    Search(String),
    ClearSession,
    Flush,
}

pub fn parse_memory_command(input: &str) -> Option<MemoryCommand> {
    let normalized = input.trim();
    if normalized == "/memory" {
        return Some(MemoryCommand::Stats);
    }
    if normalized == "/memory clear session" {
        return Some(MemoryCommand::ClearSession);
    }
    if normalized == "/memory flush" {
        return Some(MemoryCommand::Flush);
    }
    normalized
        .strip_prefix("/memory search ")
        .map(str::trim)
        .filter(|query| !query.is_empty())
        .map(|query| MemoryCommand::Search(query.to_string()))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemoryStats {
    pub session_messages: usize,
    pub longterm_entries: usize,
    pub estimated_tokens: usize,
}

pub struct MemoryManager {
    session: SessionBuffer,
    compressor: Compressor,
    longterm: LongTermStore,
    token_threshold: usize,
}

impl MemoryManager {
    pub async fn new(longterm_path: impl AsRef<Path>) -> Result<Self> {
        Self::with_threshold(longterm_path, DEFAULT_TOKEN_THRESHOLD).await
    }

    pub async fn with_threshold(
        longterm_path: impl AsRef<Path>,
        token_threshold: usize,
    ) -> Result<Self> {
        Ok(Self {
            session: SessionBuffer::new(),
            compressor: Compressor::new(),
            longterm: LongTermStore::new(longterm_path).await?,
            token_threshold: token_threshold.max(1),
        })
    }

    pub async fn load(&mut self) -> Result<()> {
        let _ = self.longterm.load().await?;
        Ok(())
    }

    pub async fn record(&mut self, message: Message) -> Result<()> {
        self.session.add(message);
        if self.session.token_count() > self.token_threshold {
            self.compress_oldest_half().await?;
        }
        Ok(())
    }

    pub async fn recall(&self, query: &str, limit: usize) -> Result<Vec<CompressedEntry>> {
        self.longterm.search(query, limit).await
    }

    pub async fn get_context(&self, query: &str) -> Result<String> {
        let recent = self
            .session
            .get_recent(DEFAULT_RECALL_LIMIT)
            .into_iter()
            .filter_map(message_to_context_line)
            .collect::<Vec<_>>();

        let recalled = self.recall(query, DEFAULT_RECALL_LIMIT).await?;
        let mut sections = Vec::new();

        if !recent.is_empty() {
            sections.push(format!("## Recent session\n{}", recent.join("\n")));
        }

        if !recalled.is_empty() {
            let lines = recalled
                .iter()
                .map(|entry| {
                    let timestamp = entry.timestamp.format("%Y-%m-%d %H:%M").to_string();
                    let keywords = if entry.keywords.is_empty() {
                        String::new()
                    } else {
                        format!(" [{}]", entry.keywords.join(", "))
                    };
                    format!("- {timestamp}: {}{}", entry.summary, keywords)
                })
                .collect::<Vec<_>>();
            sections.push(format!("## Long-term memory\n{}", lines.join("\n")));
        }

        Ok(sections.join("\n\n"))
    }

    pub async fn flush(&mut self) -> Result<()> {
        if self.session.is_empty() {
            return Ok(());
        }

        let messages = self.session.drain_oldest(self.session.len());
        let entry = self.compressor.compress(messages);
        self.longterm.add(&entry).await
    }

    pub async fn clear_session(&mut self) {
        self.session.clear();
    }

    pub async fn stats(&self) -> Result<MemoryStats> {
        Ok(MemoryStats {
            session_messages: self.session.len(),
            longterm_entries: self.longterm.entry_count().await?,
            estimated_tokens: self.session.token_count(),
        })
    }

    async fn compress_oldest_half(&mut self) -> Result<()> {
        let count = (self.session.len() / 2).max(1);
        let messages = self.session.drain_oldest(count);
        if messages.is_empty() {
            return Ok(());
        }

        let entry = self.compressor.compress(messages);
        self.longterm.add(&entry).await
    }
}

fn message_to_context_line(message: Message) -> Option<String> {
    let Content::Text { text, .. } = message.content else {
        return None;
    };
    let role = match message.role {
        Role::User => "user",
        Role::Assistant => "assistant",
        Role::System => "system",
        Role::Tool => "tool",
    };
    Some(format!("- {role}: {}", text.trim()))
}

pub fn memory_entry_to_message(role_hint: &str, content: &str) -> Message {
    let role = match role_hint {
        "assistant" => Role::Assistant,
        "system" => Role::System,
        "tool" => Role::Tool,
        _ => Role::User,
    };
    Message::text(role, content, None)
}

#[cfg(test)]
mod tests {
    use super::{MemoryCommand, MemoryManager, parse_memory_command};
    use crate::longterm::longterm_path;
    use forja_core::types::{Message, Role};
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_temp_dir(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        std::env::temp_dir().join(format!("forja_manager_{name}_{nanos}"))
    }

    #[tokio::test]
    async fn memory_manager_auto_compresses_when_threshold_is_exceeded() {
        let base_dir = unique_temp_dir("compress");
        let mut manager = MemoryManager::with_threshold(longterm_path(&base_dir, None), 4)
            .await
            .unwrap();

        manager
            .record(Message::text(
                Role::User,
                "This message is long enough to exceed the threshold quickly.",
                None,
            ))
            .await
            .unwrap();
        manager
            .record(Message::text(
                Role::Assistant,
                "Another message that forces compression.",
                None,
            ))
            .await
            .unwrap();

        let stats = manager.stats().await.unwrap();
        assert!(stats.longterm_entries >= 1);
    }

    #[tokio::test]
    async fn memory_manager_recalls_relevant_entries() {
        let base_dir = unique_temp_dir("recall");
        let mut manager = MemoryManager::with_threshold(longterm_path(&base_dir, None), 4)
            .await
            .unwrap();
        manager
            .record(Message::text(
                Role::User,
                "Deploy the project with vercel after tests pass.",
                None,
            ))
            .await
            .unwrap();
        manager
            .record(Message::text(
                Role::Assistant,
                "Next action is to deploy after verification.",
                None,
            ))
            .await
            .unwrap();
        manager.flush().await.unwrap();

        let results = manager.recall("deploy vercel", 1).await.unwrap();
        assert_eq!(results.len(), 1);
        assert!(results[0].summary.to_lowercase().contains("deploy"));
    }

    #[test]
    fn parse_memory_command_matches_supported_commands() {
        assert_eq!(parse_memory_command("/memory"), Some(MemoryCommand::Stats));
        assert_eq!(
            parse_memory_command("/memory search deploy"),
            Some(MemoryCommand::Search("deploy".to_string()))
        );
        assert_eq!(
            parse_memory_command("/memory clear session"),
            Some(MemoryCommand::ClearSession)
        );
        assert_eq!(parse_memory_command("/memory flush"), Some(MemoryCommand::Flush));
    }
}
