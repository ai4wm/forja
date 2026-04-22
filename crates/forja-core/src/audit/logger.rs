use crate::error::{ForjaError, Result};
use chrono::Utc;
use rusqlite::{Connection, params};
use serde_json::Value;
use std::path::Path;
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, PartialEq)]
pub struct AuditEvent {
    pub timestamp: String,
    pub event_type: String,
    pub agent_id: String,
    pub channel: Option<String>,
    pub payload: Value,
    pub token_count: usize,
}

impl AuditEvent {
    pub fn new(event_type: impl Into<String>, payload: Value) -> Self {
        Self {
            timestamp: Utc::now().to_rfc3339(),
            event_type: event_type.into(),
            agent_id: "default".to_string(),
            channel: None,
            payload,
            token_count: 0,
        }
    }

    pub fn with_agent_id(mut self, agent_id: impl Into<String>) -> Self {
        self.agent_id = agent_id.into();
        self
    }

    pub fn with_channel(mut self, channel: impl Into<String>) -> Self {
        self.channel = Some(channel.into());
        self
    }

    pub fn with_token_count(mut self, token_count: usize) -> Self {
        self.token_count = token_count;
        self
    }
}

#[derive(Clone)]
pub struct AuditLogger {
    db: Arc<Mutex<Connection>>,
}

impl AuditLogger {
    pub fn new(db_path: &Path) -> Result<Self> {
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|error| ForjaError::Storage(error.to_string()))?;
        }

        let connection =
            Connection::open(db_path).map_err(|error| ForjaError::Storage(error.to_string()))?;
        connection
            .execute(
                "CREATE TABLE IF NOT EXISTS audit_log (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    timestamp TEXT NOT NULL,
                    event_type TEXT NOT NULL,
                    agent_id TEXT DEFAULT 'default',
                    channel TEXT,
                    payload TEXT NOT NULL,
                    token_count INTEGER DEFAULT 0
                )",
                [],
            )
            .map_err(|error| ForjaError::Storage(error.to_string()))?;

        Ok(Self {
            db: Arc::new(Mutex::new(connection)),
        })
    }

    pub fn log_event(&self, event: AuditEvent) -> Result<()> {
        let payload = serde_json::to_string(&event.payload)?;
        let connection = self
            .db
            .lock()
            .map_err(|error| ForjaError::Storage(error.to_string()))?;
        connection
            .execute(
                "INSERT INTO audit_log (
                    timestamp,
                    event_type,
                    agent_id,
                    channel,
                    payload,
                    token_count
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    event.timestamp,
                    event.event_type,
                    event.agent_id,
                    event.channel,
                    payload,
                    event.token_count as i64
                ],
            )
            .map_err(|error| ForjaError::Storage(error.to_string()))?;
        Ok(())
    }

    pub fn query_recent(&self, limit: usize) -> Result<Vec<AuditEvent>> {
        let connection = self
            .db
            .lock()
            .map_err(|error| ForjaError::Storage(error.to_string()))?;
        let mut statement = connection
            .prepare(
                "SELECT timestamp, event_type, agent_id, channel, payload, token_count
                 FROM audit_log
                 ORDER BY id DESC
                 LIMIT ?1",
            )
            .map_err(|error| ForjaError::Storage(error.to_string()))?;

        let rows = statement
            .query_map([limit as i64], |row| {
                let payload_text: String = row.get(4)?;
                Ok(AuditEvent {
                    timestamp: row.get(0)?,
                    event_type: row.get(1)?,
                    agent_id: row.get(2)?,
                    channel: row.get(3)?,
                    payload: serde_json::from_str(&payload_text)
                        .unwrap_or(Value::String(payload_text)),
                    token_count: row.get::<_, i64>(5)? as usize,
                })
            })
            .map_err(|error| ForjaError::Storage(error.to_string()))?;

        let mut events = Vec::new();
        for row in rows {
            events.push(row.map_err(|error| ForjaError::Storage(error.to_string()))?);
        }
        events.reverse();
        Ok(events)
    }
}
