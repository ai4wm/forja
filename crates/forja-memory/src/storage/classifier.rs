use std::path::Path;

const STOP_WORDS: &[&str] = &[
    "a", "an", "and", "are", "about", "after", "before", "do", "does", "for", "how", "i",
    "is", "it", "like", "my", "of", "or", "that", "the", "their", "them", "this", "to", "we",
    "what", "when", "where", "who", "why", "you",
];

pub(crate) fn classify_topic_slug(content: &str) -> String {
    let normalized = normalize_text(content);

    if contains_any(&normalized, &["my name", "call me", "address me", "assistant name"]) {
        return "people".to_string();
    }
    if contains_any(&normalized, &["prefer", "favorite", "usually", "i like", "i love"]) {
        return "preferences".to_string();
    }
    if contains_any(
        &normalized,
        &["must", "never", "always", "rule", "rules", "decision", "constraint"],
    ) {
        return "decisions".to_string();
    }
    if contains_any(
        &normalized,
        &["project", "phase", "spec", "refactor", "implement", "forja", "rust"],
    ) {
        return "projects".to_string();
    }
    if contains_any(&normalized, &["todo", "task", "next", "plan", "remember", "fix"]) {
        return "workflow".to_string();
    }

    "general".to_string()
}

pub(crate) fn normalize_text(value: &str) -> String {
    value
        .to_lowercase()
        .chars()
        .map(|char| match char {
            'a'..='z' | '0'..='9' => char,
            _ => ' ',
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

pub(crate) fn summary_text(content: &str, limit: usize) -> String {
    let collapsed = content.split_whitespace().collect::<Vec<_>>().join(" ");

    if collapsed.chars().count() <= limit {
        return collapsed;
    }

    let mut truncated = collapsed.chars().take(limit.saturating_sub(1)).collect::<String>();
    truncated.push('…');
    truncated
}

pub(crate) fn query_score(query: &str, slug: &str, summary: &str) -> usize {
    let query_tokens = tokenize(query);
    if query_tokens.is_empty() {
        return 0;
    }

    let slug_tokens = tokenize(slug);
    let summary_tokens = tokenize(summary);
    let mut score = 0;

    for token in query_tokens {
        if slug_tokens.iter().any(|candidate| candidate == &token) {
            score += 3;
        }
        if summary_tokens.iter().any(|candidate| candidate == &token) {
            score += 1;
        }
    }

    score
}

pub(crate) fn topic_file_name(slug: &str, shard: usize) -> String {
    if shard <= 1 {
        return format!("{slug}.md");
    }

    format!("{slug}-{shard}.md")
}

pub(crate) fn parse_topic_file_name(path: &Path) -> Option<(String, usize)> {
    let name = path.file_name()?.to_str()?.strip_suffix(".md")?;

    if let Some((slug, suffix)) = name.rsplit_once('-')
        && let Ok(shard) = suffix.parse::<usize>()
    {
        return Some((slug.to_string(), shard));
    }

    Some((name.to_string(), 1))
}

fn tokenize(value: &str) -> Vec<String> {
    normalize_text(value)
        .split_whitespace()
        .filter(|token| !STOP_WORDS.contains(token))
        .map(str::to_string)
        .collect()
}

fn contains_any(haystack: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| haystack.contains(needle))
}
