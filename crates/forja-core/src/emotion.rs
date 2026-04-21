use crate::error::Result;
use crate::traits::LlmProvider;
use crate::types::{Content, Message, Role};
use chrono::{DateTime, Duration, Local, LocalResult, NaiveDate, NaiveDateTime, TimeZone, Timelike};
use serde::Deserialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MoodKey {
    Neutral,
    Happy,
    Focused,
    Concerned,
    Excited,
}

impl MoodKey {
    fn from_str(value: &str) -> Option<Self> {
        match value.trim().to_lowercase().as_str() {
            "neutral" => Some(Self::Neutral),
            "happy" => Some(Self::Happy),
            "focused" => Some(Self::Focused),
            "concerned" => Some(Self::Concerned),
            "excited" => Some(Self::Excited),
            _ => None,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Neutral => "neutral",
            Self::Happy => "happy",
            Self::Focused => "focused",
            Self::Concerned => "concerned",
            Self::Excited => "excited",
        }
    }

    fn default_tone_instruction(self, intensity: u8) -> String {
        match self {
            Self::Happy => {
                if intensity >= 4 {
                    "Reply in a bright, confident tone that shares the user's excitement.".to_string()
                } else {
                    "Reply in a gentle, positive tone.".to_string()
                }
            }
            Self::Focused => "Reply in a concise, execution-focused tone.".to_string(),
            Self::Concerned => "Reply in a calm, reassuring tone.".to_string(),
            Self::Excited => "Reply in an energetic, momentum-building tone.".to_string(),
            Self::Neutral => "Reply in a balanced, respectful tone.".to_string(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelationshipSignal {
    LateNightDetected,
    LongAbsenceDetected,
    ProgressStreakDetected,
    ErrorStreakDetected,
}

impl RelationshipSignal {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::LateNightDetected => "late_night_detected",
            Self::LongAbsenceDetected => "long_absence_detected",
            Self::ProgressStreakDetected => "progress_streak_detected",
            Self::ErrorStreakDetected => "error_streak_detected",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MoodState {
    pub mood: String,
    pub intensity: u8,
    pub reason: String,
    pub tone_instruction: String,
    pub updated_at: DateTime<Local>,
}

impl MoodState {
    pub fn neutral() -> Self {
        let mood = MoodKey::Neutral.as_str().to_string();
        let intensity = 1;
        Self {
            tone_instruction: MoodKey::Neutral.default_tone_instruction(intensity),
            mood,
            intensity,
            reason: "neutral".to_string(),
            updated_at: Local::now(),
        }
    }

    pub fn to_memory_tag(&self) -> String {
        let mood = sanitize_tag_part(&self.mood);
        let intensity = self.intensity.clamp(1, 5);
        let reason = sanitize_tag_part(&self.reason);
        format!("[mood:{mood}:{intensity}:{reason}]")
    }

    pub fn from_memory_tag(line: &str) -> Option<Self> {
        let (_, rest) = line.split_once("[mood:")?;
        let payload = rest.split(']').next()?;
        let mut parts = payload.splitn(4, ':');
        let (mood, mood_key) = parse_mood_value(parts.next()?)?;
        let intensity = parts.next()?.trim().parse::<u8>().ok()?.clamp(1, 5);
        let reason = parts.next()?.trim().to_string();

        if reason.is_empty() {
            return None;
        }

        Some(Self {
            tone_instruction: default_tone_instruction(mood_key, intensity),
            mood,
            intensity,
            reason,
            updated_at: Local::now(),
        })
    }

    pub fn tone_section(&self) -> String {
        format!("[tone]\n{}", self.tone_instruction)
    }

    pub fn has_changed_from(&self, previous: &Self) -> bool {
        previous.mood != self.mood
            || previous.intensity != self.intensity
            || previous.reason != self.reason
    }
}

#[derive(Debug, Clone)]
pub struct EmotionEngine {
    pub current: MoodState,
}

impl EmotionEngine {
    pub fn new(current: MoodState) -> Self {
        Self { current }
    }

    pub async fn analyze(
        &mut self,
        recent_messages: &[Message],
        provider: &dyn LlmProvider,
    ) -> Result<MoodState> {
        let previous = self.current.clone();
        let prompt = build_emotion_prompt(&previous, recent_messages);
        let response = match provider
            .chat(
                &[
                    Message::text(
                        Role::System,
                        "You analyze conversation mood and respond with JSON only.",
                        None,
                    ),
                    Message::text(Role::User, prompt, None),
                ],
                None,
            )
            .await
        {
            Ok(response) => response,
            Err(_) => return Ok(previous),
        };

        let Content::Text { text, .. } = response.content else {
            return Ok(previous);
        };

        let parsed: MoodResponse = match serde_json::from_str(text.trim()) {
            Ok(parsed) => parsed,
            Err(_) => return Ok(previous),
        };
        let Some((mood, mood_key)) = parse_mood_value(&parsed.mood) else {
            return Ok(previous);
        };
        let reason = sanitize_tag_part(&parsed.reason);

        if reason.is_empty() {
            return Ok(previous);
        }

        let next = MoodState {
            tone_instruction: normalize_tone_instruction(&parsed.tone_instruction, mood_key, parsed.intensity),
            mood,
            intensity: parsed.intensity.clamp(1, 5),
            reason,
            updated_at: Local::now(),
        };
        self.current = next.clone();

        Ok(next)
    }

    pub fn restore_from_memory(memory_content: &str) -> Option<MoodState> {
        let mut current_date = None;
        let mut restored = None;

        for line in memory_content.lines() {
            if let Some(date) = parse_date_header(line) {
                current_date = Some(date);
                continue;
            }

            let Some(mut mood) = MoodState::from_memory_tag(line) else {
                continue;
            };

            if let Some(updated_at) = parse_entry_timestamp(current_date, line) {
                mood.updated_at = updated_at;
            }

            restored = Some(mood);
        }

        restored
    }
}

pub struct RelationshipContext;

impl RelationshipContext {
    pub fn detect_pattern_keys(memory_content: &str) -> Vec<RelationshipSignal> {
        let entries = parse_memory_entries(memory_content);
        let mut patterns = Vec::new();

        push_pattern_if(
            &mut patterns,
            has_late_night_streak(&entries),
            RelationshipSignal::LateNightDetected,
        );

        push_pattern_if(
            &mut patterns,
            entries
                .iter()
                .filter_map(|entry| entry.timestamp)
                .next_back()
                .is_some_and(|last_timestamp| {
                    Local::now().signed_duration_since(last_timestamp) >= Duration::days(3)
                }),
            RelationshipSignal::LongAbsenceDetected,
        );

        push_pattern_if(
            &mut patterns,
            has_progress_streak(&entries),
            RelationshipSignal::ProgressStreakDetected,
        );
        push_pattern_if(
            &mut patterns,
            has_error_streak(&entries),
            RelationshipSignal::ErrorStreakDetected,
        );

        patterns
    }

    pub fn detect_patterns(memory_content: &str) -> Vec<String> {
        Self::detect_pattern_keys(memory_content)
            .into_iter()
            .map(|pattern| pattern.as_str().to_string())
            .collect()
    }

    pub fn build_context(memory_content: &str) -> Option<String> {
        let patterns = Self::detect_pattern_keys(memory_content);
        if patterns.is_empty() {
            return None;
        }

        Some(format!(
            "[relationship]\n{}",
            patterns
                .into_iter()
                .map(RelationshipSignal::as_str)
                .collect::<Vec<_>>()
                .join("\n")
        ))
    }
}

pub async fn generate_startup_greeting(
    provider: &dyn LlmProvider,
    identity_name: &str,
    user_name: &str,
    memory_content: &str,
    skip: bool,
) -> Result<Option<String>> {
    generate_startup_greeting_with_context(
        provider,
        identity_name,
        user_name,
        memory_content,
        "",
        skip,
    )
    .await
}

pub async fn generate_startup_greeting_with_context(
    provider: &dyn LlmProvider,
    identity_name: &str,
    user_name: &str,
    memory_content: &str,
    knowledge_content: &str,
    skip: bool,
) -> Result<Option<String>> {
    if skip {
        return Ok(None);
    }

    if memory_content.trim().is_empty() && knowledge_content.trim().is_empty() {
        return Ok(None);
    }

    let response = match provider
        .chat(
            &[
                Message::text(
                    Role::System,
                    "You write one natural greeting sentence for the user or NONE.",
                    None,
                ),
                Message::text(
                    Role::User,
                    format!(
                        "{user_name} just opened the app, and you are {identity_name}.\n\
Write one natural greeting sentence.\n\
Also, if there are unfinished tasks or a useful daily summary, include it naturally in your greeting.\n\
If there is nothing worth mentioning, respond with NONE only.\n\
\n\
Consider:\n\
- how long it has been since the last conversation\n\
- whether there are unfinished tasks or pending follow-ups\n\
- whether there was meaningful progress recently\n\
- whether recurring issues or patterns matter today\n\
- whether the knowledge base suggests a useful next step\n\
\n\
Memory:\n\
{memory_content}\n\
\n\
Knowledge:\n\
{knowledge_content}"
                    ),
                    None,
                ),
            ],
            None,
        )
        .await
    {
        Ok(response) => response,
        Err(_) => return Ok(None),
    };

    let Content::Text { text, .. } = response.content else {
        return Ok(None);
    };
    let trimmed = text.trim();

    if trimmed.eq_ignore_ascii_case("NONE") {
        return Ok(None);
    }

    if trimmed.is_empty() {
        return Ok(None);
    }

    Ok(Some(trimmed.to_string()))
}

#[derive(Debug, Deserialize)]
struct MoodResponse {
    mood: String,
    intensity: u8,
    reason: String,
    tone_instruction: String,
}

#[derive(Debug, Clone)]
struct MemoryEntryView {
    timestamp: Option<DateTime<Local>>,
    content: String,
}

fn build_emotion_prompt(previous: &MoodState, recent_messages: &[Message]) -> String {
    let recent_turns = recent_messages
        .iter()
        .rev()
        .filter_map(|message| {
            let Content::Text { text, .. } = &message.content else {
                return None;
            };
            Some(format!("{:?}: {text}", message.role))
        })
        .take(5)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<Vec<_>>()
        .join("\n");

    format!(
        "Analyze the emotional state of the conversation below and respond with JSON only.\n\
Previous mood: {} (intensity: {})\n\
Recent conversation:\n\
{}\n\
\nResponse format:\n\
{{\"mood\":\"...\",\"intensity\":1-5,\"reason\":\"one line\",\"tone_instruction\":\"tone guidance\"}}",
        previous.mood,
        previous.intensity,
        recent_turns
    )
}

fn parse_mood_value(value: &str) -> Option<(String, Option<MoodKey>)> {
    let mood = sanitize_tag_part(value).to_lowercase();
    if mood.is_empty() {
        return None;
    }

    Some((mood.clone(), MoodKey::from_str(&mood)))
}

fn sanitize_tag_part(value: &str) -> String {
    value
        .chars()
        .map(|char| match char {
            '[' | ']' | ':' | '\n' | '\r' | '|' => ' ',
            _ => char,
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn normalize_tone_instruction(
    value: &str,
    mood_key: Option<MoodKey>,
    intensity: u8,
) -> String {
    let normalized = sanitize_tag_part(value);
    if normalized.is_empty() {
        return default_tone_instruction(mood_key, intensity);
    }

    normalized
}

fn default_tone_instruction(mood_key: Option<MoodKey>, intensity: u8) -> String {
    mood_key
        .unwrap_or(MoodKey::Neutral)
        .default_tone_instruction(intensity)
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
            content: parse_entry_content(trimmed),
        });
    }

    entries
}

fn has_late_night_streak(entries: &[MemoryEntryView]) -> bool {
    let mut streak = 0;

    for entry in entries {
        if entry.timestamp.is_some_and(is_late_night_entry) {
            streak += 1;
            if streak >= 2 {
                return true;
            }
        } else {
            streak = 0;
        }
    }

    false
}

fn has_progress_streak(entries: &[MemoryEntryView]) -> bool {
    has_keyword_streak(entries, &["complete", "completed", "phase", "commit", "push"], 2)
}

fn has_error_streak(entries: &[MemoryEntryView]) -> bool {
    has_keyword_streak(entries, &["error", "failed", "failure", "broken"], 3)
}

fn has_keyword_streak(entries: &[MemoryEntryView], keywords: &[&str], minimum_streak: usize) -> bool {
    let mut streak = 0;

    for entry in entries {
        let normalized = entry.content.to_lowercase();
        if keywords.iter().any(|keyword| normalized.contains(&keyword.to_lowercase())) {
            streak += 1;
            if streak >= minimum_streak {
                return true;
            }
        } else {
            streak = 0;
        }
    }

    false
}

fn push_pattern_if(
    patterns: &mut Vec<RelationshipSignal>,
    condition: bool,
    pattern: RelationshipSignal,
) {
    if condition {
        patterns.push(pattern);
    }
}

fn is_late_night_entry(timestamp: DateTime<Local>) -> bool {
    let minutes = timestamp.hour() * 60 + timestamp.minute();
    minutes <= 300
}

fn parse_entry_content(line: &str) -> String {
    let mut parts = line.splitn(3, " | ");
    let _ = parts.next();
    let _ = parts.next();
    parts.next().unwrap_or(line).trim().to_string()
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
