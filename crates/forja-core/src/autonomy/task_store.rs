use super::{CurrentTaskFile, QueuedTask, TaskQueueFile};
use crate::error::{ForjaError, Result};
use chrono::Utc;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Clone)]
pub struct TaskStore {
    tasks_dir: PathBuf,
    queue_file: PathBuf,
    current_file: PathBuf,
    log_file: PathBuf,
}

impl TaskStore {
    pub fn new(base_path: impl AsRef<Path>) -> Result<Self> {
        let base_path = base_path.as_ref();
        let base_dir = if base_path.extension().is_some() {
            let parent = base_path
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or_else(|| PathBuf::from("."));
            let is_runtime_audit_db = base_path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.eq_ignore_ascii_case("audit.db"));

            if is_runtime_audit_db {
                parent
            } else {
                parent.join(
                    base_path
                        .file_stem()
                        .and_then(|stem| stem.to_str())
                        .unwrap_or("tasks"),
                )
            }
        } else {
            base_path.to_path_buf()
        };
        let tasks_dir = base_dir.join("tasks");
        fs::create_dir_all(&tasks_dir).map_err(io_error)?;
        let queue_file = tasks_dir.join("queue.json");
        let current_file = tasks_dir.join("current.json");
        let log_file = tasks_dir.join("autonomy.log");

        let store = Self {
            tasks_dir,
            queue_file,
            current_file,
            log_file,
        };
        store.ensure_queue_file()?;
        Ok(store)
    }

    pub fn queue_path(&self) -> &Path {
        &self.queue_file
    }

    pub fn current_path(&self) -> &Path {
        &self.current_file
    }

    pub fn log_path(&self) -> &Path {
        &self.log_file
    }

    pub fn load_queue(&self) -> Result<TaskQueueFile> {
        self.ensure_queue_file()?;
        let raw = fs::read_to_string(&self.queue_file).map_err(io_error)?;
        serde_json::from_str(&raw).map_err(|error| ForjaError::Storage(error.to_string()))
    }

    pub fn save_queue(&self, queue: &TaskQueueFile) -> Result<()> {
        let raw = serde_json::to_string_pretty(queue)
            .map_err(|error| ForjaError::Storage(error.to_string()))?;
        fs::write(&self.queue_file, raw).map_err(io_error)
    }

    pub fn load_current(&self) -> Result<Option<CurrentTaskFile>> {
        if !self.current_file.exists() {
            return Ok(None);
        }
        let raw = fs::read_to_string(&self.current_file).map_err(io_error)?;
        let current =
            serde_json::from_str(&raw).map_err(|error| ForjaError::Storage(error.to_string()))?;
        Ok(Some(current))
    }

    pub fn save_current(&self, task: &QueuedTask) -> Result<()> {
        let current = CurrentTaskFile {
            task: task.clone(),
            checkpointed_at: Utc::now().to_rfc3339(),
        };
        let raw = serde_json::to_string_pretty(&current)
            .map_err(|error| ForjaError::Storage(error.to_string()))?;
        fs::write(&self.current_file, raw).map_err(io_error)
    }

    pub fn clear_current(&self) -> Result<()> {
        if self.current_file.exists() {
            fs::remove_file(&self.current_file).map_err(io_error)?;
        }
        Ok(())
    }

    pub fn append_log(&self, line: &str) -> Result<()> {
        let prefix = if self.log_file.exists() { "\n" } else { "" };
        let contents = format!("{prefix}{line}");
        let mut options = fs::OpenOptions::new();
        options.create(true).append(true);
        use std::io::Write;
        let mut file = options.open(&self.log_file).map_err(io_error)?;
        file.write_all(contents.as_bytes()).map_err(io_error)
    }

    pub fn repair_state(&self) -> Result<bool> {
        let mut queue = self.load_queue()?;
        let current = self.load_current()?;
        let mut repaired = false;
        if let Some(current) = current {
            repaired = true;
            if let Some(task) = queue
                .tasks
                .iter_mut()
                .find(|task| task.id == current.task.id)
            {
                if task.status == "running" {
                    task.status = "pending".to_string();
                }
            } else {
                let mut task = current.task;
                task.status = "pending".to_string();
                queue.tasks.insert(0, task);
            }
            self.save_queue(&queue)?;
            self.clear_current()?;
        }
        Ok(repaired)
    }

    fn ensure_queue_file(&self) -> Result<()> {
        if self.queue_file.exists() {
            return Ok(());
        }
        self.save_queue(&TaskQueueFile { tasks: Vec::new() })
    }

    pub fn tasks_dir(&self) -> &Path {
        &self.tasks_dir
    }
}

fn io_error(error: impl ToString) -> ForjaError {
    ForjaError::Storage(error.to_string())
}
