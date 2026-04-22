use super::{DashboardError, DashboardState, open_read_only, table_exists};
use axum::Json;
use axum::extract::{Query, State};
use axum::response::Sse;
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::convert::Infallible;
use std::time::Duration;
use tokio_stream::StreamExt;

#[derive(Debug, Deserialize)]
pub(crate) struct AuditQuery {
    limit: Option<usize>,
    event_type: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ConversationQuery {
    limit: Option<usize>,
}

#[derive(Debug, Serialize)]
pub(crate) struct AuditRow {
    id: i64,
    timestamp: String,
    event_type: String,
    agent_id: String,
    channel: Option<String>,
    payload: Value,
    token_count: usize,
}

#[derive(Debug, Serialize)]
pub(crate) struct ConversationRow {
    id: i64,
    timestamp: String,
    event_type: String,
    agent_id: String,
    channel: Option<String>,
    token_count: usize,
    headline: String,
    detail: String,
    payload: Value,
}

#[derive(Debug, Serialize)]
pub(crate) struct HistoryRow {
    timestamp: String,
    event_type: String,
    payload: Value,
}

#[derive(Debug, Serialize)]
pub(crate) struct ToolRow {
    timestamp: String,
    tool_name: String,
    payload: Value,
}

pub(crate) async fn get_audit(
    State(state): State<DashboardState>,
    Query(query): Query<AuditQuery>,
) -> Result<Json<Vec<AuditRow>>, DashboardError> {
    let limit = query.limit.unwrap_or(50) as i64;
    let connection = open_read_only(&state.db_path)?;
    if !table_exists(&connection, "audit_log")? {
        return Ok(Json(Vec::new()));
    }

    let rows = if let Some(event_type) = query.event_type.filter(|value| !value.trim().is_empty()) {
        let mut statement = connection.prepare(
            "SELECT id, timestamp, event_type, agent_id, channel, payload, token_count
             FROM audit_log
             WHERE event_type = ?1
             ORDER BY id DESC
             LIMIT ?2",
        )?;
        let rows = statement.query_map(params![event_type, limit], map_audit_row)?;
        collect_rows(rows)?
    } else {
        let mut statement = connection.prepare(
            "SELECT id, timestamp, event_type, agent_id, channel, payload, token_count
             FROM audit_log
             ORDER BY id DESC
             LIMIT ?1",
        )?;
        let rows = statement.query_map([limit], map_audit_row)?;
        collect_rows(rows)?
    };

    Ok(Json(rows))
}

pub(crate) async fn get_conversation(
    State(state): State<DashboardState>,
    Query(query): Query<ConversationQuery>,
) -> Result<Json<Vec<ConversationRow>>, DashboardError> {
    let limit = query.limit.unwrap_or(36) as i64;
    let connection = open_read_only(&state.db_path)?;
    if !table_exists(&connection, "audit_log")? {
        return Ok(Json(Vec::new()));
    }

    let mut statement = connection.prepare(
        "SELECT id, timestamp, event_type, agent_id, channel, payload, token_count
         FROM audit_log
         WHERE event_type IN (
             'debate_message',
             'llm_call',
             'tool_call',
             'tool_result',
             'error',
             'compression'
         )
         ORDER BY id DESC
         LIMIT ?1",
    )?;
    let rows = statement.query_map([limit], |row| {
        let payload = parse_payload(row.get(5)?);
        let event_type = row.get::<_, String>(2)?;
        let (headline, detail) = describe_event(&event_type, &payload);
        Ok(ConversationRow {
            id: row.get(0)?,
            timestamp: row.get(1)?,
            event_type,
            agent_id: row.get(3)?,
            channel: row.get(4)?,
            payload,
            token_count: row.get::<_, i64>(6)? as usize,
            headline,
            detail,
        })
    })?;

    Ok(Json(collect_rows(rows)?))
}

pub(crate) async fn get_history(
    State(state): State<DashboardState>,
) -> Result<Json<Vec<HistoryRow>>, DashboardError> {
    let connection = open_read_only(&state.db_path)?;
    if !table_exists(&connection, "audit_log")? {
        return Ok(Json(Vec::new()));
    }

    let mut statement = connection.prepare(
        "SELECT timestamp, event_type, payload
         FROM audit_log
         WHERE event_type IN ('llm_call', 'tool_call', 'tool_result', 'error', 'compression')
         ORDER BY id DESC
         LIMIT 100",
    )?;
    let rows = statement.query_map([], |row| {
        Ok(HistoryRow {
            timestamp: row.get(0)?,
            event_type: row.get(1)?,
            payload: parse_payload(row.get(2)?),
        })
    })?;

    Ok(Json(collect_rows(rows)?))
}

pub(crate) async fn get_tools(
    State(state): State<DashboardState>,
) -> Result<Json<Vec<ToolRow>>, DashboardError> {
    let connection = open_read_only(&state.db_path)?;
    if !table_exists(&connection, "audit_log")? {
        return Ok(Json(Vec::new()));
    }

    let mut statement = connection.prepare(
        "SELECT timestamp, payload
         FROM audit_log
         WHERE event_type = 'tool_call'
         ORDER BY id DESC
         LIMIT 50",
    )?;
    let rows = statement.query_map([], |row| {
        let payload = parse_payload(row.get(1)?);
        let tool_name = payload
            .get("tool_name")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .to_string();
        Ok(ToolRow {
            timestamp: row.get(0)?,
            tool_name,
            payload,
        })
    })?;

    Ok(Json(collect_rows(rows)?))
}

pub(crate) async fn stream_events(
    State(state): State<DashboardState>,
) -> Sse<impl tokio_stream::Stream<Item = Result<axum::response::sse::Event, Infallible>>> {
    let db_path = state.db_path.clone();
    let stream =
        tokio_stream::wrappers::IntervalStream::new(tokio::time::interval(Duration::from_secs(2)))
            .then(move |_| {
                let db_path = db_path.clone();
                async move {
                    let payload = recent_event_payload(&db_path)
                        .unwrap_or_else(|error| json!({ "error": format!("{error:?}") }));
                    Ok(axum::response::sse::Event::default().data(payload.to_string()))
                }
            });

    Sse::new(stream).keep_alive(
        axum::response::sse::KeepAlive::new()
            .interval(Duration::from_secs(5))
            .text("keep-alive"),
    )
}

fn recent_event_payload(db_path: &std::path::PathBuf) -> Result<Value, DashboardError> {
    let connection = open_read_only(db_path)?;
    if !table_exists(&connection, "audit_log")? {
        return Ok(json!({
            "status": "waiting",
            "message": "audit_log unavailable",
        }));
    }

    let mut statement = connection.prepare(
        "SELECT id, timestamp, event_type, payload
         FROM audit_log
         ORDER BY id DESC
         LIMIT 1",
    )?;
    let value = statement
        .query_row([], |row| {
            let payload_text: String = row.get(3)?;
            Ok(json!({
                "id": row.get::<_, i64>(0)?,
                "timestamp": row.get::<_, String>(1)?,
                "event_type": row.get::<_, String>(2)?,
                "payload": serde_json::from_str::<Value>(&payload_text)
                    .unwrap_or(Value::String(payload_text)),
            }))
        })
        .optional()?;

    Ok(value.unwrap_or_else(|| {
        json!({
            "status": "idle",
            "message": "No audit events yet",
        })
    }))
}

fn map_audit_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<AuditRow> {
    Ok(AuditRow {
        id: row.get(0)?,
        timestamp: row.get(1)?,
        event_type: row.get(2)?,
        agent_id: row.get(3)?,
        channel: row.get(4)?,
        payload: parse_payload(row.get(5)?),
        token_count: row.get::<_, i64>(6)? as usize,
    })
}

fn parse_payload(payload_text: String) -> Value {
    serde_json::from_str(&payload_text).unwrap_or(Value::String(payload_text))
}

fn describe_event(event_type: &str, payload: &Value) -> (String, String) {
    match event_type {
        "debate_message" => (
            format!(
                "{} · {}",
                payload
                    .get("phase")
                    .and_then(Value::as_str)
                    .unwrap_or("Debate"),
                payload
                    .get("role")
                    .and_then(Value::as_str)
                    .unwrap_or("Agent")
            ),
            payload
                .get("content")
                .and_then(Value::as_str)
                .unwrap_or("(empty)")
                .to_string(),
        ),
        "llm_call" => (
            "Model invocation".to_string(),
            format!(
                "mode={}",
                payload
                    .get("mode")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown")
            ),
        ),
        "tool_call" => (
            format!(
                "Tool · {}",
                payload
                    .get("tool_name")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown")
            ),
            summarize_value(payload.get("arguments").unwrap_or(payload), 160),
        ),
        "tool_result" => (
            "Tool result".to_string(),
            summarize_value(payload.get("result").unwrap_or(payload), 160),
        ),
        "error" => (
            "Engine error".to_string(),
            payload
                .get("error")
                .and_then(Value::as_str)
                .unwrap_or("Unknown error")
                .to_string(),
        ),
        "compression" => (
            "Context compression".to_string(),
            format!(
                "{} → {} tokens",
                payload
                    .get("before_tokens")
                    .and_then(Value::as_i64)
                    .unwrap_or_default(),
                payload
                    .get("after_tokens")
                    .and_then(Value::as_i64)
                    .unwrap_or_default()
            ),
        ),
        _ => (event_type.to_string(), summarize_value(payload, 160)),
    }
}

fn summarize_value(value: &Value, max_len: usize) -> String {
    let raw = if let Some(text) = value.as_str() {
        text.to_string()
    } else {
        serde_json::to_string(value).unwrap_or_else(|_| value.to_string())
    };

    if raw.chars().count() <= max_len {
        raw
    } else {
        format!("{}...", raw.chars().take(max_len).collect::<String>())
    }
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
