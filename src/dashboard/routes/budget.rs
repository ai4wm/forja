use super::{DashboardError, DashboardState, open_read_only, table_exists};
use axum::Json;
use axum::extract::State;
use serde::Serialize;

#[derive(Debug, Serialize)]
pub(crate) struct BudgetRow {
    agent_id: String,
    monthly_limit: usize,
    used_tokens: usize,
    month_key: String,
    warning_emitted: bool,
    percent: usize,
}

pub(crate) async fn get_budget(
    State(state): State<DashboardState>,
) -> Result<Json<Vec<BudgetRow>>, DashboardError> {
    let connection = open_read_only(&state.db_path)?;
    if !table_exists(&connection, "agent_budgets")? {
        return Ok(Json(Vec::new()));
    }

    let mut statement = connection.prepare(
        "SELECT agent_id, monthly_limit, used_tokens, month_key, warning_emitted
         FROM agent_budgets
         ORDER BY agent_id ASC",
    )?;
    let rows = statement.query_map([], |row| {
        let monthly_limit = row.get::<_, i64>(1)? as usize;
        let used_tokens = row.get::<_, i64>(2)? as usize;
        Ok(BudgetRow {
            agent_id: row.get(0)?,
            monthly_limit,
            used_tokens,
            month_key: row.get(3)?,
            warning_emitted: row.get::<_, i64>(4)? != 0,
            percent: if monthly_limit == 0 {
                0
            } else {
                used_tokens.saturating_mul(100) / monthly_limit
            },
        })
    })?;

    let mut budgets = Vec::new();
    for row in rows {
        budgets.push(row?);
    }
    Ok(Json(budgets))
}
