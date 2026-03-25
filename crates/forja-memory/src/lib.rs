use async_trait::async_trait;
use forja_core::error::Result;
use forja_core::traits::MemoryStore;
use forja_core::types::MemoryEntry;
use std::path::Path;

pub mod storage;

use storage::Storage;

pub struct MarkdownMemoryStore {
    storage: Storage,
}

impl MarkdownMemoryStore {
    pub async fn new(memory_path: impl AsRef<Path>) -> Result<Self> {
        let storage = Storage::init(memory_path).await?;
        Ok(Self { storage })
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
