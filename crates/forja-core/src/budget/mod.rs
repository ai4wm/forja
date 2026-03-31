pub mod manager;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentBudget {
    pub agent_id: String,
    pub monthly_limit: usize,
    pub used_tokens: usize,
    pub month_key: String,
    pub warning_emitted: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BudgetStatus {
    Ok { used: usize, limit: usize },
    Warning { used: usize, limit: usize },
    Exceeded { used: usize, limit: usize },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BudgetMode {
    #[default]
    Monitor,
    Enforce,
}

impl BudgetStatus {
    pub fn usage_tuple(&self) -> (usize, usize) {
        match *self {
            Self::Ok { used, limit }
            | Self::Warning { used, limit }
            | Self::Exceeded { used, limit } => (used, limit),
        }
    }
}

#[cfg(test)]
mod tests;
