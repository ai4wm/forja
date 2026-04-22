use async_trait::async_trait;
use forja_core::context::token_counter::count_messages_tokens;
use forja_core::error::Result;
use forja_core::traits::{DreamRunOutcome, DreamRunStatus, DreamTrigger, MemoryStore};
use forja_core::types::{MemoryEntry, Message, Role};
use std::fmt::Display;
use std::path::Path;
use tokio::sync::Mutex;

mod session;
mod sqlite;
pub mod storage;

use session::SessionMemory;
use sqlite::SqliteMemoryIndex;
use storage::Storage;

pub struct MarkdownMemoryStore {
    storage: Storage,
    sqlite: SqliteMemoryIndex,
    dream_lock: Mutex<()>,
    session: Mutex<SessionMemory>,
}

impl MarkdownMemoryStore {
    pub async fn new(memory_path: impl AsRef<Path>) -> Result<Self> {
        let storage = Storage::init(memory_path).await?;
        let sqlite = SqliteMemoryIndex::new(&storage.memory_db_path())?;
        sqlite.rebuild_from_storage(&storage).await?;
        Ok(Self {
            storage,
            sqlite,
            dream_lock: Mutex::new(()),
            session: Mutex::new(SessionMemory::new()),
        })
    }

    pub async fn load_startup_context(&self) -> Result<String> {
        let startup = self.storage.read_startup_context().await?;
        let sqlite_summaries = self.sqlite.recent_summary_context(3)?;
        let session_context = self.session.lock().await.startup_context();
        Ok(join_non_empty_sections([
            startup,
            sqlite_summaries,
            session_context,
        ]))
    }

    pub async fn load_relevant(&self, query: &str) -> Result<String> {
        let markdown = self.storage.read_relevant(query).await?;
        let sqlite = self.sqlite.search_context(query, 3, 2)?;
        let session = self.session.lock().await.relevant_context(query);
        Ok(join_non_empty_sections([markdown, sqlite, session]))
    }

    pub async fn flush_and_summarize<F, O>(&self, summarizer: F) -> Result<()>
    where
        F: Fn(String) -> O,
        O: SummarizeOutput,
    {
        self.storage
            .flush_and_summarize(|block| summarizer(block).into_summary_result())
            .await?;
        self.sqlite.rebuild_from_storage(&self.storage).await
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
        self.storage.append_entry(entry).await?;
        self.sqlite.upsert_entry(entry)?;
        self.session.lock().await.record(entry);
        Ok(())
    }

    async fn load_all(&self) -> Result<String> {
        let markdown = self.storage.read_all().await?;
        let sqlite = self.sqlite.recent_summary_context(5)?;
        let session = self.session.lock().await.full_context();
        Ok(join_non_empty_sections([markdown, sqlite, session]))
    }

    async fn load_startup_context(&self) -> Result<String> {
        MarkdownMemoryStore::load_startup_context(self).await
    }

    async fn load_relevant(&self, query: &str) -> Result<String> {
        MarkdownMemoryStore::load_relevant(self, query).await
    }

    async fn flush(&self) -> Result<()> {
        self.storage.reconcile().await?;
        self.sqlite.rebuild_from_storage(&self.storage).await
    }

    async fn run_dream(&self, trigger: DreamTrigger) -> Result<DreamRunOutcome> {
        let _guard = self.dream_lock.lock().await;
        let outcome = self.storage.run_dream(trigger).await?;
        if outcome.status != DreamRunStatus::AbortedConflict {
            self.sqlite.rebuild_from_storage(&self.storage).await?;
        }
        Ok(outcome)
    }

    async fn latest_dream_timestamp(&self) -> Result<Option<u64>> {
        self.storage.latest_dream_timestamp().await
    }
}

fn join_non_empty_sections<const N: usize>(sections: [String; N]) -> String {
    sections
        .into_iter()
        .filter(|section| !section.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n\n")
}

pub(crate) fn estimate_tokens(text: &str) -> usize {
    if text.trim().is_empty() {
        return 0;
    }

    let message = Message::text(Role::System, text, None);
    count_messages_tokens(&[message], "cl100k_base")
}
