pub mod loop_runner;
pub mod skills;
pub mod unresolved;

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

#[derive(Debug, Clone, PartialEq, Eq)]
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
}

#[derive(Debug, Clone, PartialEq)]
pub enum AutonomyAction {
    AwaitingApproval {
        task_id: i64,
        description: String,
        source: String,
    },
    ExecuteTask {
        task_id: i64,
        description: String,
        source: String,
        tool_name: String,
        args: serde_json::Value,
    },
    RetryUnresolved {
        id: i64,
        task: String,
        retry_count: u32,
        max_retries: u32,
    },
    FailedUnresolved {
        id: i64,
        task: String,
    },
}

#[cfg(test)]
mod tests;
