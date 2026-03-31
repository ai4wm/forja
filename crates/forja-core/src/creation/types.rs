#[derive(Debug, Clone, PartialEq)]
pub struct TaskItem {
    pub name: String,
    pub assigned_role: String,
    pub estimated_hours: f32,
    pub priority: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DebatePhase {
    Diverge,
    Conflict,
    Converge,
}

impl DebatePhase {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Diverge => "Diverge",
            Self::Conflict => "Conflict",
            Self::Converge => "Converge",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct DebateMessage {
    pub agent_id: String,
    pub role: String,
    pub phase: DebatePhase,
    pub round: usize,
    pub content: String,
    pub tokens: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DebateResult {
    pub summary: String,
    pub task_list: Vec<TaskItem>,
    pub transcript: Vec<DebateMessage>,
    pub total_tokens: usize,
    pub total_rounds: usize,
}
