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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlashCommand {
    Mode(ExecMode),
    Think(ThinkLevel),
    Role(Role),
}

pub fn detect_role(message: &str) -> Role {
    let normalized = message.to_lowercase();

    if contains_any(
        &normalized,
        &[
            "코드",
            "에러",
            "버그",
            "cargo",
            "git",
            "함수",
            "컴파일",
            "빌드",
            "테스트",
            "구현",
            "리팩토링",
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
            "글",
            "블로그",
            "기사",
            "제목",
            "문단",
            "요약",
            "작성",
            "원고",
            "에세이",
            "카피",
            "콘텐츠",
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
            "일정",
            "미팅",
            "알림",
            "할일",
            "메모",
            "약속",
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
            "비교",
            "분석",
            "조사",
            "리서치",
            "통계",
            "가격",
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
