use super::{DashboardError, DashboardState, open_read_only, table_exists};
use axum::Json;
use axum::extract::{Query, State};
use rusqlite::params;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Deserialize)]
pub(crate) struct MemoryQuery {
    q: Option<String>,
    limit: Option<usize>,
}

#[derive(Debug, Serialize)]
pub(crate) struct MemoryStateRow {
    memory_entries: i64,
    memory_summaries: i64,
}

#[derive(Debug, Serialize)]
pub(crate) struct MemoryEntryRow {
    id: String,
    timestamp: i64,
    role: String,
    content: String,
    source: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct MemorySummaryRow {
    source: String,
    summary: String,
    created_at: i64,
}

pub(crate) async fn get_memory(
    State(state): State<DashboardState>,
) -> Result<Json<MemoryStateRow>, DashboardError> {
    let memory_db_path = memory_db_path(&state.db_path);
    if !memory_db_path.exists() {
        return Ok(Json(MemoryStateRow {
            memory_entries: 0,
            memory_summaries: 0,
        }));
    }

    let connection = open_read_only(&memory_db_path)?;
    let memory_entries = if table_exists(&connection, "memory_entries")? {
        connection.query_row("SELECT COUNT(*) FROM memory_entries", [], |row| row.get(0))?
    } else {
        0
    };
    let memory_summaries = if table_exists(&connection, "memory_summaries")? {
        connection.query_row("SELECT COUNT(*) FROM memory_summaries", [], |row| {
            row.get(0)
        })?
    } else {
        0
    };

    Ok(Json(MemoryStateRow {
        memory_entries,
        memory_summaries,
    }))
}

pub(crate) async fn get_memory_entries(
    State(state): State<DashboardState>,
    Query(query): Query<MemoryQuery>,
) -> Result<Json<Vec<MemoryEntryRow>>, DashboardError> {
    let memory_db_path = memory_db_path(&state.db_path);
    if !memory_db_path.exists() {
        return Ok(Json(Vec::new()));
    }

    let limit = query.limit.unwrap_or(12).min(50) as i64;
    let connection = open_read_only(&memory_db_path)?;
    if !table_exists(&connection, "memory_entries")? {
        return Ok(Json(Vec::new()));
    }

    let rows = if let Some(search) = normalized_query(query.q.as_deref()) {
        let mut statement = connection.prepare(
            "SELECT memory_entries.id, memory_entries.timestamp, memory_entries.role,
                    memory_entries.content, memory_entries.source
             FROM memory_entries_fts
             JOIN memory_entries ON memory_entries.id = memory_entries_fts.id
             WHERE memory_entries_fts MATCH ?1
             ORDER BY memory_entries.timestamp DESC, bm25(memory_entries_fts)
             LIMIT ?2",
        )?;
        let rows = statement.query_map(params![search, limit], |row| {
            Ok(MemoryEntryRow {
                id: row.get(0)?,
                timestamp: row.get(1)?,
                role: row.get(2)?,
                content: row.get(3)?,
                source: row.get(4)?,
            })
        })?;
        collect_rows(rows)?
    } else {
        let mut statement = connection.prepare(
            "SELECT id, timestamp, role, content, source
             FROM memory_entries
             ORDER BY timestamp DESC
             LIMIT ?1",
        )?;
        let rows = statement.query_map([limit], |row| {
            Ok(MemoryEntryRow {
                id: row.get(0)?,
                timestamp: row.get(1)?,
                role: row.get(2)?,
                content: row.get(3)?,
                source: row.get(4)?,
            })
        })?;
        collect_rows(rows)?
    };

    Ok(Json(rows))
}

pub(crate) async fn get_memory_summaries(
    State(state): State<DashboardState>,
    Query(query): Query<MemoryQuery>,
) -> Result<Json<Vec<MemorySummaryRow>>, DashboardError> {
    let memory_db_path = memory_db_path(&state.db_path);
    if !memory_db_path.exists() {
        return Ok(Json(Vec::new()));
    }

    let limit = query.limit.unwrap_or(8).min(30) as i64;
    let connection = open_read_only(&memory_db_path)?;
    if !table_exists(&connection, "memory_summaries")? {
        return Ok(Json(Vec::new()));
    }

    let rows = if let Some(search) = normalized_query(query.q.as_deref()) {
        let mut statement = connection.prepare(
            "SELECT memory_summaries.source, memory_summaries.summary, memory_summaries.created_at
             FROM memory_summaries_fts
             JOIN memory_summaries ON memory_summaries.source = memory_summaries_fts.source
             WHERE memory_summaries_fts MATCH ?1
             ORDER BY bm25(memory_summaries_fts), memory_summaries.created_at DESC
             LIMIT ?2",
        )?;
        let rows = statement.query_map(params![search, limit], |row| {
            Ok(MemorySummaryRow {
                source: row.get(0)?,
                summary: row.get(1)?,
                created_at: row.get(2)?,
            })
        })?;
        collect_rows(rows)?
    } else {
        let mut statement = connection.prepare(
            "SELECT source, summary, created_at
             FROM memory_summaries
             ORDER BY created_at DESC
             LIMIT ?1",
        )?;
        let rows = statement.query_map([limit], |row| {
            Ok(MemorySummaryRow {
                source: row.get(0)?,
                summary: row.get(1)?,
                created_at: row.get(2)?,
            })
        })?;
        collect_rows(rows)?
    };

    Ok(Json(rows))
}

fn memory_db_path(db_path: &Path) -> PathBuf {
    db_path
        .parent()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("memory")
        .join("memory.db")
}

fn normalized_query(value: Option<&str>) -> Option<String> {
    let query = value.unwrap_or_default().trim().to_lowercase();
    if query.is_empty() {
        return None;
    }

    let tokens = query
        .chars()
        .map(|char| match char {
            'a'..='z' | '0'..='9' => char,
            _ => ' ',
        })
        .collect::<String>()
        .split_whitespace()
        .filter(|token| !matches!(*token, "the" | "and" | "for" | "with" | "this" | "that"))
        .map(|token| format!("{token}*"))
        .collect::<Vec<_>>();

    if tokens.is_empty() {
        None
    } else {
        Some(tokens.join(" OR "))
    }
}

fn collect_rows<T>(
    rows: rusqlite::MappedRows<'_, impl FnMut(&rusqlite::Row<'_>) -> rusqlite::Result<T>>,
) -> Result<Vec<T>, DashboardError> {
    let mut collected = Vec::new();
    for row in rows {
        collected.push(row?);
    }
    Ok(collected)
}
