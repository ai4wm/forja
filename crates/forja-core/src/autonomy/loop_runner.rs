use super::skills::SkillRegistry;
use super::task_store::TaskStore;
use super::unresolved::UnresolvedStore;
use super::{AutonomyAction, AutonomyConfig, AutonomyStatusSummary, QueuedTask};
use crate::creation::DebateResult;
use crate::error::{ForjaError, Result};
use chrono::{DateTime, Duration, Utc};
use rusqlite::{params, Connection};
use std::path::{Path, PathBuf};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};

#[derive(Clone)]
pub struct AutonomousLoop {
    pub config: AutonomyConfig,
    pub skill_registry: SkillRegistry,
    pub unresolved_store: UnresolvedStore,
    pub db_path: PathBuf,
    db: Arc<Mutex<Connection>>,
    task_store: TaskStore,
    active: Arc<AtomicBool>,
    stop_requested: Arc<AtomicBool>,
    empty_notified: Arc<AtomicBool>,
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
                    requires_approval INTEGER DEFAULT 0
                )",
                [],
            )
            .map_err(|error| ForjaError::Storage(error.to_string()))?;

        let task_store = TaskStore::new(&db_path)?;
        let resume_required = task_store.repair_state()?;

        Ok(Self {
            skill_registry: SkillRegistry::new(&db_path)?,
            unresolved_store: UnresolvedStore::new(&db_path)?,
            config,
            db_path,
            db: Arc::new(Mutex::new(connection)),
            task_store,
            active: Arc::new(AtomicBool::new(resume_required)),
            stop_requested: Arc::new(AtomicBool::new(false)),
            empty_notified: Arc::new(AtomicBool::new(false)),
        })
    }

    pub fn start(&self) -> Result<()> {
        self.active.store(true, Ordering::SeqCst);
        self.stop_requested.store(false, Ordering::SeqCst);
        self.empty_notified.store(false, Ordering::SeqCst);
        Ok(())
    }

    pub fn request_stop(&self) -> Result<()> {
        self.stop_requested.store(true, Ordering::SeqCst);
        Ok(())
    }

    pub fn is_active(&self) -> bool {
        self.active.load(Ordering::SeqCst)
    }

    pub fn stop_requested(&self) -> bool {
        self.stop_requested.load(Ordering::SeqCst)
    }

    pub fn status_summary(&self) -> String {
        let summary = self.status_snapshot().unwrap_or(AutonomyStatusSummary {
            active: self.is_active(),
            stop_requested: self.stop_requested(),
            queue_len: 0,
            current_task: None,
        });
        format!(
            "active={} stop_requested={} running={} queue_len={}",
            summary.active,
            summary.stop_requested,
            summary.current_task.is_some(),
            summary.queue_len
        )
    }

    pub fn status_snapshot(&self) -> Result<AutonomyStatusSummary> {
        let queue = self.task_store.load_queue()?;
        let current_task = self.task_store.load_current()?.map(|current| current.task);
        Ok(AutonomyStatusSummary {
            active: self.is_active(),
            stop_requested: self.stop_requested(),
            queue_len: queue.tasks.len(),
            current_task,
        })
    }

    pub fn tick(&self) -> Result<Vec<AutonomyAction>> {
        if !self.config.enabled || !self.is_active() {
            return Ok(Vec::new());
        }

        if self.stop_requested() && self.current_task()?.is_none() {
            self.active.store(false, Ordering::SeqCst);
            return Ok(vec![AutonomyAction::QueueEmpty]);
        }

        if self.current_task()?.is_some() {
            return Ok(Vec::new());
        }

        let mut pending = self.get_pending_tasks()?;
        if pending.is_empty() {
            if self.empty_notified.swap(true, Ordering::SeqCst) {
                return Ok(Vec::new());
            }
            return Ok(vec![AutonomyAction::QueueEmpty]);
        }
        self.empty_notified.store(false, Ordering::SeqCst);

        Ok(vec![AutonomyAction::ExecuteTask {
            task: Box::new(pending.remove(0)),
        }])
    }

    pub fn enqueue_task(&self, description: &str, source: &str) -> Result<i64> {
        let mut queue = self.task_store.load_queue()?;
        let next_id = queue
            .tasks
            .iter()
            .map(|task| task.id)
            .max()
            .unwrap_or(0)
            .saturating_add(1);
        let task = QueuedTask {
            id: next_id,
            description: description.to_string(),
            source: source.to_string(),
            status: "pending".to_string(),
            created_at: Utc::now().to_rfc3339(),
            started_at: None,
            completed_at: None,
            result: None,
            requires_approval: false,
            retry_count: 0,
            next_attempt_at: None,
            task_ref: extract_task_reference(description),
            cancel_requested: false,
        };
        queue.tasks.push(task.clone());
        self.task_store.save_queue(&queue)?;
        self.upsert_task_row(&task)?;
        self.empty_notified.store(false, Ordering::SeqCst);
        Ok(next_id)
    }

    pub fn enqueue_from_debate(&self, debate_result: &DebateResult) -> Result<Vec<i64>> {
        let mut ids = Vec::new();
        for task in &debate_result.task_list {
            ids.push(self.enqueue_task(&task.name, "debate")?);
        }
        Ok(ids)
    }

    pub fn list_tasks(&self) -> Result<Vec<QueuedTask>> {
        Ok(self.task_store.load_queue()?.tasks)
    }

    pub fn get_pending_tasks(&self) -> Result<Vec<QueuedTask>> {
        let now = Utc::now();
        Ok(self
            .task_store
            .load_queue()?
            .tasks
            .into_iter()
            .filter(|task| task.status == "pending")
            .filter(|task| !task.cancel_requested)
            .filter(|task| {
                task.next_attempt_at
                    .as_deref()
                    .and_then(parse_rfc3339)
                    .is_none_or(|time| time <= now)
            })
            .collect())
    }

    pub fn cancel_task(&self, task_id: i64) -> Result<()> {
        let mut queue = self.task_store.load_queue()?;
        let now = Utc::now().to_rfc3339();
        if let Some(task) = queue.tasks.iter_mut().find(|task| task.id == task_id) {
            task.cancel_requested = true;
            task.status = "failed".to_string();
            task.completed_at = Some(now);
            task.result = Some("cancelled".to_string());
            self.upsert_task_row(task)?;
        }
        self.task_store.save_queue(&queue)?;

        if self.current_task()?.is_some_and(|task| task.id == task_id) {
            self.task_store.clear_current()?;
        }

        Ok(())
    }

    pub fn current_task(&self) -> Result<Option<QueuedTask>> {
        Ok(self.task_store.load_current()?.map(|current| current.task))
    }

    pub fn notification_log_path(&self) -> &Path {
        self.task_store.log_path()
    }

    pub fn append_notification_log(&self, line: &str) -> Result<()> {
        self.task_store.append_log(line)
    }

    pub(crate) fn mark_task_started(&self, task_id: i64) -> Result<QueuedTask> {
        let mut queue = self.task_store.load_queue()?;
        let now = Utc::now().to_rfc3339();
        let task = queue
            .tasks
            .iter_mut()
            .find(|task| task.id == task_id)
            .ok_or_else(|| ForjaError::Storage(format!("Task #{task_id} not found")))?;
        task.status = "running".to_string();
        task.started_at = Some(now);
        task.completed_at = None;
        task.result = None;
        task.next_attempt_at = None;
        let snapshot = task.clone();
        self.task_store.save_queue(&queue)?;
        self.task_store.save_current(&snapshot)?;
        self.upsert_task_row(&snapshot)?;
        Ok(snapshot)
    }

    pub(crate) fn mark_task_completed(&self, task_id: i64, result: String) -> Result<QueuedTask> {
        let mut queue = self.task_store.load_queue()?;
        let now = Utc::now().to_rfc3339();
        let task = queue
            .tasks
            .iter_mut()
            .find(|task| task.id == task_id)
            .ok_or_else(|| ForjaError::Storage(format!("Task #{task_id} not found")))?;
        task.status = "completed".to_string();
        task.completed_at = Some(now);
        task.result = Some(result);
        let snapshot = task.clone();
        self.task_store.save_queue(&queue)?;
        self.task_store.clear_current()?;
        self.upsert_task_row(&snapshot)?;
        Ok(snapshot)
    }

    pub(crate) fn record_task_failure(
        &self,
        task_id: i64,
        error: &str,
    ) -> Result<QueuedTask> {
        let mut queue = self.task_store.load_queue()?;
        let now = Utc::now();
        let task = queue
            .tasks
            .iter_mut()
            .find(|task| task.id == task_id)
            .ok_or_else(|| ForjaError::Storage(format!("Task #{task_id} not found")))?;
        task.retry_count = task.retry_count.saturating_add(1);
        if task.retry_count >= self.config.max_retries {
            task.status = "failed".to_string();
            task.completed_at = Some(now.to_rfc3339());
            task.result = Some(error.to_string());
            task.next_attempt_at = None;
        } else {
            task.status = "pending".to_string();
            task.completed_at = None;
            task.result = Some(error.to_string());
            task.next_attempt_at = Some((now + retry_backoff(task.retry_count)).to_rfc3339());
        }
        let snapshot = task.clone();
        self.task_store.save_queue(&queue)?;
        self.task_store.clear_current()?;
        self.upsert_task_row(&snapshot)?;
        Ok(snapshot)
    }

    pub(crate) fn add_unresolved(&self, task: &str, error: &str) -> Result<()> {
        self.unresolved_store.add(task, error, self.config.max_retries)
    }

    fn upsert_task_row(&self, task: &QueuedTask) -> Result<()> {
        let connection = self.lock_connection()?;
        connection
            .execute(
                "INSERT INTO task_queue (
                    id, description, source, status, created_at, started_at, completed_at, result, requires_approval
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
                 ON CONFLICT(id) DO UPDATE SET
                    description=excluded.description,
                    source=excluded.source,
                    status=excluded.status,
                    created_at=excluded.created_at,
                    started_at=excluded.started_at,
                    completed_at=excluded.completed_at,
                    result=excluded.result,
                    requires_approval=excluded.requires_approval",
                params![
                    task.id,
                    task.description,
                    task.source,
                    task.status,
                    task.created_at,
                    task.started_at,
                    task.completed_at,
                    task.result,
                    i64::from(task.requires_approval),
                ],
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

fn retry_backoff(retry_count: u32) -> Duration {
    let secs = match retry_count {
        1 => 1,
        2 => 2,
        _ => 4,
    };
    Duration::seconds(secs)
}

fn parse_rfc3339(value: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|value| value.with_timezone(&Utc))
}

fn extract_task_reference(description: &str) -> Option<String> {
    let trimmed = description.trim();
    if trimmed.starts_with("SPEC-") {
        return Some(trimmed.to_string());
    }

    let path = Path::new(trimmed);
    if path.extension().and_then(|ext| ext.to_str()) == Some("md") && path.exists() {
        return Some(trimmed.to_string());
    }

    None
}
