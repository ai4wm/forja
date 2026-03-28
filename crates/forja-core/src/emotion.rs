use crate::types::{Content, Message, Role};
use chrono::{DateTime, Duration, Local, LocalResult, NaiveDate, NaiveDateTime, TimeZone, Timelike};

const LONG_ABSENCE_DAYS: i64 = 3;
const HIGH_FREQUENCY_THRESHOLD: usize = 4;
const HIGH_FREQUENCY_WINDOW_MINUTES: i64 = 60;
const LATE_NIGHT_END_HOUR: u32 = 5;
const FRUSTRATION_KEYWORDS: &[&str] = &[
    "annoyed",
    "blocked",
    "broken",
    "error",
    "failing",
    "failed",
    "frustrated",
    "frustrating",
    "stuck",
    "why does this keep",
];

#[derive(Debug, Clone, Default)]
pub struct EmotionEngine;

impl EmotionEngine {
    pub fn new() -> Self {
        Self
    }

    pub fn detect_signals(
        &self,
        recent_messages: &[Message],
        memory_content: &str,
        now: DateTime<Local>,
    ) -> Vec<String> {
        let entries = parse_memory_entries(memory_content);
        let mut signals = Vec::new();

        if is_late_night(now) {
            signals.push("late_night_detected".to_string());
        }

        if is_first_session_today(&entries, now) {
            signals.push("first_session_today".to_string());
        }

        if has_long_absence(&entries, now) {
            signals.push("long_absence_detected".to_string());
        }

        if has_high_frequency(&entries, now) {
            signals.push("high_frequency_detected".to_string());
        }

        if has_frustration(recent_messages) {
            signals.push("frustration_detected".to_string());
        }

        signals
    }
}

pub struct RelationshipContext;

impl RelationshipContext {
    pub fn detect_patterns(
        recent_messages: &[Message],
        memory_content: &str,
        now: DateTime<Local>,
    ) -> Vec<String> {
        EmotionEngine::new().detect_signals(recent_messages, memory_content, now)
    }
}

#[derive(Debug, Clone)]
struct MemoryEntryView {
    timestamp: Option<DateTime<Local>>,
}

fn parse_memory_entries(memory_content: &str) -> Vec<MemoryEntryView> {
    let mut entries = Vec::new();
    let mut current_date = None;

    for line in memory_content.lines() {
        if let Some(date) = parse_date_header(line) {
            current_date = Some(date);
            continue;
        }

        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        entries.push(MemoryEntryView {
            timestamp: parse_entry_timestamp(current_date, trimmed),
        });
    }

    entries
}

fn is_late_night(now: DateTime<Local>) -> bool {
    now.hour() < LATE_NIGHT_END_HOUR
}

fn is_first_session_today(entries: &[MemoryEntryView], now: DateTime<Local>) -> bool {
    !entries.iter().any(|entry| {
        entry.timestamp
            .map(|timestamp| timestamp.date_naive() == now.date_naive())
            .unwrap_or(false)
    })
}

fn has_long_absence(entries: &[MemoryEntryView], now: DateTime<Local>) -> bool {
    let Some(last_timestamp) = entries.iter().filter_map(|entry| entry.timestamp).next_back() else {
        return false;
    };

    now.signed_duration_since(last_timestamp) >= Duration::days(LONG_ABSENCE_DAYS)
}

fn has_high_frequency(entries: &[MemoryEntryView], now: DateTime<Local>) -> bool {
    entries
        .iter()
        .filter_map(|entry| entry.timestamp)
        .filter(|timestamp| {
            let delta = now.signed_duration_since(*timestamp);
            delta >= Duration::zero() && delta <= Duration::minutes(HIGH_FREQUENCY_WINDOW_MINUTES)
        })
        .count()
        >= HIGH_FREQUENCY_THRESHOLD
}

fn has_frustration(recent_messages: &[Message]) -> bool {
    recent_messages
        .iter()
        .filter(|message| message.role == Role::User)
        .filter_map(|message| match &message.content {
            Content::Text { text, .. } => Some(text.to_lowercase()),
            _ => None,
        })
        .any(|text| FRUSTRATION_KEYWORDS.iter().any(|keyword| text.contains(keyword)))
}

fn parse_date_header(line: &str) -> Option<NaiveDate> {
    let date_text = line.strip_prefix("--- ")?.strip_suffix(" ---")?;
    NaiveDate::parse_from_str(date_text, "%Y-%m-%d").ok()
}

fn parse_entry_timestamp(current_date: Option<NaiveDate>, line: &str) -> Option<DateTime<Local>> {
    let current_date = current_date?;
    let time_text = line.split(" | ").next()?;
    let (hour, minute) = time_text.split_once(':')?;
    let hour = hour.parse::<u32>().ok()?;
    let minute = minute.parse::<u32>().ok()?;
    let naive = NaiveDateTime::new(current_date, chrono::NaiveTime::from_hms_opt(hour, minute, 0)?);

    match Local.from_local_datetime(&naive) {
        LocalResult::Single(timestamp) => Some(timestamp),
        LocalResult::Ambiguous(timestamp, _) => Some(timestamp),
        LocalResult::None => None,
    }
}
