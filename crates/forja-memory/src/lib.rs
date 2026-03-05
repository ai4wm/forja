use async_trait::async_trait;
use forja_core::traits::MemoryStore;
use forja_core::types::MemoryEntry;
use forja_core::error::Result;

pub mod storage;
pub mod tokenizer;

use storage::Storage;
use tokenizer::{Bm25Tokenizer, DocumentIndex};

pub struct MarkdownMemoryStore {
    storage: Storage,
    tokenizer: Bm25Tokenizer,
}

impl MarkdownMemoryStore {
    pub async fn new(base_dir: impl AsRef<std::path::Path>) -> Result<Self> {
        let storage = Storage::init(base_dir).await?;
        let tokenizer = Bm25Tokenizer::new();
        Ok(Self { storage, tokenizer })
    }

    // 내부 메서드로 유지 (필요 시 직접 호출용)
    pub async fn archive_old_files(&self, retain_count: usize) -> Result<()> {
        self.storage.archive_old_files(retain_count).await
    }
}

#[async_trait]
impl MemoryStore for MarkdownMemoryStore {
    async fn save(&self, entry: &MemoryEntry) -> Result<()> {
        self.storage.write_entry(entry).await
    }

    async fn search(&self, query: &str, limit: usize) -> Result<Vec<MemoryEntry>> {
        let mut entries = self.storage.read_all_entries().await?;
        
        // 문서 파싱하여 인덱스 객체 목록 구성
        let docs: Vec<DocumentIndex> = entries
            .iter()
            .map(|e| Bm25Tokenizer::build_doc_index(e.id.clone(), &e.content))
            .collect();

        // BM25 유사도 점수 산출
        let scores = self.tokenizer.score_documents(query, &docs);
        
        let mut score_map = std::collections::HashMap::new();
        for (id, score) in scores {
            score_map.insert(id, score);
        }

        // 각 엔트리에 점수 할당 및 빈 점수 필터링
        for entry in &mut entries {
            if let Some(&s) = score_map.get(&entry.id) {
                entry.score = s;
            }
        }

        // 점수가 높은 순으로 내림차순 정렬
        entries.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));

        // 0점 이하는 필터링하고 limit 개수만큼 반환
        let results: Vec<MemoryEntry> = entries
            .into_iter()
            .filter(|e| e.score > 0.0)
            .take(limit)
            .collect();

        Ok(results)
    }

    async fn flush(&self) -> Result<()> {
        // 기본값: 최근 50개 세션 텍스트만 유지하고 
        // 나머지는 fragments 디렉토리로 아카이브 이동시킴
        self.archive_old_files(50).await
    }
}
