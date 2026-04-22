use super::Skill;
use crate::error::{ForjaError, Result};
use chrono::Utc;
use rusqlite::{Connection, params};
use std::path::Path;
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub struct SkillRegistry {
    db: Arc<Mutex<Connection>>,
}

impl SkillRegistry {
    pub fn new(db_path: &Path) -> Result<Self> {
        let connection =
            Connection::open(db_path).map_err(|error| ForjaError::Storage(error.to_string()))?;
        connection
            .execute(
                "CREATE TABLE IF NOT EXISTS skills (
                    tool_name TEXT PRIMARY KEY,
                    success_count INTEGER DEFAULT 0,
                    last_used TEXT,
                    auto_approved INTEGER DEFAULT 0
                )",
                [],
            )
            .map_err(|error| ForjaError::Storage(error.to_string()))?;

        Ok(Self {
            db: Arc::new(Mutex::new(connection)),
        })
    }

    pub fn record_success(&self, tool_name: &str) -> Result<()> {
        let connection = self.lock_connection()?;
        connection
            .execute(
                "INSERT INTO skills (tool_name, success_count, last_used, auto_approved)
                 VALUES (?1, 1, ?2, 0)
                 ON CONFLICT(tool_name) DO UPDATE SET
                    success_count = success_count + 1,
                    last_used = excluded.last_used",
                params![tool_name, Utc::now().to_rfc3339()],
            )
            .map_err(|error| ForjaError::Storage(error.to_string()))?;
        Ok(())
    }

    pub fn is_auto_approved(&self, tool_name: &str) -> Result<bool> {
        let connection = self.lock_connection()?;
        let mut statement = connection
            .prepare("SELECT auto_approved FROM skills WHERE tool_name = ?1")
            .map_err(|error| ForjaError::Storage(error.to_string()))?;
        let status = statement.query_row([tool_name], |row| row.get::<_, i64>(0));

        match status {
            Ok(value) => Ok(value != 0),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(false),
            Err(error) => Err(ForjaError::Storage(error.to_string())),
        }
    }

    pub fn list_skills(&self) -> Result<Vec<Skill>> {
        let connection = self.lock_connection()?;
        let mut statement = connection
            .prepare(
                "SELECT tool_name, success_count, last_used, auto_approved
                 FROM skills
                 ORDER BY tool_name ASC",
            )
            .map_err(|error| ForjaError::Storage(error.to_string()))?;
        let rows = statement
            .query_map([], |row| {
                Ok(Skill {
                    tool_name: row.get(0)?,
                    success_count: row.get::<_, i64>(1)? as u32,
                    last_used: row.get(2)?,
                    auto_approved: row.get::<_, i64>(3)? != 0,
                })
            })
            .map_err(|error| ForjaError::Storage(error.to_string()))?;

        let mut skills = Vec::new();
        for row in rows {
            skills.push(row.map_err(|error| ForjaError::Storage(error.to_string()))?);
        }
        Ok(skills)
    }

    pub fn check_and_promote(&self, tool_name: &str, threshold: u32) -> Result<bool> {
        let connection = self.lock_connection()?;
        let mut statement = connection
            .prepare("SELECT success_count, auto_approved FROM skills WHERE tool_name = ?1")
            .map_err(|error| ForjaError::Storage(error.to_string()))?;
        let row = statement.query_row([tool_name], |row| {
            Ok((row.get::<_, i64>(0)? as u32, row.get::<_, i64>(1)? != 0))
        });

        match row {
            Ok((count, approved)) if !approved && count >= threshold => {
                connection
                    .execute(
                        "UPDATE skills SET auto_approved = 1 WHERE tool_name = ?1",
                        [tool_name],
                    )
                    .map_err(|error| ForjaError::Storage(error.to_string()))?;
                Ok(true)
            }
            Ok(_) => Ok(false),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(false),
            Err(error) => Err(ForjaError::Storage(error.to_string())),
        }
    }

    fn lock_connection(&self) -> Result<std::sync::MutexGuard<'_, Connection>> {
        self.db
            .lock()
            .map_err(|error| ForjaError::Storage(error.to_string()))
    }
}
