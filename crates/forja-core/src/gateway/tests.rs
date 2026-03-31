use super::adapter::{ChannelAdapter, CliAdapter, TelegramAdapter};
use super::{ChannelKind, MessageType};
use crate::types::{Content, Message, Role};

#[test]
fn test_cli_message_round_trip_preserves_text_content() {
    let adapter = CliAdapter;
    let original = Message::text(Role::User, "hello from cli", None);

    let envelope = adapter.to_envelope(original.clone());
    let reconstructed = adapter.from_envelope(envelope);

    assert_eq!(reconstructed.role, Role::User);
    match reconstructed.content {
        Content::Text { text, .. } => assert_eq!(text, "hello from cli"),
        other => panic!("expected text content, got {other:?}"),
    }
}

#[test]
fn test_telegram_message_round_trip_preserves_text_content() {
    let adapter = TelegramAdapter;
    let original = Message::text(Role::Assistant, "hello telegram", None);

    let envelope = adapter.to_envelope(original.clone());
    let reconstructed = adapter.from_envelope(envelope);

    assert_eq!(reconstructed.role, Role::Assistant);
    match reconstructed.content {
        Content::Text { text, .. } => assert_eq!(text, "hello telegram"),
        other => panic!("expected text content, got {other:?}"),
    }
}

#[test]
fn test_channel_kind_and_message_type_serialize_correctly() {
    assert_eq!(
        serde_json::to_string(&ChannelKind::Cli).expect("channel kind should serialize"),
        "\"Cli\""
    );
    assert_eq!(
        serde_json::to_string(&MessageType::Approval).expect("message type should serialize"),
        "\"Approval\""
    );
}
