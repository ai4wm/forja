pub mod agents;
pub mod combination;
pub mod debate;
pub mod execution;
pub mod mutation;
pub mod types;

use crate::budget::manager::BudgetManager;
use crate::budget::BudgetMode;
use crate::ralf::RalfConfig;
use std::sync::Arc;
use std::time::Duration;

pub use types::{DebateMessage, DebatePhase, DebateResult, DebateRunMetadata, TaskItem};

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
    pub combination_rounds: usize,
    pub mutation_rounds: usize,
    pub converge_rounds: usize,
    pub min_agents: usize,
    pub max_agents: usize,
    pub auto_team_sizing: bool,
}

impl Default for DebateConfig {
    fn default() -> Self {
        Self {
            diverge_rounds: 2,
            conflict_rounds: 3,
            combination_rounds: 1,
            mutation_rounds: 1,
            converge_rounds: 1,
            min_agents: 3,
            max_agents: 5,
            auto_team_sizing: true,
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

#[derive(Clone)]
pub struct CreationRunContext {
    pub ralf_config: RalfConfig,
    pub budget_manager: Option<Arc<BudgetManager>>,
    pub budget_mode: BudgetMode,
    pub max_prompt_context_chars: usize,
    pub max_logged_chars: usize,
    pub inter_call_delay: Duration,
}

impl Default for CreationRunContext {
    fn default() -> Self {
        Self {
            ralf_config: RalfConfig::default(),
            budget_manager: None,
            budget_mode: BudgetMode::Monitor,
            max_prompt_context_chars: 2_000,
            max_logged_chars: 512,
            inter_call_delay: Duration::from_secs(2),
        }
    }
}

#[cfg(test)]
mod tests;
#[cfg(test)]
mod expanded_tests;
#[cfg(test)]
mod policy_tests;
