use super::UnresolvedTask;
use crate::error::{ForjaError, Result};
use chrono::Utc;
use rusqlite::{Connection, params};
use std::path::Path;
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub struct UnresolvedStore {
    db: Arc<Mutex<Connection>>,
}

impl UnresolvedStore {
    pub fn new(db_path: &Path) -> Result<Self> {
        let connection =
            Connection::open(db_path).map_err(|error| ForjaError::Storage(error.to_string()))?;
        connection
            .execute(
                "CREATE TABLE IF NOT EXISTS unresolved (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    task TEXT NOT NULL,
                    error TEXT,
                    retry_count INTEGER DEFAULT 0,
                    max_retries INTEGER DEFAULT 3,
                    created_at TEXT NOT NULL,
                    last_tried TEXT,
                    status TEXT DEFAULT 'pending'
                )",
                [],
            )
            .map_err(|error| ForjaError::Storage(error.to_string()))?;

        Ok(Self {
            db: Arc::new(Mutex::new(connection)),
        })
    }

    pub fn add(&self, task: &str, error: &str, max_retries: u32) -> Result<()> {
        let connection = self.lock_connection()?;
        connection
            .execute(
                "INSERT INTO unresolved (
                    task, error, retry_count, max_retries, created_at, last_tried, status
                ) VALUES (?1, ?2, 0, ?3, ?4, NULL, 'pending')",
                params![task, error, max_retries as i64, Utc::now().to_rfc3339()],
            )
            .map_err(|db_error| ForjaError::Storage(db_error.to_string()))?;
        Ok(())
    }

    pub fn get_pending(&self) -> Result<Vec<UnresolvedTask>> {
        let connection = self.lock_connection()?;
        let mut statement = connection
            .prepare(
                "SELECT id, task, error, retry_count, max_retries, created_at, last_tried, status
                 FROM unresolved
                 WHERE status = 'pending' AND retry_count < max_retries
                 ORDER BY id ASC",
            )
            .map_err(|error| ForjaError::Storage(error.to_string()))?;
        let rows = statement
            .query_map([], map_unresolved_row)
            .map_err(|error| ForjaError::Storage(error.to_string()))?;
        let mut tasks = Vec::new();
        for row in rows {
            tasks.push(row.map_err(|error| ForjaError::Storage(error.to_string()))?);
        }
        Ok(tasks)
    }

    pub fn increment_retry(&self, id: i64) -> Result<()> {
        let connection = self.lock_connection()?;
        connection
            .execute(
                "UPDATE unresolved
                 SET retry_count = retry_count + 1, last_tried = ?1
                 WHERE id = ?2",
                params![Utc::now().to_rfc3339(), id],
            )
            .map_err(|error| ForjaError::Storage(error.to_string()))?;
        Ok(())
    }

    pub fn mark_resolved(&self, id: i64) -> Result<()> {
        self.update_status(id, "resolved")
    }

    pub fn mark_failed(&self, id: i64) -> Result<()> {
        self.update_status(id, "failed")
    }

    pub fn list_all(&self) -> Result<Vec<UnresolvedTask>> {
        let connection = self.lock_connection()?;
        let mut statement = connection
            .prepare(
                "SELECT id, task, error, retry_count, max_retries, created_at, last_tried, status
                 FROM unresolved
                 ORDER BY id ASC",
            )
            .map_err(|error| ForjaError::Storage(error.to_string()))?;
        let rows = statement
            .query_map([], map_unresolved_row)
            .map_err(|error| ForjaError::Storage(error.to_string()))?;
        let mut tasks = Vec::new();
        for row in rows {
            tasks.push(row.map_err(|error| ForjaError::Storage(error.to_string()))?);
        }
        Ok(tasks)
    }

    fn update_status(&self, id: i64, status: &str) -> Result<()> {
        let connection = self.lock_connection()?;
        connection
            .execute(
                "UPDATE unresolved SET status = ?1 WHERE id = ?2",
                params![status, id],
            )
            .map_err(|error| ForjaError::Storage(error.to_string()))?;
        Ok(())
    }

    fn lock_connection(&self) -> Result<std::sync::MutexGuard<'_, Connection>> {
        self.db
            .lock()
            .map_err(|error| ForjaError::Storage(error.to_string()))
    }
}

fn map_unresolved_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<UnresolvedTask> {
    Ok(UnresolvedTask {
        id: row.get(0)?,
        task: row.get(1)?,
        error: row.get(2)?,
        retry_count: row.get::<_, i64>(3)? as u32,
        max_retries: row.get::<_, i64>(4)? as u32,
        created_at: row.get(5)?,
        last_tried: row.get(6)?,
        status: row.get(7)?,
    })
}
