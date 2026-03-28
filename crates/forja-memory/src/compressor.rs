use chrono::{DateTime, Local};
use forja_core::types::{Content, Message, Role};
use std::collections::{BTreeSet, HashMap};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompressedEntry {
    pub timestamp: DateTime<Local>,
    pub summary: String,
    pub keywords: Vec<String>,
    pub original_count: usize,
    pub code_snippets: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct Compressor;

impl Compressor {
    pub fn new() -> Self {
        Self
    }

    pub fn compress(&self, messages: Vec<Message>) -> CompressedEntry {
        let texts = message_texts(&messages);
        let summary = build_summary(&texts);
        let keywords = extract_keywords(&texts);
        let code_snippets = extract_code_snippets(&texts);
        let timestamp = Local::now();

        CompressedEntry {
            timestamp,
            summary,
            keywords,
            original_count: messages.len(),
            code_snippets,
        }
    }
}

fn message_texts(messages: &[Message]) -> Vec<String> {
    messages
        .iter()
        .filter_map(|message| {
            let Content::Text { text, .. } = &message.content else {
                return None;
            };
            let role = match message.role {
                Role::User => "user",
                Role::Assistant => "assistant",
                Role::System => "system",
                Role::Tool => "tool",
            };
            Some(format!("{role}: {}", text.trim()))
        })
        .filter(|text| !text.trim().is_empty())
        .collect()
}

fn build_summary(texts: &[String]) -> String {
    let mut selected = texts
        .iter()
        .filter(|text| is_key_fact(text) || looks_like_action_item(text) || contains_code_reference(text))
        .take(4)
        .cloned()
        .collect::<Vec<_>>();

    if selected.is_empty() {
        selected = texts.iter().take(3).cloned().collect();
    }

    let summary = selected
        .into_iter()
        .map(|text| truncate_text(&text, 160))
        .collect::<Vec<_>>()
        .join(" | ");

    if summary.trim().is_empty() {
        "Conversation summary unavailable.".to_string()
    } else {
        summary
    }
}

fn extract_keywords(texts: &[String]) -> Vec<String> {
    let mut frequencies = HashMap::<String, usize>::new();
    for token in texts.iter().flat_map(|text| tokenize(text)) {
        if is_stopword(&token) || token.len() < 3 {
            continue;
        }
        *frequencies.entry(token).or_default() += 1;
    }

    let mut weighted = frequencies.into_iter().collect::<Vec<_>>();
    weighted.sort_by(|left, right| right.1.cmp(&left.1).then_with(|| left.0.cmp(&right.0)));

    let mut keywords = weighted
        .into_iter()
        .map(|(token, _)| token)
        .take(8)
        .collect::<Vec<_>>();

    if keywords.is_empty() {
        keywords.push("memory".to_string());
    }

    keywords
}

fn extract_code_snippets(texts: &[String]) -> Vec<String> {
    let snippets = texts
        .iter()
        .filter(|text| contains_code_reference(text))
        .map(|text| truncate_text(text, 200))
        .collect::<BTreeSet<_>>();

    snippets.into_iter().take(3).collect()
}

fn tokenize(text: &str) -> Vec<String> {
    text.split(|character: char| !character.is_alphanumeric() && character != '_' && character != '.')
        .map(str::trim)
        .filter(|token| !token.is_empty())
        .map(|token| token.to_lowercase())
        .collect()
}

fn is_key_fact(text: &str) -> bool {
    let normalized = text.to_lowercase();
    [
        "decide",
        "decision",
        "prefer",
        "remember",
        "implemented",
        "fixed",
        "resolved",
        "deploy",
        "ship",
    ]
    .iter()
    .any(|keyword| normalized.contains(keyword))
}

fn looks_like_action_item(text: &str) -> bool {
    let normalized = text.to_lowercase();
    [
        "todo",
        "next",
        "follow up",
        "need to",
        "should",
        "must",
        "action",
    ]
    .iter()
    .any(|keyword| normalized.contains(keyword))
}

fn contains_code_reference(text: &str) -> bool {
    let normalized = text.to_lowercase();
    [
        ".rs",
        ".py",
        ".ts",
        ".js",
        "fn ",
        "struct ",
        "impl ",
        "cargo ",
        "git ",
        "```",
        "::",
    ]
    .iter()
    .any(|token| normalized.contains(token))
}

fn truncate_text(text: &str, max_len: usize) -> String {
    if text.chars().count() <= max_len {
        return text.to_string();
    }

    let truncated = text.chars().take(max_len).collect::<String>();
    format!("{truncated}...")
}

fn is_stopword(token: &str) -> bool {
    matches!(
        token,
        "the"
            | "and"
            | "are"
            | "but"
            | "for"
            | "from"
            | "have"
            | "into"
            | "that"
            | "this"
            | "with"
            | "user"
            | "assistant"
            | "system"
            | "tool"
    )
}

#[cfg(test)]
mod tests {
    use super::Compressor;
    use forja_core::types::{Message, Role};

    #[test]
    fn compressor_creates_non_empty_summary_and_keywords() {
        let compressor = Compressor::new();
        let entry = compressor.compress(vec![
            Message::text(Role::User, "We decided to deploy after fixing auth.rs.", None),
            Message::text(
                Role::Assistant,
                "Next action: run cargo test and then deploy.",
                None,
            ),
        ]);

        assert!(!entry.summary.is_empty());
        assert!(!entry.keywords.is_empty());
        assert_eq!(entry.original_count, 2);
        assert!(!entry.code_snippets.is_empty());
    }
}
