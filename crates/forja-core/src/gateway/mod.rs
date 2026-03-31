pub mod adapter;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Envelope {
    pub id: String,
    pub sender: String,
    pub text: String,
    pub channel: ChannelKind,
    pub timestamp: DateTime<Utc>,
    pub msg_type: MessageType,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChannelKind {
    Cli,
    Telegram,
    Discord,
    Internal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MessageType {
    Chat,
    Task,
    Heartbeat,
    Log,
    Approval,
}

#[cfg(test)]
mod tests;
