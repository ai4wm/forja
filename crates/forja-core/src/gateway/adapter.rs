use super::{ChannelKind, Envelope, MessageType};
use crate::types::{Content, Message, Role};
use chrono::{TimeZone, Utc};

#[allow(clippy::wrong_self_convention)]
pub trait ChannelAdapter: Send + Sync {
    fn to_envelope(&self, raw: Message) -> Envelope;
    fn from_envelope(&self, envelope: Envelope) -> Message;
}

pub struct CliAdapter;

pub struct TelegramAdapter;

pub struct DiscordAdapter;

impl ChannelAdapter for CliAdapter {
    fn to_envelope(&self, raw: Message) -> Envelope {
        message_to_envelope(raw, ChannelKind::Cli)
    }

    fn from_envelope(&self, envelope: Envelope) -> Message {
        envelope_to_message(envelope)
    }
}

impl ChannelAdapter for TelegramAdapter {
    fn to_envelope(&self, raw: Message) -> Envelope {
        message_to_envelope(raw, ChannelKind::Telegram)
    }

    fn from_envelope(&self, envelope: Envelope) -> Message {
        envelope_to_message(envelope)
    }
}

impl ChannelAdapter for DiscordAdapter {
    fn to_envelope(&self, raw: Message) -> Envelope {
        message_to_envelope(raw, ChannelKind::Discord)
    }

    fn from_envelope(&self, envelope: Envelope) -> Message {
        envelope_to_message(envelope)
    }
}

fn message_to_envelope(raw: Message, channel: ChannelKind) -> Envelope {
    let sender = role_sender(&raw.role).to_string();
    let text = match raw.content {
        Content::Text { text, .. } => text,
        Content::ToolCall {
            call_id,
            tool_name,
            arguments,
            reasoning_content,
            ..
        } => {
            let reasoning = reasoning_content.unwrap_or_default();
            format!(
                "call_id={call_id} tool_name={tool_name} arguments={arguments} reasoning={reasoning}"
            )
        }
        Content::ToolResult { call_id, result } => {
            format!("call_id={call_id} result={result}")
        }
    };

    Envelope {
        id: raw.id,
        sender,
        text,
        channel,
        timestamp: Utc
            .timestamp_opt(raw.timestamp as i64, 0)
            .single()
            .unwrap_or_else(Utc::now),
        msg_type: MessageType::Chat,
    }
}

fn envelope_to_message(envelope: Envelope) -> Message {
    Message::text(sender_role(&envelope.sender), envelope.text, None)
}

fn role_sender(role: &Role) -> &'static str {
    match role {
        Role::User => "user",
        Role::Assistant => "assistant",
        Role::System => "system",
        Role::Tool => "tool",
    }
}

fn sender_role(sender: &str) -> Role {
    match sender {
        "user" => Role::User,
        "assistant" => Role::Assistant,
        "system" => Role::System,
        "tool" => Role::Tool,
        _ => Role::User,
    }
}
