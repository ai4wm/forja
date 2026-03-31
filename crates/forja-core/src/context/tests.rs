use super::compressor::{
    compress_history,
    emergency_compress_history,
    CompressionOutcome,
};
use super::token_counter::{count_messages_tokens, count_tokens};
use super::window::{compressed_summary_message, is_compressed_summary};
use super::SummaryCallback;
use crate::types::{Message, Role};
use serde_json::json;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};

#[test]
fn test_token_counting_accuracy_for_known_strings() {
    assert_eq!(count_tokens("", "cl100k_base"), 0);
    assert_eq!(count_tokens("hello", "cl100k_base"), 1);
    assert_eq!(count_tokens("hello world", "cl100k_base"), 2);

    let messages = vec![
        Message::text(Role::User, "hello", None),
        Message::text(Role::Assistant, "world", None),
    ];
    assert!(count_messages_tokens(&messages, "cl100k_base") >= 2);
}

#[test]
fn test_compression_triggers_at_correct_thresholds() {
    let mut warn_only = seeded_messages(4, 26);
    let warn_callback = summary_callback("warn summary");
    let warn = block_on_ready(compress_history(
        &mut warn_only,
        "cl100k_base",
        128,
        Some(&warn_callback),
    ))
    .expect("warn compression should succeed");
    assert!(warn.warned);
    assert!(!warn.summarized);
    assert!(!warn.hard_compressed);

    let mut summarize = seeded_messages(12, 11);
    let summarize_callback = summary_callback("sum");
    let summarized = block_on_ready(compress_history(
        &mut summarize,
        "cl100k_base",
        140,
        Some(&summarize_callback),
    ))
    .expect("soft compression should succeed");
    assert!(summarized.summarized);
    assert!(!summarized.hard_compressed);

    let mut hard = seeded_messages(12, 16);
    let hard_callback = summary_callback("hard summary");
    let hard_result = block_on_ready(compress_history(
        &mut hard,
        "cl100k_base",
        128,
        Some(&hard_callback),
    ))
    .expect("hard compression should succeed");
    assert!(hard_result.hard_compressed);
}

#[test]
fn test_system_prompt_and_recent_messages_survive_compression() {
    let pinned = Message::text(Role::System, "Pinned system context", None)
        .with_metadata("tokens", json!(8));
    let mut messages = vec![pinned.clone()];
    messages.extend(seeded_messages(12, 16));

    let recent_ids: Vec<String> = messages
        .iter()
        .rev()
        .take(10)
        .map(|message| message.id.clone())
        .collect();
    let compress_callback = summary_callback("compressed summary");

    let outcome = block_on_ready(compress_history(
        &mut messages,
        "cl100k_base",
        200,
        Some(&compress_callback),
    ))
    .expect("compression should succeed");

    assert!(outcome.summarized);
    assert!(messages.iter().any(|message| message.id == pinned.id));
    assert!(messages.iter().any(is_compressed_summary));
    for message_id in recent_ids {
        assert!(
            messages.iter().any(|message| message.id == message_id),
            "recent message {message_id} should survive compression"
        );
    }
}

#[test]
fn test_emergency_compression_preserves_last_three_messages() {
    let mut messages = seeded_messages(8, 20);
    let last_three: Vec<String> = messages
        .iter()
        .rev()
        .take(3)
        .map(|message| message.id.clone())
        .collect();
    let emergency_callback = summary_callback("line 1\nline 2\nline 3");

    let outcome = block_on_ready(emergency_compress_history(
        &mut messages,
        "cl100k_base",
        Some(&emergency_callback),
    ))
    .expect("emergency compression should succeed");

    assert_eq!(outcome, CompressionOutcome::emergency_only());
    assert_eq!(messages.len(), 4);
    assert!(is_compressed_summary(&messages[0]));
    let expected_summary = compressed_summary_message("line 1 line 2 line 3".to_string());
    assert_eq!(messages[0].role, expected_summary.role);
    assert_eq!(messages[0].metadata, expected_summary.metadata);
    assert_eq!(messages[0].content_text_len(), expected_summary.content_text_len());
    let remaining_ids: Vec<String> = messages
        .iter()
        .skip(1)
        .map(|message| message.id.clone())
        .collect();
    assert_eq!(remaining_ids, last_three.into_iter().rev().collect::<Vec<_>>());
}

fn seeded_messages(count: usize, tokens_per_message: usize) -> Vec<Message> {
    (0..count)
        .map(|index| {
            let role = if index % 2 == 0 {
                Role::User
            } else {
                Role::Assistant
            };
            Message::text(role, format!("message {index}"), None)
                .with_metadata("tokens", json!(tokens_per_message))
        })
        .collect()
}

fn summary_callback(summary: &'static str) -> SummaryCallback {
    Box::new(move |_messages: Vec<Message>| {
        Box::pin(async move { Ok::<String, crate::error::ForjaError>(summary.to_string()) })
    })
}

fn block_on_ready<F>(future: F) -> F::Output
where
    F: Future,
{
    let mut future = Box::pin(future);
    let waker = noop_waker();
    let mut context = Context::from_waker(&waker);

    match Future::poll(Pin::as_mut(&mut future), &mut context) {
        Poll::Ready(output) => output,
        Poll::Pending => panic!("future unexpectedly pending in unit test"),
    }
}

fn noop_waker() -> Waker {
    unsafe { Waker::from_raw(noop_raw_waker()) }
}

fn noop_raw_waker() -> RawWaker {
    RawWaker::new(std::ptr::null(), &NOOP_WAKER_VTABLE)
}

static NOOP_WAKER_VTABLE: RawWakerVTable = RawWakerVTable::new(
    |_data| noop_raw_waker(),
    |_data| {},
    |_data| {},
    |_data| {},
);
