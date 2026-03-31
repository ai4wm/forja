use crate::types::{Message, Role};
use serde_json::json;

const COMPRESSED_PREFIX: &str = "[compressed]";

#[derive(Debug, Clone)]
pub struct WindowSegments {
    pub preserved: Vec<Message>,
    pub compressible: Vec<Message>,
    pub recent: Vec<Message>,
}

pub fn partition_history(messages: &[Message], keep_recent: usize) -> WindowSegments {
    let recent_start = messages.len().saturating_sub(keep_recent);
    let mut preserved = Vec::new();
    let mut compressible = Vec::new();
    let recent = messages[recent_start..].to_vec();

    for (index, message) in messages.iter().enumerate() {
        if index >= recent_start {
            break;
        }

        if message.role == Role::System {
            preserved.push(message.clone());
        } else {
            compressible.push(message.clone());
        }
    }

    WindowSegments {
        preserved,
        compressible,
        recent,
    }
}

pub fn compressed_summary_message(summary: String) -> Message {
    Message::text(Role::System, format!("{COMPRESSED_PREFIX}\n{summary}"), None)
        .with_metadata("compressed", json!(true))
}

pub fn is_compressed_summary(message: &Message) -> bool {
    message.role == Role::System
        && message
            .metadata
            .get("compressed")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false)
}

pub fn merge_history(
    preserved: Vec<Message>,
    summary: Option<Message>,
    recent: Vec<Message>,
) -> Vec<Message> {
    let mut merged = preserved;
    if let Some(summary) = summary {
        merged.push(summary);
    }
    merged.extend(recent);
    merged
}
