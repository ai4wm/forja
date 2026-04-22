use super::{DashboardError, DashboardState, open_read_only, table_exists};
use axum::Json;
use axum::extract::{Path, State};
use forja_core::traits::TelegramConnectionStatus;
use rusqlite::Connection;
use serde::Serialize;
use serde_json::{Value, json};

#[derive(Debug, Serialize)]
pub(crate) struct SkillRow {
    tool_name: String,
    success_count: usize,
    last_used: Option<String>,
    auto_approved: bool,
}

#[derive(Debug, Serialize)]
pub(crate) struct UnresolvedRow {
    id: i64,
    task: String,
    error: Option<String>,
    retry_count: usize,
    max_retries: usize,
    created_at: String,
    last_tried: Option<String>,
    status: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct TaskQueueRow {
    id: i64,
    description: String,
    source: String,
    status: String,
    created_at: String,
    started_at: Option<String>,
    completed_at: Option<String>,
    result: Option<String>,
    requires_approval: bool,
}

#[derive(Debug, Serialize)]
pub(crate) struct ChannelStatusRow {
    telegram: &'static str,
}

pub(crate) async fn get_skills(
    State(state): State<DashboardState>,
) -> Result<Json<Vec<SkillRow>>, DashboardError> {
    let connection = open_read_only(&state.db_path)?;
    if !table_exists(&connection, "skills")? {
        return Ok(Json(Vec::new()));
    }

    let mut statement = connection.prepare(
        "SELECT tool_name, success_count, last_used, auto_approved
         FROM skills
         ORDER BY tool_name ASC",
    )?;
    let rows = statement.query_map([], |row| {
        Ok(SkillRow {
            tool_name: row.get(0)?,
            success_count: row.get::<_, i64>(1)? as usize,
            last_used: row.get(2)?,
            auto_approved: row.get::<_, i64>(3)? != 0,
        })
    })?;

    Ok(Json(collect_rows(rows)?))
}

pub(crate) async fn get_unresolved(
    State(state): State<DashboardState>,
) -> Result<Json<Vec<UnresolvedRow>>, DashboardError> {
    let connection = open_read_only(&state.db_path)?;
    if !table_exists(&connection, "unresolved")? {
        return Ok(Json(Vec::new()));
    }

    let mut statement = connection.prepare(
        "SELECT id, task, error, retry_count, max_retries, created_at, last_tried, status
         FROM unresolved
         ORDER BY id ASC",
    )?;
    let rows = statement.query_map([], |row| {
        Ok(UnresolvedRow {
            id: row.get(0)?,
            task: row.get(1)?,
            error: row.get(2)?,
            retry_count: row.get::<_, i64>(3)? as usize,
            max_retries: row.get::<_, i64>(4)? as usize,
            created_at: row.get(5)?,
            last_tried: row.get(6)?,
            status: row.get(7)?,
        })
    })?;

    Ok(Json(collect_rows(rows)?))
}

pub(crate) async fn get_tasks(
    State(state): State<DashboardState>,
) -> Result<Json<Vec<TaskQueueRow>>, DashboardError> {
    let connection = open_read_only(&state.db_path)?;
    if !table_exists(&connection, "task_queue")? {
        return Ok(Json(Vec::new()));
    }

    let mut statement = connection.prepare(
        "SELECT id, description, source, status, created_at, started_at, completed_at, result, requires_approval
         FROM task_queue
         ORDER BY id ASC",
    )?;
    let rows = statement.query_map([], |row| {
        Ok(TaskQueueRow {
            id: row.get(0)?,
            description: row.get(1)?,
            source: row.get(2)?,
            status: row.get(3)?,
            created_at: row.get(4)?,
            started_at: row.get(5)?,
            completed_at: row.get(6)?,
            result: row.get(7)?,
            requires_approval: row.get::<_, i64>(8)? != 0,
        })
    })?;

    Ok(Json(collect_rows(rows)?))
}

pub(crate) async fn approve_task(
    State(state): State<DashboardState>,
    Path(id): Path<i64>,
) -> Result<Json<Value>, DashboardError> {
    let connection = Connection::open(&state.db_path).map_err(DashboardError::Db)?;
    if !table_exists(&connection, "task_queue")? {
        return Ok(Json(json!({
            "ok": false,
            "id": id,
            "reason": "task_queue unavailable",
        })));
    }

    connection.execute(
        "UPDATE task_queue SET requires_approval = 0 WHERE id = ?1",
        [id],
    )?;
    Ok(Json(json!({ "ok": true, "id": id })))
}

pub(crate) async fn get_channel_status(
    State(state): State<DashboardState>,
) -> Result<Json<ChannelStatusRow>, DashboardError> {
    let telegram = match (state.telegram_status)() {
        TelegramConnectionStatus::Connected => "Connected",
        TelegramConnectionStatus::Disconnected => "Disconnected",
        TelegramConnectionStatus::Reconnecting => "Reconnecting",
    };

    Ok(Json(ChannelStatusRow { telegram }))
}

fn collect_rows<T>(
    rows: rusqlite::MappedRows<'_, impl FnMut(&rusqlite::Row<'_>) -> rusqlite::Result<T>>,
) -> Result<Vec<T>, DashboardError> {
    let mut collected = Vec::new();
    for row in rows {
        collected.push(row?);
    }
    Ok(collected)
}
