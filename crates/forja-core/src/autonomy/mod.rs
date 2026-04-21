pub mod loop_runner;
pub mod skills;
pub mod task_store;
pub mod unresolved;

use crate::traits::LlmProvider;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

pub type CloudEscalationConfirmer = Arc<dyn Fn(&str) -> bool + Send + Sync>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AutonomyConfig {
    pub enabled: bool,
    pub task_check_interval_secs: u64,
    pub skill_threshold: u32,
    pub max_retries: u32,
    pub require_approval: bool,
}

impl Default for AutonomyConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            task_check_interval_secs: 300,
            skill_threshold: 5,
            max_retries: 3,
            require_approval: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Skill {
    pub tool_name: String,
    pub success_count: u32,
    pub last_used: Option<String>,
    pub auto_approved: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnresolvedTask {
    pub id: i64,
    pub task: String,
    pub error: Option<String>,
    pub retry_count: u32,
    pub max_retries: u32,
    pub created_at: String,
    pub last_tried: Option<String>,
    pub status: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QueuedTask {
    pub id: i64,
    pub description: String,
    pub source: String,
    pub status: String,
    pub created_at: String,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
    pub result: Option<String>,
    pub requires_approval: bool,
    pub retry_count: u32,
    pub next_attempt_at: Option<String>,
    pub task_ref: Option<String>,
    pub cancel_requested: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskQueueFile {
    pub tasks: Vec<QueuedTask>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CurrentTaskFile {
    pub task: QueuedTask,
    pub checkpointed_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AutonomyStatusSummary {
    pub active: bool,
    pub stop_requested: bool,
    pub queue_len: usize,
    pub current_task: Option<QueuedTask>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum AutonomyAction {
    ExecuteTask { task: Box<QueuedTask> },
    QueueEmpty,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AutonomyTarget {
    pub provider: String,
    pub model: String,
    pub label: String,
    pub local: bool,
}

#[derive(Clone)]
pub struct AutonomyExecutionRuntime {
    pub local_monitor: Option<Arc<dyn LlmProvider>>,
    pub local_target: Option<AutonomyTarget>,
    pub cloud_target: Option<AutonomyTarget>,
    pub cloud_escalation_requires_confirmation: bool,
    pub cloud_escalation_confirmer: Option<CloudEscalationConfirmer>,
}

#[cfg(test)]
mod tests;
