use super::Engine;
use crate::budget::manager::BudgetManager;
use crate::budget::{BudgetMode, BudgetStatus};
use crate::error::{ForjaError, Result};
use std::sync::Arc;

impl Engine {
    pub fn with_budget_mode(mut self, budget_mode: BudgetMode) -> Self {
        self.budget_mode = budget_mode;
        self
    }

    pub fn with_budget_manager(mut self, budget_manager: Arc<BudgetManager>) -> Self {
        self.budget_manager = Some(budget_manager);
        self
    }

    pub fn with_agent_id(mut self, agent_id: String) -> Self {
        self.current_agent_id = agent_id;
        self
    }

    pub(crate) fn check_current_agent_budget(&self) -> Result<()> {
        let Some(budget_manager) = &self.budget_manager else {
            return Ok(());
        };

        let status = budget_manager.check_budget(&self.current_agent_id)?;
        match status {
            BudgetStatus::Ok { .. } => Ok(()),
            BudgetStatus::Warning { used, limit } => {
                eprintln!("[Budget] {} at 80%", self.current_agent_id);
                self.log_budget_event("budget_warning", used, limit);
                Ok(())
            }
            BudgetStatus::Exceeded { used, limit } => {
                self.log_budget_event("budget_exceeded", used, limit);
                match self.budget_mode {
                    BudgetMode::Monitor => Ok(()),
                    BudgetMode::Enforce => Err(ForjaError::LlmError(format!(
                        "Agent budget exceeded for {}",
                        self.current_agent_id
                    ))),
                }
            }
        }
    }

    pub(super) fn record_current_agent_usage(&self, tokens: usize) -> Result<()> {
        let Some(budget_manager) = &self.budget_manager else {
            return Ok(());
        };

        let status = budget_manager.record_usage(&self.current_agent_id, tokens)?;
        let event_type = match status {
            BudgetStatus::Ok { .. } => "budget_usage",
            BudgetStatus::Warning { .. } => "budget_warning",
            BudgetStatus::Exceeded { .. } => "budget_exceeded",
        };
        let (used, limit) = status.usage_tuple();
        self.log_budget_event(event_type, used, limit);
        Ok(())
    }
}
