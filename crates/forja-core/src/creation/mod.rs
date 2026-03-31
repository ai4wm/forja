pub mod agents;
pub mod debate;
pub mod types;

pub use types::{DebateMessage, DebatePhase, DebateResult, TaskItem};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DebateAgent {
    pub id: String,
    pub role: String,
    pub framework: String,
    pub budget: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DebateConfig {
    pub diverge_rounds: usize,
    pub conflict_rounds: usize,
    pub converge_rounds: usize,
    pub max_agents: usize,
}

impl Default for DebateConfig {
    fn default() -> Self {
        Self {
            diverge_rounds: 2,
            conflict_rounds: 3,
            converge_rounds: 1,
            max_agents: 5,
        }
    }
}

#[derive(Debug, Clone)]
pub struct DebateEngine {
    pub agents: Vec<DebateAgent>,
    pub config: DebateConfig,
}

impl DebateEngine {
    pub fn new(agents: Vec<DebateAgent>, config: DebateConfig) -> Self {
        Self { agents, config }
    }
}

#[cfg(test)]
mod tests;
