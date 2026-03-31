use super::skills::SkillRegistry;
use super::unresolved::UnresolvedStore;
use super::{AutonomyAction, AutonomyConfig, QueuedTask};
use crate::creation::DebateResult;
use crate::error::{ForjaError, Result};
use chrono::Utc;
use rusqlite::{params, Connection};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub struct AutonomousLoop {
    pub config: AutonomyConfig,
    pub skill_registry: SkillRegistry,
    pub unresolved_store: UnresolvedStore,
    pub db_path: PathBuf,
    db: Arc<Mutex<Connection>>,
}

impl AutonomousLoop {
    pub fn new(config: AutonomyConfig, db_path: impl AsRef<Path>) -> Result<Self> {
        let db_path = db_path.as_ref().to_path_buf();
        let connection = Connection::open(&db_path)
            .map_err(|error| ForjaError::Storage(error.to_string()))?;
        connection
            .execute(
                "CREATE TABLE IF NOT EXISTS task_queue (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    description TEXT NOT NULL,
                    source TEXT NOT NULL,
                    status TEXT DEFAULT 'pending',
                    created_at TEXT NOT NULL,
                    started_at TEXT,
                    completed_at TEXT,
                    result TEXT,
                    requires_approval INTEGER DEFAULT 1
                )",
                [],
            )
            .map_err(|error| ForjaError::Storage(error.to_string()))?;

        Ok(Self {
            skill_registry: SkillRegistry::new(&db_path)?,
            unresolved_store: UnresolvedStore::new(&db_path)?,
            config,
            db_path,
            db: Arc::new(Mutex::new(connection)),
        })
    }

    pub fn tick(&self) -> Result<Vec<AutonomyAction>> {
        if !self.config.enabled {
            return Ok(Vec::new());
        }

        let mut actions = Vec::new();
        for task in self.get_pending_tasks()? {
            let parsed = parse_task_description(&task.description);
            let requires_approval = self.config.require_approval && task.requires_approval;

            if requires_approval && !parsed.auto_approved {
                actions.push(AutonomyAction::AwaitingApproval {
                    task_id: task.id,
                    description: task.description,
                    source: task.source,
                });
                continue;
            }

            if let Some(tool_name) = parsed.tool_name {
                actions.push(AutonomyAction::ExecuteTask {
                    task_id: task.id,
                    description: task.description,
                    source: task.source,
                    tool_name,
                    args: parsed.args,
                });
            } else {
                actions.push(AutonomyAction::AwaitingApproval {
                    task_id: task.id,
                    description: task.description,
                    source: task.source,
                });
            }
        }

        for task in self.unresolved_store.get_pending()? {
            let next_retry = task.retry_count.saturating_add(1);
            if next_retry >= task.max_retries {
                self.unresolved_store.mark_failed(task.id)?;
                actions.push(AutonomyAction::FailedUnresolved {
                    id: task.id,
                    task: task.task,
                });
            } else {
                self.unresolved_store.increment_retry(task.id)?;
                actions.push(AutonomyAction::RetryUnresolved {
                    id: task.id,
                    task: task.task,
                    retry_count: next_retry,
                    max_retries: task.max_retries,
                });
            }
        }

        Ok(actions)
    }

    pub fn enqueue_task(&self, description: &str, source: &str) -> Result<i64> {
        let connection = self.lock_connection()?;
        connection
            .execute(
                "INSERT INTO task_queue (
                    description, source, status, created_at, requires_approval
                ) VALUES (?1, ?2, 'pending', ?3, ?4)",
                params![
                    description,
                    source,
                    Utc::now().to_rfc3339(),
                    i64::from(self.config.require_approval),
                ],
            )
            .map_err(|error| ForjaError::Storage(error.to_string()))?;
        Ok(connection.last_insert_rowid())
    }

    pub fn enqueue_from_debate(&self, debate_result: &DebateResult) -> Result<Vec<i64>> {
        let mut ids = Vec::new();
        for task in &debate_result.task_list {
            ids.push(self.enqueue_task(&task.name, "debate")?);
        }
        Ok(ids)
    }

    pub fn get_pending_tasks(&self) -> Result<Vec<QueuedTask>> {
        let connection = self.lock_connection()?;
        let mut statement = connection
            .prepare(
                "SELECT id, description, source, status, created_at, started_at, completed_at, result, requires_approval
                 FROM task_queue
                 WHERE status = 'pending'
                 ORDER BY id ASC",
            )
            .map_err(|error| ForjaError::Storage(error.to_string()))?;
        let rows = statement
            .query_map([], map_task_row)
            .map_err(|error| ForjaError::Storage(error.to_string()))?;
        let mut tasks = Vec::new();
        for row in rows {
            tasks.push(row.map_err(|error| ForjaError::Storage(error.to_string()))?);
        }
        Ok(tasks)
    }

    pub fn approve_task(&self, task_id: i64) -> Result<()> {
        let connection = self.lock_connection()?;
        connection
            .execute(
                "UPDATE task_queue SET requires_approval = 0 WHERE id = ?1",
                [task_id],
            )
            .map_err(|error| ForjaError::Storage(error.to_string()))?;
        Ok(())
    }

    pub(crate) fn mark_task_started(&self, task_id: i64) -> Result<()> {
        self.update_task_state(task_id, "running", Some(String::new()), None)
    }

    pub(crate) fn mark_task_completed(&self, task_id: i64, result: String) -> Result<()> {
        self.update_task_state(task_id, "completed", None, Some(result))
    }

    pub(crate) fn mark_task_failed(&self, task_id: i64, error: String) -> Result<()> {
        self.update_task_state(task_id, "failed", None, Some(error))
    }

    pub(crate) fn record_tool_success(&self, tool_name: &str) -> Result<bool> {
        self.skill_registry.record_success(tool_name)?;
        self.skill_registry
            .check_and_promote(tool_name, self.config.skill_threshold)
    }

    pub(crate) fn add_unresolved(&self, task: &str, error: &str) -> Result<()> {
        self.unresolved_store.add(task, error, self.config.max_retries)
    }

    fn update_task_state(
        &self,
        task_id: i64,
        status: &str,
        started_marker: Option<String>,
        result: Option<String>,
    ) -> Result<()> {
        let connection = self.lock_connection()?;
        let now = Utc::now().to_rfc3339();

        match status {
            "running" => {
                let _ = started_marker;
                connection
                    .execute(
                        "UPDATE task_queue
                         SET status = 'running', started_at = ?1
                         WHERE id = ?2",
                        params![now, task_id],
                    )
                    .map_err(|error| ForjaError::Storage(error.to_string()))?;
            }
            "completed" => {
                connection
                    .execute(
                        "UPDATE task_queue
                         SET status = 'completed', completed_at = ?1, result = ?2
                         WHERE id = ?3",
                        params![now, result, task_id],
                    )
                    .map_err(|error| ForjaError::Storage(error.to_string()))?;
            }
            _ => {
                connection
                    .execute(
                        "UPDATE task_queue
                         SET status = 'failed', completed_at = ?1, result = ?2
                         WHERE id = ?3",
                        params![now, result, task_id],
                    )
                    .map_err(|error| ForjaError::Storage(error.to_string()))?;
            }
        }

        Ok(())
    }

    fn lock_connection(&self) -> Result<std::sync::MutexGuard<'_, Connection>> {
        self.db
            .lock()
            .map_err(|error| ForjaError::Storage(error.to_string()))
    }
}

struct ParsedTask {
    tool_name: Option<String>,
    args: Value,
    auto_approved: bool,
}

fn parse_task_description(description: &str) -> ParsedTask {
    let trimmed = description.trim();
    let Some((tool_name, args_text)) = trimmed.split_once(' ') else {
        return ParsedTask {
            tool_name: None,
            args: Value::Null,
            auto_approved: false,
        };
    };

    let args = serde_json::from_str(args_text).unwrap_or(Value::Null);
    ParsedTask {
        tool_name: if args.is_null() {
            None
        } else {
            Some(tool_name.to_string())
        },
        args,
        auto_approved: false,
    }
}

fn map_task_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<QueuedTask> {
    Ok(QueuedTask {
        id: row.get(0)?,
        description: row.get(1)?,
        source: row.get(2)?,
        status: row.get(3)?,
        created_at: row.get(4)?,
        started_at: row.get(5)?,
        completed_at: row.get(6)?,
        result: row.get(7)?,
        requires_approval: row.get::<_, i64>(8)? != 0,
    })
}
