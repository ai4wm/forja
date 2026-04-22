#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ExecMode {
    Safe,
    #[default]
    Auto,
    Trust,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ThinkLevel {
    Min,
    #[default]
    Mid,
    Max,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Role {
    #[default]
    Auto,
    Coder,
    Writer,
    Assistant,
    Analyst,
    Default,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ModeState {
    pub exec_mode: ExecMode,
    pub think_level: ThinkLevel,
    pub role: Role,
    detected_role: Role,
}

impl ModeState {
    pub fn new(exec_mode: ExecMode, think_level: ThinkLevel, role: Role) -> Self {
        Self {
            exec_mode,
            think_level,
            role,
            detected_role: Role::Default,
        }
    }

    pub fn update_exec_mode(&mut self, exec_mode: ExecMode) {
        self.exec_mode = exec_mode;
    }

    pub fn update_think_level(&mut self, think_level: ThinkLevel) {
        self.think_level = think_level;
    }

    pub fn update_role(&mut self, role: Role) {
        self.role = role;
        if role != Role::Auto {
            self.detected_role = role;
        }
    }

    pub fn update_detected_role(&mut self, role: Role) {
        self.detected_role = role;
    }

    pub fn effective_role(&self) -> Role {
        match self.role {
            Role::Auto => self.detected_role,
            other => other,
        }
    }
}

impl ExecMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Safe => "safe",
            Self::Auto => "auto",
            Self::Trust => "trust",
        }
    }
}

impl ThinkLevel {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Min => "min",
            Self::Mid => "mid",
            Self::Max => "max",
        }
    }
}

impl Role {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Coder => "coder",
            Self::Writer => "writer",
            Self::Assistant => "assistant",
            Self::Analyst => "analyst",
            Self::Default => "default",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlashCommand {
    Mode(ExecMode),
    Think(ThinkLevel),
    Role(Role),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NaturalLanguageCommand {
    Mode(ExecMode),
    Think(ThinkLevel),
    Role(Role),
    Model(String),
}

impl NaturalLanguageCommand {
    pub fn to_slash_command(&self) -> String {
        match self {
            Self::Mode(mode) => format!("/mode {}", mode.as_str()),
            Self::Think(level) => format!("/think {}", level.as_str()),
            Self::Role(role) => format!("/role {}", role.as_str()),
            Self::Model(model) => format!("/model {model}"),
        }
    }

    pub fn confirmation_prompt(&self) -> String {
        match self {
            Self::Mode(mode) => format!("Switch to {} mode?", mode.as_str()),
            Self::Think(level) => format!("Switch thinking level to {}?", level.as_str()),
            Self::Role(role) => format!("Switch role to {}?", role.as_str()),
            Self::Model(model) => format!("Switch model to {model}?"),
        }
    }
}

pub fn detect_role(message: &str) -> Role {
    let normalized = message.to_lowercase();

    if contains_any(
        &normalized,
        &[
            "\u{CF54}\u{B4DC}",
            "\u{C5D0}\u{B7EC}",
            "\u{BC84}\u{ADF8}",
            "cargo",
            "git",
            "\u{D568}\u{C218}",
            "\u{CEF4}\u{D30C}\u{C77C}",
            "\u{BE4C}\u{B4DC}",
            "\u{D14C}\u{C2A4}\u{D2B8}",
            "\u{AD6C}\u{D604}",
            "\u{B9AC}\u{D329}\u{D1A0}\u{B9C1}",
            "code",
            "error",
            "bug",
            "build",
            "test",
            "implement",
            "phase",
            "rust",
            "compile",
        ],
    ) {
        return Role::Coder;
    }

    if contains_any(
        &normalized,
        &[
            "\u{AE00}",
            "\u{BE14}\u{B85C}\u{ADF8}",
            "\u{AE30}\u{C0AC}",
            "\u{C81C}\u{BAA9}",
            "\u{BB38}\u{B2E8}",
            "\u{C694}\u{C57D}",
            "\u{C791}\u{C131}",
            "\u{C6D0}\u{ACE0}",
            "\u{C5D0}\u{C138}\u{C774}",
            "\u{CE74}\u{D53C}",
            "\u{CF58}\u{D150}\u{CE20}",
            "write",
            "blog",
            "article",
            "draft",
            "essay",
            "copy",
        ],
    ) {
        return Role::Writer;
    }

    if contains_any(
        &normalized,
        &[
            "\u{C77C}\u{C815}",
            "\u{BBF8}\u{D305}",
            "\u{C54C}\u{B9BC}",
            "\u{D560}\u{C77C}",
            "\u{BA54}\u{BAA8}",
            "\u{C57D}\u{C18D}",
            "schedule",
            "meeting",
            "reminder",
            "todo",
        ],
    ) {
        return Role::Assistant;
    }

    if contains_any(
        &normalized,
        &[
            "\u{BE44}\u{AD50}",
            "\u{BD84}\u{C11D}",
            "\u{C870}\u{C0AC}",
            "\u{B9AC}\u{C11C}\u{CE58}",
            "\u{D1B5}\u{ACC4}",
            "\u{AC00}\u{ACA9}",
            "compare",
            "analyze",
            "research",
            "statistics",
            "price",
        ],
    ) {
        return Role::Analyst;
    }

    Role::Default
}

pub fn parse_slash_command(input: &str) -> Option<SlashCommand> {
    let normalized = input.trim().to_lowercase();
    let mut parts = normalized.split_whitespace();
    let command = parts.next()?;
    let value = parts.next()?;
    if parts.next().is_some() {
        return None;
    }

    match command {
        "/mode" => match value {
            "safe" => Some(SlashCommand::Mode(ExecMode::Safe)),
            "auto" => Some(SlashCommand::Mode(ExecMode::Auto)),
            "trust" => Some(SlashCommand::Mode(ExecMode::Trust)),
            _ => None,
        },
        "/think" => match value {
            "min" => Some(SlashCommand::Think(ThinkLevel::Min)),
            "mid" => Some(SlashCommand::Think(ThinkLevel::Mid)),
            "max" => Some(SlashCommand::Think(ThinkLevel::Max)),
            _ => None,
        },
        "/role" => match value {
            "coder" => Some(SlashCommand::Role(Role::Coder)),
            "writer" => Some(SlashCommand::Role(Role::Writer)),
            "assistant" => Some(SlashCommand::Role(Role::Assistant)),
            "analyst" => Some(SlashCommand::Role(Role::Analyst)),
            "auto" => Some(SlashCommand::Role(Role::Auto)),
            _ => None,
        },
        _ => None,
    }
}

pub fn parse_natural_language_command(input: &str) -> Option<NaturalLanguageCommand> {
    let trimmed = input.trim();
    if trimmed.is_empty() || trimmed.starts_with('/') {
        return None;
    }

    let normalized = trimmed.to_lowercase();

    detect_mode_change(&normalized)
        .map(NaturalLanguageCommand::Mode)
        .or_else(|| detect_think_change(&normalized).map(NaturalLanguageCommand::Think))
        .or_else(|| detect_role_change(&normalized).map(NaturalLanguageCommand::Role))
        .or_else(|| detect_model_change(trimmed, &normalized).map(NaturalLanguageCommand::Model))
}

pub fn parse_screenshot_command(input: &str) -> Option<String> {
    let trimmed = input.trim();
    if trimmed == "/ss" {
        return Some(String::new());
    }

    trimmed
        .strip_prefix("/ss ")
        .map(str::trim)
        .map(ToOwned::to_owned)
}

pub fn parse_image_command(input: &str) -> Option<(PathBuf, String)> {
    let trimmed = input.trim();
    let remainder = trimmed.strip_prefix("/image")?.trim();
    if remainder.is_empty() {
        return None;
    }

    detect_image_path(remainder)
}

pub fn detect_image_path(input: &str) -> Option<(PathBuf, String)> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return None;
    }

    if let Some((path, rest)) = extract_quoted_path(trimmed)
        && is_image_path(path)
    {
        return Some((PathBuf::from(path), rest.trim().to_string()));
    }

    for token in trimmed.split_whitespace() {
        let path = strip_surrounding_quotes(token);
        if !is_image_path(path) {
            continue;
        }

        let remaining = trimmed.replacen(token, "", 1).trim().to_string();
        return Some((PathBuf::from(path), remaining));
    }

    None
}

fn contains_any(haystack: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| haystack.contains(needle))
}

fn detect_mode_change(normalized: &str) -> Option<ExecMode> {
    let mode_cue = normalized.contains("mode") || normalized.contains("모드");
    let explicit_change = contains_any(
        normalized,
        &[
            "바꿔", "변경", "전환", "설정", "해줘", "switch", "change", "set", "turn",
        ],
    );

    if !(mode_cue || explicit_change) {
        return None;
    }

    if contains_any(
        normalized,
        &["auto mode", "automatic mode", "자동 모드", "오토 모드"],
    ) || (mode_cue && contains_any(normalized, &[" auto", "auto ", "automatic", "자동"]))
    {
        return Some(ExecMode::Auto);
    }

    if contains_any(normalized, &["safe mode", "안전 모드", "세이프 모드"])
        || (mode_cue && contains_any(normalized, &["safe", "안전"]))
    {
        return Some(ExecMode::Safe);
    }

    if contains_any(normalized, &["trust mode", "신뢰 모드", "트러스트 모드"])
        || (mode_cue && contains_any(normalized, &["trust", "신뢰"]))
    {
        return Some(ExecMode::Trust);
    }

    None
}

fn detect_think_change(normalized: &str) -> Option<ThinkLevel> {
    if contains_any(
        normalized,
        &[
            "깊게 생각",
            "깊게 생각해",
            "깊게 생각해줘",
            "deep think",
            "think deeply",
            "think harder",
            "maximum reasoning",
            "max reasoning",
        ],
    ) {
        return Some(ThinkLevel::Max);
    }

    if contains_any(
        normalized,
        &[
            "간단히 생각",
            "짧게 생각",
            "빠르게 생각",
            "think briefly",
            "minimal reasoning",
            "minimum reasoning",
            "quick reasoning",
        ],
    ) {
        return Some(ThinkLevel::Min);
    }

    if contains_any(
        normalized,
        &[
            "기본 생각",
            "보통 생각",
            "standard reasoning",
            "normal reasoning",
            "mid reasoning",
        ],
    ) {
        return Some(ThinkLevel::Mid);
    }

    None
}

fn detect_role_change(normalized: &str) -> Option<Role> {
    let role_cue = contains_any(normalized, &["역할", "role", "act as", "mode"]);
    let change_cue = contains_any(
        normalized,
        &[
            "해줘", "바꿔", "전환", "설정", "switch", "change", "set", "be ",
        ],
    );

    if !(role_cue || change_cue) {
        return None;
    }

    if contains_any(
        normalized,
        &[
            "코더 역할",
            "코더 모드",
            "coder role",
            "coding mode",
            "developer role",
        ],
    ) {
        return Some(Role::Coder);
    }
    if contains_any(
        normalized,
        &["작가 역할", "작성 모드", "writer role", "writing mode"],
    ) {
        return Some(Role::Writer);
    }
    if contains_any(
        normalized,
        &[
            "비서 역할",
            "assistant role",
            "assistant mode",
            "도우미 역할",
        ],
    ) {
        return Some(Role::Assistant);
    }
    if contains_any(
        normalized,
        &["분석가 역할", "분석 모드", "analyst role", "analysis mode"],
    ) {
        return Some(Role::Analyst);
    }

    None
}

fn detect_model_change(input: &str, normalized: &str) -> Option<String> {
    let change_cue = contains_any(
        normalized,
        &[
            "모델",
            "model",
            "switch to",
            "change to",
            "use model",
            "use ",
            "바꿔",
            "변경",
            "전환",
            "써",
            "사용",
        ],
    );
    if !change_cue {
        return None;
    }

    if let Some(after) = extract_after_keyword(input, "model")
        && let Some(model) = normalize_model_target(after)
    {
        return Some(model);
    }
    if let Some(after) = extract_after_keyword(input, "모델")
        && let Some(model) = normalize_model_target(after)
    {
        return Some(model);
    }
    if let Some(before) = extract_before_keyword(input, "모델")
        && let Some(model) = normalize_model_target(before)
    {
        return Some(model);
    }

    None
}

fn extract_after_keyword<'a>(input: &'a str, keyword: &str) -> Option<&'a str> {
    let lower = input.to_lowercase();
    let start = lower.find(keyword)?;
    let after = &input[start + keyword.len()..];
    Some(after.trim())
}

fn extract_before_keyword<'a>(input: &'a str, keyword: &str) -> Option<&'a str> {
    let start = input.find(keyword)?;
    Some(input[..start].trim())
}

fn normalize_model_target(candidate: &str) -> Option<String> {
    let trimmed = candidate
        .trim()
        .trim_matches('"')
        .trim_matches('\'')
        .trim_start_matches("to ")
        .trim_start_matches("use ")
        .trim_start_matches("the ");
    if trimmed.is_empty() {
        return None;
    }

    let sanitized = [
        "으로",
        "로",
        "로 바꿔줘",
        "로 바꿔",
        "로 변경해줘",
        "로 변경",
        "로 전환해줘",
        "로 전환",
        "switch",
        "change",
        "set",
        "please",
        "해줘",
        "바꿔줘",
        "바꿔",
        "변경해줘",
        "변경",
        "전환해줘",
        "전환",
    ]
    .iter()
    .fold(trimmed.to_string(), |value, suffix| {
        value.trim_end_matches(suffix).trim().to_string()
    });

    if sanitized.is_empty() {
        return None;
    }

    Some(
        sanitized
            .split_whitespace()
            .next()
            .unwrap_or_default()
            .trim_matches('"')
            .trim_matches('\'')
            .to_string(),
    )
}

fn extract_quoted_path(input: &str) -> Option<(&str, &str)> {
    let start = input.find('"')?;
    let rest = &input[start + 1..];
    let end = rest.find('"')?;
    let path = &rest[..end];
    let prefix = input[..start].trim();
    let suffix = rest[end + 1..].trim();

    let remaining = if prefix.is_empty() {
        suffix
    } else if suffix.is_empty() {
        prefix
    } else {
        return None;
    };

    Some((path, remaining))
}

fn strip_surrounding_quotes(value: &str) -> &str {
    value.trim_matches('"')
}

fn is_image_path(value: &str) -> bool {
    let normalized = value.to_lowercase();
    [".png", ".jpg", ".jpeg", ".gif", ".webp"]
        .iter()
        .any(|extension| normalized.ends_with(extension))
}
use std::path::PathBuf;
