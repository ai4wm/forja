use super::{AgentBudget, BudgetStatus};
use crate::error::{ForjaError, Result};
use chrono::Utc;
use rusqlite::{Connection, params};
use std::path::Path;
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub struct BudgetManager {
    db: Arc<Mutex<Connection>>,
}

impl BudgetManager {
    pub fn new(db_path: &Path) -> Result<Self> {
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|error| ForjaError::Storage(error.to_string()))?;
        }

        let connection =
            Connection::open(db_path).map_err(|error| ForjaError::Storage(error.to_string()))?;
        connection
            .execute(
                "CREATE TABLE IF NOT EXISTS agent_budgets (
                    agent_id TEXT PRIMARY KEY,
                    monthly_limit INTEGER NOT NULL,
                    used_tokens INTEGER NOT NULL DEFAULT 0,
                    month_key TEXT NOT NULL,
                    warning_emitted INTEGER NOT NULL DEFAULT 0
                )",
                [],
            )
            .map_err(|error| ForjaError::Storage(error.to_string()))?;

        Ok(Self {
            db: Arc::new(Mutex::new(connection)),
        })
    }

    pub fn register_agent(&self, agent_id: &str, monthly_limit: usize) -> Result<()> {
        let month_key = current_month_key();
        let connection = self.lock_connection()?;
        connection
            .execute(
                "INSERT INTO agent_budgets (
                    agent_id,
                    monthly_limit,
                    used_tokens,
                    month_key,
                    warning_emitted
                ) VALUES (?1, ?2, 0, ?3, 0)
                ON CONFLICT(agent_id) DO UPDATE SET
                    monthly_limit = excluded.monthly_limit",
                params![agent_id, monthly_limit as i64, month_key],
            )
            .map_err(|error| ForjaError::Storage(error.to_string()))?;
        Ok(())
    }

    pub fn record_usage(&self, agent_id: &str, tokens: usize) -> Result<BudgetStatus> {
        self.reset_month_if_needed(agent_id)?;
        let budget = self.load_budget(agent_id)?;
        let used_tokens = budget.used_tokens.saturating_add(tokens);
        let warning_emitted = if budget.monthly_limit == 0 {
            true
        } else {
            used_tokens * 100 / budget.monthly_limit >= 80
        };

        let connection = self.lock_connection()?;
        connection
            .execute(
                "UPDATE agent_budgets
                 SET used_tokens = ?1, warning_emitted = ?2
                 WHERE agent_id = ?3",
                params![used_tokens as i64, i64::from(warning_emitted), agent_id],
            )
            .map_err(|error| ForjaError::Storage(error.to_string()))?;

        Ok(status_from_usage(used_tokens, budget.monthly_limit))
    }

    pub fn check_budget(&self, agent_id: &str) -> Result<BudgetStatus> {
        self.reset_month_if_needed(agent_id)?;
        let budget = self.load_budget(agent_id)?;
        Ok(status_from_usage(budget.used_tokens, budget.monthly_limit))
    }

    pub fn reset_month_if_needed(&self, agent_id: &str) -> Result<()> {
        let budget = self.load_budget(agent_id)?;
        let current_month = current_month_key();
        if budget.month_key == current_month {
            return Ok(());
        }

        let connection = self.lock_connection()?;
        connection
            .execute(
                "UPDATE agent_budgets
                 SET used_tokens = 0, month_key = ?1, warning_emitted = 0
                 WHERE agent_id = ?2",
                params![current_month, agent_id],
            )
            .map_err(|error| ForjaError::Storage(error.to_string()))?;
        Ok(())
    }

    fn load_budget(&self, agent_id: &str) -> Result<AgentBudget> {
        let connection = self.lock_connection()?;
        let mut statement = connection
            .prepare(
                "SELECT agent_id, monthly_limit, used_tokens, month_key, warning_emitted
                 FROM agent_budgets
                 WHERE agent_id = ?1",
            )
            .map_err(|error| ForjaError::Storage(error.to_string()))?;

        statement
            .query_row([agent_id], |row| {
                Ok(AgentBudget {
                    agent_id: row.get(0)?,
                    monthly_limit: row.get::<_, i64>(1)? as usize,
                    used_tokens: row.get::<_, i64>(2)? as usize,
                    month_key: row.get(3)?,
                    warning_emitted: row.get::<_, i64>(4)? != 0,
                })
            })
            .map_err(|error| {
                if matches!(error, rusqlite::Error::QueryReturnedNoRows) {
                    ForjaError::Internal(format!("budget not found for agent {agent_id}"))
                } else {
                    ForjaError::Storage(error.to_string())
                }
            })
    }

    fn lock_connection(&self) -> Result<std::sync::MutexGuard<'_, Connection>> {
        self.db
            .lock()
            .map_err(|error| ForjaError::Storage(error.to_string()))
    }
}

fn status_from_usage(used: usize, limit: usize) -> BudgetStatus {
    if limit == 0 || used >= limit {
        BudgetStatus::Exceeded { used, limit }
    } else if used * 100 / limit >= 80 {
        BudgetStatus::Warning { used, limit }
    } else {
        BudgetStatus::Ok { used, limit }
    }
}

fn current_month_key() -> String {
    Utc::now().format("%Y-%m").to_string()
}
