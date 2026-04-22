use super::{DashboardError, DashboardState, open_read_only, table_exists};
use axum::Json;
use axum::extract::{Path, State};
use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Serialize)]
pub(crate) struct DebateSummary {
    id: String,
    started_at: String,
    message_count: usize,
    preview: String,
}

#[derive(Debug, Serialize, Clone)]
pub(crate) struct DebateTranscriptItem {
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

pub(crate) async fn get_debates(
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

pub(crate) async fn get_debate(
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

fn load_debate_groups(db_path: &std::path::PathBuf) -> Result<Vec<DebateGroup>, DashboardError> {
    let connection = open_read_only(db_path)?;
    if !table_exists(&connection, "audit_log")? {
        return Ok(Vec::new());
    }

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
