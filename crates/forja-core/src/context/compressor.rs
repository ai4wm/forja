use super::SummaryCallback;
use super::token_counter::count_messages_tokens;
use super::window::{compressed_summary_message, merge_history, partition_history};
use crate::error::Result;
use crate::types::{Content, Message};

pub const DEFAULT_MAX_CONTEXT_TOKENS: usize = 128_000;
const WARNING_PERCENT: usize = 80;
const SUMMARY_PERCENT: usize = 90;
const HARD_PERCENT: usize = 95;
const SOFT_RECENT_MESSAGES: usize = 10;
const HARD_RECENT_MESSAGES: usize = 5;
const EMERGENCY_RECENT_MESSAGES: usize = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CompressionOutcome {
    pub warned: bool,
    pub summarized: bool,
    pub hard_compressed: bool,
    pub emergency_compressed: bool,
}

impl CompressionOutcome {
    pub const fn emergency_only() -> Self {
        Self {
            warned: false,
            summarized: false,
            hard_compressed: false,
            emergency_compressed: true,
        }
    }
}

pub async fn compress_history(
    messages: &mut Vec<Message>,
    model: &str,
    max_context_tokens: usize,
    summary_callback: Option<&SummaryCallback>,
) -> Result<CompressionOutcome> {
    let total_tokens = count_messages_tokens(messages, model);
    compress_history_for_total(
        messages,
        total_tokens,
        model,
        max_context_tokens,
        summary_callback,
    )
    .await
}

pub async fn compress_history_for_total(
    messages: &mut Vec<Message>,
    total_tokens: usize,
    model: &str,
    max_context_tokens: usize,
    summary_callback: Option<&SummaryCallback>,
) -> Result<CompressionOutcome> {
    let thresholds = CompressionThresholds::from_max_context_tokens(max_context_tokens);
    let mut outcome = CompressionOutcome::default();
    let mut working_total = total_tokens;

    if working_total >= thresholds.warning_tokens {
        outcome.warned = true;
    }

    if working_total >= thresholds.summary_tokens {
        let segments = partition_history(messages, SOFT_RECENT_MESSAGES);
        if !segments.compressible.is_empty() {
            let summary = summarize_messages(&segments.compressible, summary_callback).await?;
            *messages = merge_history(
                segments.preserved,
                Some(compressed_summary_message(summary)),
                segments.recent,
            );
            working_total = count_messages_tokens(messages, model);
            outcome.summarized = true;
        }
    }

    if working_total >= thresholds.hard_tokens {
        let segments = partition_history(messages, HARD_RECENT_MESSAGES);
        let summary = if segments.compressible.is_empty() {
            None
        } else {
            Some(compressed_summary_message(
                summarize_messages(&segments.compressible, summary_callback).await?,
            ))
        };
        *messages = merge_history(segments.preserved, summary, segments.recent);
        outcome.hard_compressed = true;
    }

    Ok(outcome)
}

pub async fn emergency_compress_history(
    messages: &mut Vec<Message>,
    _model: &str,
    summary_callback: Option<&SummaryCallback>,
) -> Result<CompressionOutcome> {
    let split_at = messages.len().saturating_sub(EMERGENCY_RECENT_MESSAGES);
    let older_messages = messages[..split_at].to_vec();
    let recent = messages[split_at..].to_vec();

    if older_messages.is_empty() {
        return Ok(CompressionOutcome::emergency_only());
    }

    let summary = summarize_messages(&older_messages, summary_callback).await?;
    let paragraph = summary
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join(" ");

    let mut compressed = vec![compressed_summary_message(paragraph)];
    compressed.extend(recent);
    *messages = compressed;

    Ok(CompressionOutcome::emergency_only())
}

#[derive(Debug, Clone, Copy)]
struct CompressionThresholds {
    warning_tokens: usize,
    summary_tokens: usize,
    hard_tokens: usize,
}

impl CompressionThresholds {
    fn from_max_context_tokens(max_context_tokens: usize) -> Self {
        Self {
            warning_tokens: max_context_tokens * WARNING_PERCENT / 100,
            summary_tokens: max_context_tokens * SUMMARY_PERCENT / 100,
            hard_tokens: max_context_tokens * HARD_PERCENT / 100,
        }
    }
}

async fn summarize_messages(
    messages: &[Message],
    summary_callback: Option<&SummaryCallback>,
) -> Result<String> {
    if let Some(summary_callback) = summary_callback {
        return summary_callback(messages.to_vec()).await;
    }

    Ok(fallback_summary(messages))
}

fn fallback_summary(messages: &[Message]) -> String {
    let mut lines = Vec::new();

    for message in messages.iter().take(3) {
        let snippet = message_text(message);
        if !snippet.is_empty() {
            lines.push(snippet);
        }
    }

    if lines.is_empty() {
        "Compressed older context.".to_string()
    } else {
        lines.join("\n")
    }
}

fn message_text(message: &Message) -> String {
    match &message.content {
        Content::Text { text, .. } => text.clone(),
        Content::ToolCall {
            tool_name,
            arguments,
            reasoning_content,
            ..
        } => {
            let reasoning = reasoning_content.as_deref().unwrap_or_default();
            format!("Tool {tool_name}: {arguments} {reasoning}")
                .trim()
                .to_string()
        }
        Content::ToolResult { result, .. } => format!("Tool result: {result}"),
    }
}
