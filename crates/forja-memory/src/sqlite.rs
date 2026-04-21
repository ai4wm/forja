use crate::storage::Storage;
use chrono::TimeZone;
use forja_core::error::{ForjaError, Result};
use forja_core::types::MemoryEntry;
use rusqlite::{params, Connection};
use std::path::Path;
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub struct SqliteMemoryIndex {
    db: Arc<Mutex<Connection>>,
}

impl SqliteMemoryIndex {
    pub fn new(db_path: &Path) -> Result<Self> {
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|error| ForjaError::Storage(error.to_string()))?;
        }

        let connection = Connection::open(db_path)
            .map_err(|error| ForjaError::Storage(error.to_string()))?;
        connection
            .execute_batch(
                "CREATE TABLE IF NOT EXISTS memory_entries (
                    id TEXT PRIMARY KEY,
                    timestamp INTEGER NOT NULL,
                    role TEXT NOT NULL,
                    content TEXT NOT NULL,
                    source TEXT NOT NULL
                );
                CREATE VIRTUAL TABLE IF NOT EXISTS memory_entries_fts USING fts5(
                    id,
                    role,
                    content,
                    source
                );
                CREATE TABLE IF NOT EXISTS memory_summaries (
                    source TEXT PRIMARY KEY,
                    summary TEXT NOT NULL,
                    created_at INTEGER NOT NULL
                );
                CREATE VIRTUAL TABLE IF NOT EXISTS memory_summaries_fts USING fts5(
                    source,
                    summary
                );",
            )
            .map_err(|error| ForjaError::Storage(error.to_string()))?;

        Ok(Self {
            db: Arc::new(Mutex::new(connection)),
        })
    }

    pub async fn rebuild_from_storage(&self, storage: &Storage) -> Result<()> {
        let entries = storage.export_entry_rows().await?;
        let summaries = storage.export_summary_rows().await?;
        let connection = self.lock_connection()?;

        connection
            .execute("DELETE FROM memory_entries", [])
            .map_err(|error| ForjaError::Storage(error.to_string()))?;
        connection
            .execute("DELETE FROM memory_entries_fts", [])
            .map_err(|error| ForjaError::Storage(error.to_string()))?;
        connection
            .execute("DELETE FROM memory_summaries", [])
            .map_err(|error| ForjaError::Storage(error.to_string()))?;
        connection
            .execute("DELETE FROM memory_summaries_fts", [])
            .map_err(|error| ForjaError::Storage(error.to_string()))?;

        for entry in entries {
            connection
                .execute(
                    "INSERT INTO memory_entries (id, timestamp, role, content, source)
                     VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![entry.id, entry.timestamp as i64, entry.role, entry.content, entry.source],
                )
                .map_err(|error| ForjaError::Storage(error.to_string()))?;
            connection
                .execute(
                    "INSERT INTO memory_entries_fts (id, role, content, source)
                     VALUES (?1, ?2, ?3, ?4)",
                    params![entry.id, entry.role, entry.content, entry.source],
                )
                .map_err(|error| ForjaError::Storage(error.to_string()))?;
        }

        for summary in summaries {
            connection
                .execute(
                    "INSERT INTO memory_summaries (source, summary, created_at)
                     VALUES (?1, ?2, ?3)",
                    params![summary.source, summary.summary, summary.created_at as i64],
                )
                .map_err(|error| ForjaError::Storage(error.to_string()))?;
            connection
                .execute(
                    "INSERT INTO memory_summaries_fts (source, summary)
                     VALUES (?1, ?2)",
                    params![summary.source, summary.summary],
                )
                .map_err(|error| ForjaError::Storage(error.to_string()))?;
        }

        Ok(())
    }

    pub fn upsert_entry(&self, entry: &MemoryEntry) -> Result<()> {
        let role = entry_role(entry);
        let source = format!("live/{}", format_date(entry.timestamp));
        let connection = self.lock_connection()?;

        connection
            .execute(
                "INSERT OR REPLACE INTO memory_entries (id, timestamp, role, content, source)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![entry.id, entry.timestamp as i64, role, entry.content, source],
            )
            .map_err(|error| ForjaError::Storage(error.to_string()))?;
        connection
            .execute("DELETE FROM memory_entries_fts WHERE id = ?1", [entry.id.as_str()])
            .map_err(|error| ForjaError::Storage(error.to_string()))?;
        connection
            .execute(
                "INSERT INTO memory_entries_fts (id, role, content, source)
                 VALUES (?1, ?2, ?3, ?4)",
                params![entry.id, role, entry.content, source],
            )
            .map_err(|error| ForjaError::Storage(error.to_string()))?;
        Ok(())
    }

    pub fn search_context(
        &self,
        query: &str,
        entry_limit: usize,
        summary_limit: usize,
    ) -> Result<String> {
        let search_query = fts_query(query);
        if search_query.is_empty() {
            return Ok(String::new());
        }

        let connection = self.lock_connection()?;
        let mut sections = Vec::new();

        let mut entry_stmt = connection
            .prepare(
                "SELECT memory_entries.role, memory_entries.content, memory_entries.source
                 FROM memory_entries_fts
                 JOIN memory_entries ON memory_entries.id = memory_entries_fts.id
                 WHERE memory_entries_fts MATCH ?1
                 ORDER BY memory_entries.rowid DESC, bm25(memory_entries_fts)
                 LIMIT ?2",
            )
            .map_err(|error| ForjaError::Storage(error.to_string()))?;
        let rows = entry_stmt
            .query_map(params![search_query, entry_limit as i64], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })
            .map_err(|error| ForjaError::Storage(error.to_string()))?;
        let mut entry_lines = Vec::new();
        for row in rows {
            let (role, content, source) =
                row.map_err(|error| ForjaError::Storage(error.to_string()))?;
            entry_lines.push(format!("- {} | {} | {}", source, role, content));
        }
        if !entry_lines.is_empty() {
            sections.push(format!(
                "[memory long-term - SQLite FTS Memory]\n\n{}",
                entry_lines.join("\n")
            ));
        }

        let mut summary_stmt = connection
            .prepare(
                "SELECT memory_summaries.source, memory_summaries.summary
                 FROM memory_summaries_fts
                 JOIN memory_summaries ON memory_summaries.source = memory_summaries_fts.source
                 WHERE memory_summaries_fts MATCH ?1
                 ORDER BY bm25(memory_summaries_fts)
                 LIMIT ?2",
            )
            .map_err(|error| ForjaError::Storage(error.to_string()))?;
        let summary_rows = summary_stmt
            .query_map(params![fts_query(query), summary_limit as i64], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(|error| ForjaError::Storage(error.to_string()))?;
        let mut summary_lines = Vec::new();
        for row in summary_rows {
            let (source, summary) =
                row.map_err(|error| ForjaError::Storage(error.to_string()))?;
            summary_lines.push(format!("## {source}\n{summary}"));
        }
        if !summary_lines.is_empty() {
            sections.push(format!(
                "[memory mid-term - SQLite Summary Memory]\n\n{}",
                summary_lines.join("\n\n")
            ));
        }

        Ok(sections.join("\n\n"))
    }

    pub fn recent_summary_context(&self, limit: usize) -> Result<String> {
        let connection = self.lock_connection()?;
        let mut statement = connection
            .prepare(
                "SELECT source, summary
                 FROM memory_summaries
                 ORDER BY created_at DESC
                 LIMIT ?1",
            )
            .map_err(|error| ForjaError::Storage(error.to_string()))?;
        let rows = statement
            .query_map([limit as i64], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(|error| ForjaError::Storage(error.to_string()))?;

        let mut sections = Vec::new();
        for row in rows {
            let (source, summary) =
                row.map_err(|error| ForjaError::Storage(error.to_string()))?;
            sections.push(format!("## {source}\n{summary}"));
        }

        if sections.is_empty() {
            return Ok(String::new());
        }

        Ok(format!(
            "[memory mid-term - SQLite Summary Memory]\n\n{}",
            sections.join("\n\n")
        ))
    }

    fn lock_connection(&self) -> Result<std::sync::MutexGuard<'_, Connection>> {
        self.db
            .lock()
            .map_err(|error| ForjaError::Storage(error.to_string()))
    }
}

fn fts_query(query: &str) -> String {
    tokenize(query)
        .into_iter()
        .map(|token| format!("{token}*"))
        .collect::<Vec<_>>()
        .join(" OR ")
}

fn tokenize(value: &str) -> Vec<String> {
    value.to_lowercase()
        .chars()
        .map(|char| match char {
            'a'..='z' | '0'..='9' => char,
            _ => ' ',
        })
        .collect::<String>()
        .split_whitespace()
        .filter(|token| !matches!(*token, "the" | "and" | "for" | "with" | "this" | "that"))
        .map(str::to_string)
        .collect()
}

fn entry_role(entry: &MemoryEntry) -> &str {
    entry
        .tags
        .iter()
        .find_map(|tag| match tag.as_str() {
            "assistant" => Some("assistant"),
            "system" => Some("system"),
            "tool" => Some("tool"),
            "user" => Some("user"),
            _ => None,
        })
        .unwrap_or("user")
}

fn format_date(timestamp: u64) -> String {
    chrono::Local
        .timestamp_opt(timestamp as i64, 0)
        .single()
        .unwrap_or_else(|| chrono::Local.timestamp_opt(0, 0).earliest().unwrap())
        .format("%Y-%m-%d")
        .to_string()
}

#[derive(Clone)]
pub struct SqliteEntryRow {
    pub id: String,
    pub timestamp: u64,
    pub role: String,
    pub content: String,
    pub source: String,
}

#[derive(Clone)]
pub struct SqliteSummaryRow {
    pub source: String,
    pub summary: String,
    pub created_at: u64,
}
