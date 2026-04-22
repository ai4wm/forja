use crate::estimate_tokens;
use forja_core::types::MemoryEntry;

const DEFAULT_MAX_CONTEXT_TOKENS: usize = 128_000;
const WARNING_PERCENT: usize = 80;
const SUMMARY_PERCENT: usize = 90;
const RECENT_ENTRY_KEEP: usize = 6;

#[derive(Debug, Clone)]
struct SessionSummary {
    text: String,
}

#[derive(Debug, Default)]
pub struct SessionMemory {
    entries: Vec<MemoryEntry>,
    summaries: Vec<SessionSummary>,
    warned: bool,
    max_context_tokens: usize,
}

impl SessionMemory {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            summaries: Vec::new(),
            warned: false,
            max_context_tokens: std::env::var("FORJA_SESSION_MEMORY_MAX_TOKENS")
                .ok()
                .and_then(|value| value.parse::<usize>().ok())
                .filter(|value| *value > 0)
                .unwrap_or(DEFAULT_MAX_CONTEXT_TOKENS),
        }
    }

    pub fn record(&mut self, entry: &MemoryEntry) {
        self.entries.push(entry.clone());

        let warning_threshold = self.max_context_tokens * WARNING_PERCENT / 100;
        let summary_threshold = self.max_context_tokens * SUMMARY_PERCENT / 100;
        let total_tokens = self.total_tokens();

        if total_tokens >= warning_threshold && !self.warned {
            eprintln!("[Memory] Session memory usage reached 80% of the configured token budget");
            self.warned = true;
        }

        if total_tokens >= summary_threshold {
            self.compress_old_entries();
        }

        if self.total_tokens() < warning_threshold {
            self.warned = false;
        }
    }

    pub fn startup_context(&self) -> String {
        if self.summaries.is_empty() {
            return String::new();
        }

        format!(
            "[memory session summaries - Compressed Session Summaries]\n\n{}",
            self.summaries
                .iter()
                .map(|summary| summary.text.as_str())
                .collect::<Vec<_>>()
                .join("\n\n")
        )
    }

    pub fn relevant_context(&self, query: &str) -> String {
        let query_tokens = tokenize(query);
        if query_tokens.is_empty() {
            return String::new();
        }

        let mut sections = Vec::new();
        let matching_entries = self
            .entries
            .iter()
            .filter(|entry| contains_any_token(&entry.content, &query_tokens))
            .rev()
            .take(4)
            .map(render_entry)
            .collect::<Vec<_>>();
        if !matching_entries.is_empty() {
            sections.push(format!(
                "[memory session - Active Session Memory]\n\n{}",
                matching_entries
                    .into_iter()
                    .rev()
                    .collect::<Vec<_>>()
                    .join("\n")
            ));
        }

        let matching_summaries = self
            .summaries
            .iter()
            .filter(|summary| contains_any_token(&summary.text, &query_tokens))
            .map(|summary| summary.text.clone())
            .collect::<Vec<_>>();
        if !matching_summaries.is_empty() {
            sections.push(format!(
                "[memory session summaries - Relevant Compressed Session Summaries]\n\n{}",
                matching_summaries.join("\n\n")
            ));
        }

        sections.join("\n\n")
    }

    pub fn full_context(&self) -> String {
        let mut sections = Vec::new();
        let summary_context = self.startup_context();
        if !summary_context.is_empty() {
            sections.push(summary_context);
        }
        if !self.entries.is_empty() {
            sections.push(format!(
                "[memory session - Active Session Memory]\n\n{}",
                self.entries
                    .iter()
                    .rev()
                    .take(6)
                    .map(render_entry)
                    .collect::<Vec<_>>()
                    .into_iter()
                    .rev()
                    .collect::<Vec<_>>()
                    .join("\n")
            ));
        }
        sections.join("\n\n")
    }

    fn total_tokens(&self) -> usize {
        let summary_tokens = self
            .summaries
            .iter()
            .map(|summary| estimate_tokens(&summary.text))
            .sum::<usize>();
        let entry_tokens = self
            .entries
            .iter()
            .map(|entry| estimate_tokens(&render_entry(entry)))
            .sum::<usize>();
        summary_tokens + entry_tokens
    }

    fn compress_old_entries(&mut self) {
        if self.entries.len() <= RECENT_ENTRY_KEEP {
            return;
        }

        let split_at = self.entries.len() - RECENT_ENTRY_KEEP;
        let older = self.entries.drain(..split_at).collect::<Vec<_>>();
        let summary = summarize_entries(&older);
        if !summary.trim().is_empty() {
            self.summaries.push(SessionSummary { text: summary });
        }
    }
}

fn summarize_entries(entries: &[MemoryEntry]) -> String {
    if entries.is_empty() {
        return String::new();
    }

    let first_ts = entries
        .first()
        .map(|entry| entry.timestamp)
        .unwrap_or_default();
    let last_ts = entries
        .last()
        .map(|entry| entry.timestamp)
        .unwrap_or_default();
    let lines = entries
        .iter()
        .rev()
        .take(5)
        .map(render_entry)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<Vec<_>>()
        .join("\n");

    format!(
        "Session summary [{}..{}]\n{}",
        first_ts,
        last_ts,
        truncate_text(&lines, 800)
    )
}

fn render_entry(entry: &MemoryEntry) -> String {
    let role = entry
        .tags
        .iter()
        .find(|tag| matches!(tag.as_str(), "user" | "assistant" | "system" | "tool"))
        .cloned()
        .unwrap_or_else(|| "user".to_string());
    format!("- {} | {}", role, truncate_text(&entry.content, 200))
}

fn truncate_text(value: &str, limit: usize) -> String {
    let collapsed = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.chars().count() <= limit {
        return collapsed;
    }

    let mut output = collapsed
        .chars()
        .take(limit.saturating_sub(1))
        .collect::<String>();
    output.push('…');
    output
}

fn tokenize(value: &str) -> Vec<String> {
    value
        .to_lowercase()
        .chars()
        .map(|char| match char {
            'a'..='z' | '0'..='9' => char,
            _ => ' ',
        })
        .collect::<String>()
        .split_whitespace()
        .map(str::to_string)
        .collect()
}

fn contains_any_token(value: &str, query_tokens: &[String]) -> bool {
    let normalized = value.to_lowercase();
    query_tokens.iter().any(|token| normalized.contains(token))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: &str, content: &str) -> MemoryEntry {
        MemoryEntry {
            id: id.to_string(),
            content: content.to_string(),
            score: 0.0,
            timestamp: 100,
            tags: vec!["user".to_string()],
            metadata: Default::default(),
        }
    }

    #[test]
    fn session_memory_compresses_old_entries_after_threshold() {
        let mut session = SessionMemory {
            max_context_tokens: 60,
            ..SessionMemory::new()
        };

        for index in 0..8 {
            session.record(&entry(&format!("id-{index}"), &"alpha ".repeat(20)));
        }

        assert!(!session.summaries.is_empty());
        assert!(session.entries.len() <= RECENT_ENTRY_KEEP);
    }

    #[test]
    fn session_memory_relevant_context_matches_recent_entries() {
        let mut session = SessionMemory::new();
        session.record(&entry("id-1", "deploy checklist passed"));

        let relevant = session.relevant_context("deploy");

        assert!(relevant.contains("deploy checklist passed"));
    }
}
