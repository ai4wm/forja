use async_trait::async_trait;
use forja_core::error::Result;
use forja_core::traits::MemoryStore;
use forja_core::types::MemoryEntry;
use std::path::Path;
use std::fmt::Display;

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

    pub async fn load_startup_context(&self) -> Result<String> {
        self.storage.read_startup_context().await
    }

    pub async fn load_relevant(&self, query: &str) -> Result<String> {
        self.storage.read_relevant(query).await
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

    async fn load_startup_context(&self) -> Result<String> {
        self.storage.read_startup_context().await
    }

    async fn load_relevant(&self, query: &str) -> Result<String> {
        self.storage.read_relevant(query).await
    }

    async fn flush(&self) -> Result<()> {
        self.storage.reconcile().await
    }
}
