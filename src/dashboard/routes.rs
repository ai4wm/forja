use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use forja_core::traits::TelegramConnectionStatus;
use rusqlite::{params, Connection, OpenFlags};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::Arc;
use tower_http::cors::{Any, CorsLayer};

const INDEX_HTML: &str = include_str!("static/index.html");

pub(crate) type TelegramStatusProvider =
    Arc<dyn Fn() -> TelegramConnectionStatus + Send + Sync>;

#[derive(Clone)]
pub(crate) struct DashboardState {
    db_path: PathBuf,
    telegram_status: TelegramStatusProvider,
}

pub(crate) fn build_router_with_status(
    db_path: PathBuf,
    telegram_status: TelegramStatusProvider,
) -> Router {
    Router::new()
        .route("/", get(index))
        .route("/api/audit", get(get_audit))
        .route("/api/debates", get(get_debates))
        .route("/api/debate/:id", get(get_debate))
        .route("/api/budget", get(get_budget))
        .route("/api/skills", get(get_skills))
        .route("/api/unresolved", get(get_unresolved))
        .route("/api/tasks", get(get_tasks))
        .route("/api/channel-status", get(get_channel_status))
        .route("/api/approve/:id", post(approve_task))
        .with_state(DashboardState {
            db_path,
            telegram_status,
        })
        .layer(CorsLayer::new().allow_origin(Any).allow_methods(Any))
}

pub(crate) fn default_telegram_status_provider() -> TelegramStatusProvider {
    Arc::new(|| TelegramConnectionStatus::Disconnected)
}

async fn index() -> Html<&'static str> {
    Html(INDEX_HTML)
}

#[derive(Debug, Deserialize)]
struct AuditQuery {
    limit: Option<usize>,
    event_type: Option<String>,
}

#[derive(Debug, Serialize)]
struct AuditRow {
    id: i64,
    timestamp: String,
    event_type: String,
    agent_id: String,
    channel: Option<String>,
    payload: Value,
    token_count: usize,
}

async fn get_audit(
    State(state): State<DashboardState>,
    Query(query): Query<AuditQuery>,
) -> Result<Json<Vec<AuditRow>>, DashboardError> {
    let limit = query.limit.unwrap_or(50) as i64;
    let connection = open_read_only(&state.db_path)?;

    let rows = if let Some(event_type) = query.event_type.filter(|value| !value.trim().is_empty()) {
        let mut statement = connection.prepare(
            "SELECT id, timestamp, event_type, agent_id, channel, payload, token_count
             FROM audit_log
             WHERE event_type = ?1
             ORDER BY id DESC
             LIMIT ?2",
        )?;
        let rows = statement.query_map(params![event_type, limit], map_audit_row)?;
        let mut collected = Vec::new();
        for row in rows {
            collected.push(row?);
        }
        collected
    } else {
        let mut statement = connection.prepare(
            "SELECT id, timestamp, event_type, agent_id, channel, payload, token_count
             FROM audit_log
             ORDER BY id DESC
             LIMIT ?1",
        )?;
        let rows = statement.query_map([limit], map_audit_row)?;
        let mut collected = Vec::new();
        for row in rows {
            collected.push(row?);
        }
        collected
    };

    Ok(Json(rows))
}

#[derive(Debug, Serialize)]
struct DebateSummary {
    id: String,
    started_at: String,
    message_count: usize,
    preview: String,
}

#[derive(Debug, Serialize, Clone)]
struct DebateTranscriptItem {
    row_id: i64,
    timestamp: String,
    agent_id: String,
    phase: String,
    round: usize,
    role: String,
    content: String,
    tokens: usize,
}

#[derive(Debug, Clone)]
struct DebateGroup {
    id: String,
    started_at: String,
    transcript: Vec<DebateTranscriptItem>,
}

async fn get_debates(
    State(state): State<DashboardState>,
) -> Result<Json<Vec<DebateSummary>>, DashboardError> {
    let groups = load_debate_groups(&state.db_path)?;
    let summaries = groups
        .into_iter()
        .map(|group| DebateSummary {
            id: group.id,
            started_at: group.started_at,
            message_count: group.transcript.len(),
            preview: group
                .transcript
                .last()
                .map(|item| item.content.clone())
                .unwrap_or_default(),
        })
        .collect();
    Ok(Json(summaries))
}

async fn get_debate(
    State(state): State<DashboardState>,
    Path(id): Path<String>,
) -> Result<Json<Vec<DebateTranscriptItem>>, DashboardError> {
    let groups = load_debate_groups(&state.db_path)?;
    let transcript = groups
        .into_iter()
        .find(|group| group.id == id)
        .map(|group| group.transcript)
        .ok_or_else(|| DashboardError::NotFound(format!("debate {id} not found")))?;
    Ok(Json(transcript))
}

#[derive(Debug, Serialize)]
struct BudgetRow {
    agent_id: String,
    monthly_limit: usize,
    used_tokens: usize,
    month_key: String,
    warning_emitted: bool,
    percent: usize,
}

async fn get_budget(
    State(state): State<DashboardState>,
) -> Result<Json<Vec<BudgetRow>>, DashboardError> {
    let connection = open_read_only(&state.db_path)?;
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

#[derive(Debug, Serialize)]
struct SkillRow {
    tool_name: String,
    success_count: usize,
    last_used: Option<String>,
    auto_approved: bool,
}

async fn get_skills(
    State(state): State<DashboardState>,
) -> Result<Json<Vec<SkillRow>>, DashboardError> {
    let connection = open_read_only(&state.db_path)?;
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

    let mut skills = Vec::new();
    for row in rows {
        skills.push(row?);
    }
    Ok(Json(skills))
}

#[derive(Debug, Serialize)]
struct UnresolvedRow {
    id: i64,
    task: String,
    error: Option<String>,
    retry_count: usize,
    max_retries: usize,
    created_at: String,
    last_tried: Option<String>,
    status: String,
}

async fn get_unresolved(
    State(state): State<DashboardState>,
) -> Result<Json<Vec<UnresolvedRow>>, DashboardError> {
    let connection = open_read_only(&state.db_path)?;
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

    let mut unresolved = Vec::new();
    for row in rows {
        unresolved.push(row?);
    }
    Ok(Json(unresolved))
}

#[derive(Debug, Serialize)]
struct TaskQueueRow {
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

async fn get_tasks(
    State(state): State<DashboardState>,
) -> Result<Json<Vec<TaskQueueRow>>, DashboardError> {
    let connection = open_read_only(&state.db_path)?;
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

    let mut tasks = Vec::new();
    for row in rows {
        tasks.push(row?);
    }
    Ok(Json(tasks))
}

async fn approve_task(
    State(state): State<DashboardState>,
    Path(id): Path<i64>,
) -> Result<Json<Value>, DashboardError> {
    let connection = Connection::open(&state.db_path).map_err(DashboardError::Db)?;
    connection.execute(
        "UPDATE task_queue SET requires_approval = 0 WHERE id = ?1",
        [id],
    )?;
    Ok(Json(json!({ "ok": true, "id": id })))
}

#[derive(Debug, Serialize)]
struct ChannelStatusRow {
    telegram: &'static str,
}

async fn get_channel_status(
    State(state): State<DashboardState>,
) -> Result<Json<ChannelStatusRow>, DashboardError> {
    let telegram = match (state.telegram_status)() {
        TelegramConnectionStatus::Connected => "Connected",
        TelegramConnectionStatus::Disconnected => "Disconnected",
        TelegramConnectionStatus::Reconnecting => "Reconnecting",
    };

    Ok(Json(ChannelStatusRow { telegram }))
}

fn load_debate_groups(db_path: &PathBuf) -> Result<Vec<DebateGroup>, DashboardError> {
    let connection = open_read_only(db_path)?;
    let mut statement = connection.prepare(
        "SELECT id, timestamp, agent_id, payload, token_count
         FROM audit_log
         WHERE event_type = 'debate_message'
         ORDER BY id ASC",
    )?;
    let rows = statement.query_map([], |row| {
        let payload_text: String = row.get(3)?;
        let payload: Value = serde_json::from_str(&payload_text)
            .unwrap_or_else(|_| Value::Object(Default::default()));
        Ok(DebateTranscriptItem {
            row_id: row.get(0)?,
            timestamp: row.get(1)?,
            agent_id: row.get(2)?,
            phase: payload
                .get("phase")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            round: payload
                .get("round")
                .and_then(Value::as_u64)
                .unwrap_or_default() as usize,
            role: payload
                .get("role")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            content: payload
                .get("content")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            tokens: row.get::<_, i64>(4)? as usize,
        })
    })?;

    let mut groups = Vec::new();
    let mut current_group: Option<DebateGroup> = None;

    for row in rows {
        let item = row?;
        let current = current_group.get_or_insert_with(|| DebateGroup {
            id: item.row_id.to_string(),
            started_at: item.timestamp.clone(),
            transcript: Vec::new(),
        });
        let is_converge = item.phase.eq_ignore_ascii_case("Converge");
        current.transcript.push(item);
        if is_converge && let Some(group) = current_group.take() {
            groups.push(group);
        }
    }

    if let Some(group) = current_group.take() {
        groups.push(group);
    }

    Ok(groups)
}

fn open_read_only(db_path: &PathBuf) -> Result<Connection, DashboardError> {
    Connection::open_with_flags(db_path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(DashboardError::Db)
}

fn map_audit_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<AuditRow> {
    let payload_text: String = row.get(5)?;
    Ok(AuditRow {
        id: row.get(0)?,
        timestamp: row.get(1)?,
        event_type: row.get(2)?,
        agent_id: row.get(3)?,
        channel: row.get(4)?,
        payload: serde_json::from_str(&payload_text)
            .unwrap_or(Value::String(payload_text)),
        token_count: row.get::<_, i64>(6)? as usize,
    })
}

#[derive(Debug)]
pub(crate) enum DashboardError {
    Db(rusqlite::Error),
    Json(serde_json::Error),
    NotFound(String),
}

impl From<rusqlite::Error> for DashboardError {
    fn from(value: rusqlite::Error) -> Self {
        Self::Db(value)
    }
}

impl From<serde_json::Error> for DashboardError {
    fn from(value: serde_json::Error) -> Self {
        Self::Json(value)
    }
}

impl IntoResponse for DashboardError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            Self::Db(error) => (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()),
            Self::Json(error) => (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()),
            Self::NotFound(message) => (StatusCode::NOT_FOUND, message),
        };

        (status, Json(json!({ "error": message }))).into_response()
    }
}
