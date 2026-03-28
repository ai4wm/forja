use async_trait::async_trait;
use forja_core::error::Result;
use forja_core::traits::MemoryStore;
use forja_core::types::MemoryEntry;
use std::fmt::Display;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

pub mod compressor;
pub mod longterm;
pub mod manager;
pub mod session;
pub mod storage;

use manager::{MemoryManager, MemoryStats, memory_entry_to_message};
use storage::Storage;

pub use compressor::{CompressedEntry, Compressor};
pub use longterm::{LongTermStore, longterm_path};
pub use manager::{MemoryCommand, MemoryManager as UnifiedMemoryManager, parse_memory_command};
pub use session::SessionBuffer;

pub struct MarkdownMemoryStore {
    storage: Storage,
}

impl MarkdownMemoryStore {
    pub async fn new(memory_path: impl AsRef<Path>) -> Result<Self> {
        let storage = Storage::init(memory_path).await?;
        Ok(Self { storage })
    }

    pub async fn flush_and_summarize<F, O>(&self, summarizer: F) -> Result<()>
    where
        F: Fn(String) -> O,
        O: SummarizeOutput,
    {
        self.storage
            .flush_and_summarize(|block| summarizer(block).into_summary_result())
            .await
    }
}

pub struct MemoryManagerStore {
    manager: Arc<tokio::sync::Mutex<MemoryManager>>,
    current_query: Arc<Mutex<String>>,
}

impl MemoryManagerStore {
    pub async fn new(base_dir: impl AsRef<Path>, agent_name: Option<&str>) -> Result<Self> {
        let manager = MemoryManager::new(longterm_path(base_dir.as_ref(), agent_name)).await?;
        Ok(Self {
            manager: Arc::new(tokio::sync::Mutex::new(manager)),
            current_query: Arc::new(Mutex::new(String::new())),
        })
    }

    pub fn set_current_query(&self, query: impl Into<String>) {
        if let Ok(mut current_query) = self.current_query.lock() {
            *current_query = query.into();
        }
    }

    pub async fn load(&self) -> Result<()> {
        self.manager.lock().await.load().await
    }

    pub async fn stats(&self) -> Result<MemoryStats> {
        self.manager.lock().await.stats().await
    }

    pub async fn search(&self, query: &str, limit: usize) -> Result<Vec<CompressedEntry>> {
        self.manager.lock().await.recall(query, limit).await
    }

    pub async fn clear_session(&self) {
        self.manager.lock().await.clear_session().await;
    }

    pub async fn flush_manager(&self) -> Result<()> {
        self.manager.lock().await.flush().await
    }
}

impl Clone for MemoryManagerStore {
    fn clone(&self) -> Self {
        Self {
            manager: self.manager.clone(),
            current_query: self.current_query.clone(),
        }
    }
}

pub trait SummarizeOutput {
    fn into_summary_result(self) -> std::result::Result<String, String>;
}

impl SummarizeOutput for String {
    fn into_summary_result(self) -> std::result::Result<String, String> {
        Ok(self)
    }
}

impl<E> SummarizeOutput for std::result::Result<String, E>
where
    E: Display,
{
    fn into_summary_result(self) -> std::result::Result<String, String> {
        self.map_err(|error| error.to_string())
    }
}

#[async_trait]
impl MemoryStore for MarkdownMemoryStore {
    async fn save(&self, entry: &MemoryEntry) -> Result<()> {
        self.storage.append_entry(entry).await
    }

    async fn load_all(&self) -> Result<String> {
        self.storage.read_all().await
    }

    async fn flush(&self) -> Result<()> {
        Ok(())
    }
}

#[async_trait]
impl MemoryStore for MemoryManagerStore {
    async fn save(&self, entry: &MemoryEntry) -> Result<()> {
        let role_hint = entry
            .tags
            .iter()
            .find(|tag| matches!(tag.as_str(), "assistant" | "system" | "tool" | "user"))
            .map(|tag| tag.as_str())
            .unwrap_or("user");
        let message = memory_entry_to_message(role_hint, &entry.content);
        self.manager.lock().await.record(message).await
    }

    async fn load_all(&self) -> Result<String> {
        let query = self
            .current_query
            .lock()
            .map(|query| query.clone())
            .unwrap_or_default();
        self.manager.lock().await.get_context(&query).await
    }

    async fn flush(&self) -> Result<()> {
        self.flush_manager().await
    }
}

impl From<tokio::sync::Mutex<MemoryManager>> for MemoryManagerStore {
    fn from(manager: tokio::sync::Mutex<MemoryManager>) -> Self {
        Self {
            manager: Arc::new(manager),
            current_query: Arc::new(Mutex::new(String::new())),
        }
    }
}

pub fn default_memory_base_dir() -> PathBuf {
    std::env::var("FORJA_MEMORY_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            dirs_next::home_dir()
                .unwrap_or_default()
                .join(".forja")
                .join("memory")
        })
}

#[cfg(test)]
mod tests {
    use super::MemoryManagerStore;
    use crate::longterm::longterm_path;
    use forja_core::traits::MemoryStore;
    use forja_core::types::MemoryEntry;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_temp_dir(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        std::env::temp_dir().join(format!("forja_memory_store_{name}_{nanos}"))
    }

    #[tokio::test]
    async fn memory_manager_store_flushes_to_longterm_file() {
        let base_dir = unique_temp_dir("flush");
        let store = MemoryManagerStore::new(&base_dir, None).await.unwrap();
        store
            .save(&MemoryEntry {
                id: "user_1".to_string(),
                timestamp: 1,
                tags: vec!["user".to_string()],
                content: "Remember the deploy checklist.".to_string(),
                score: 0.0,
                metadata: Default::default(),
            })
            .await
            .unwrap();
        store.flush().await.unwrap();

        let path = longterm_path(base_dir.as_path(), None);
        let contents = tokio::fs::read_to_string(path).await.unwrap();
        assert!(contents.contains("summary:"));
    }
}
