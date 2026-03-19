use async_trait::async_trait;
use forja_core::types::MemoryEntry;
use forja_core::error::Result;
use forja_core::traits::MemoryStore;
use std::cmp::Ordering;
use std::collections::HashMap;
use std::sync::RwLock;
use std::time::{SystemTime, UNIX_EPOCH};

pub mod storage;
pub mod tokenizer;

use storage::Storage;
use tokenizer::{Bm25Tokenizer, DocumentIndex};

#[derive(Debug, Clone)]
pub struct MemoryLoadOptions {
    pub max_files: Option<usize>,
    pub recent_days: Option<u64>,
}

impl Default for MemoryLoadOptions {
    fn default() -> Self {
        Self {
            max_files: Some(500),
            recent_days: None,
        }
    }
}

#[derive(Default)]
struct MemoryIndex {
    entries: Vec<MemoryEntry>,
    docs: Vec<DocumentIndex>,
}

pub struct MarkdownMemoryStore {
    storage: Storage,
    tokenizer: Bm25Tokenizer,
    load_options: MemoryLoadOptions,
    index: RwLock<MemoryIndex>,
}

impl MarkdownMemoryStore {
    pub async fn new(base_dir: impl AsRef<std::path::Path>) -> Result<Self> {
        Self::new_with_options(base_dir, MemoryLoadOptions::default()).await
    }

    pub async fn new_with_options(
        base_dir: impl AsRef<std::path::Path>,
        load_options: MemoryLoadOptions,
    ) -> Result<Self> {
        let storage = Storage::init(base_dir).await?;
        let tokenizer = Bm25Tokenizer::new();
        let entries = Self::load_bootstrap_entries(&storage, &load_options).await?;
        let index = RwLock::new(Self::build_index(entries));

        Ok(Self {
            storage,
            tokenizer,
            load_options,
            index,
        })
    }

    // 내부 메서드로 유지 (필요 시 직접 호출용)
    pub async fn archive_old_files(&self, retain_count: usize) -> Result<()> {
        self.storage.archive_old_files(retain_count).await
    }

    fn build_index(entries: Vec<MemoryEntry>) -> MemoryIndex {
        let docs = entries
            .iter()
            .map(|entry| Bm25Tokenizer::build_doc_index(entry.id.clone(), &entry.content))
            .collect();

        MemoryIndex { entries, docs }
    }

    async fn load_bootstrap_entries(
        storage: &Storage,
        load_options: &MemoryLoadOptions,
    ) -> Result<Vec<MemoryEntry>> {
        let mut entries = storage.read_all_entries().await?;
        entries.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));

        if let Some(recent_days) = load_options.recent_days {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            let cutoff = now.saturating_sub(recent_days.saturating_mul(24 * 60 * 60));
            entries.retain(|entry| entry.timestamp >= cutoff);
        }

        if let Some(max_files) = load_options.max_files {
            entries.truncate(max_files);
        }

        Ok(entries)
    }

    async fn rebuild_index(&self) -> Result<()> {
        let entries = Self::load_bootstrap_entries(&self.storage, &self.load_options).await?;
        let next_index = Self::build_index(entries);
        let mut index = self.index.write().unwrap();
        *index = next_index;
        Ok(())
    }

    fn upsert_index_entry(index: &mut MemoryIndex, entry: &MemoryEntry) {
        let next_doc = Bm25Tokenizer::build_doc_index(entry.id.clone(), &entry.content);

        if let Some(position) = index
            .entries
            .iter()
            .position(|existing| existing.id == entry.id)
        {
            index.entries[position] = entry.clone();
            index.docs[position] = next_doc;
            return;
        }

        index.entries.push(entry.clone());
        index.docs.push(next_doc);
    }
}

#[async_trait]
impl MemoryStore for MarkdownMemoryStore {
    async fn save(&self, entry: &MemoryEntry) -> Result<()> {
        self.storage.write_entry(entry).await?;

        let mut index = self.index.write().unwrap();
        Self::upsert_index_entry(&mut index, entry);
        Ok(())
    }

    async fn search(&self, query: &str, limit: usize) -> Result<Vec<MemoryEntry>> {
        let index = self.index.read().unwrap();

        if index.entries.is_empty() {
            return Ok(Vec::new());
        }

        let scores = self.tokenizer.score_documents(query, &index.docs);
        let mut score_map = HashMap::new();
        for (id, score) in scores {
            score_map.insert(id, score);
        }

        let mut results: Vec<MemoryEntry> = index
            .entries
            .iter()
            .filter_map(|entry| {
                let score = *score_map.get(&entry.id)?;
                if score <= 0.0 {
                    return None;
                }

                let mut next_entry = entry.clone();
                next_entry.score = score;
                Some(next_entry)
            })
            .collect();

        results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(Ordering::Equal));
        results.truncate(limit);

        Ok(results)
    }

    async fn flush(&self) -> Result<()> {
        // 기본값: 최근 50개 세션 텍스트만 유지하고 
        // 나머지는 fragments 디렉토리로 아카이브 이동시킴
        self.archive_old_files(50).await?;
        self.rebuild_index().await
    }
}
