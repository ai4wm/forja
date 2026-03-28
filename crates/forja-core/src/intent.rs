use crate::mode::{ExecMode, Role, ThinkLevel};
use crate::skill::SkillLoader;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InternalCommand {
    Mode(ExecMode),
    Think(ThinkLevel),
    Role(Role),
    Screenshot(Option<String>),
    Help,
    Models,
    Model(String),
    Background(BackgroundCmd),
    Skill(String, String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BackgroundCmd {
    Status,
    Off,
    Auto,
}

pub fn detect_intent(input: &str) -> Option<InternalCommand> {
    let normalized = normalize(input);
    if normalized.is_empty() {
        return None;
    }

    detect_mode(&normalized)
        .or_else(|| detect_think(&normalized))
        .or_else(|| detect_role(&normalized))
        .or_else(|| detect_screenshot(&normalized))
        .or_else(|| detect_help(&normalized))
        .or_else(|| detect_models(&normalized))
        .or_else(|| detect_background(&normalized))
        .or_else(|| detect_model(&normalized))
}

pub fn detect_intent_with_skills(
    input: &str,
    skill_loader: &SkillLoader,
) -> Option<InternalCommand> {
    detect_intent(input).or_else(|| detect_skill(input, skill_loader))
}

fn detect_mode(input: &str) -> Option<InternalCommand> {
    if starts_with_any(
        input,
        &[
            "switch to safe mode",
            "set safe mode",
            "use safe mode",
            "enable safe mode",
            "confirm everything",
            "\u{C548}\u{C804} \u{BAA8}\u{B4DC}",
            "\u{C548}\u{C804}\u{D558}\u{AC8C}",
        ],
    ) {
        return Some(InternalCommand::Mode(ExecMode::Safe));
    }

    if starts_with_any(
        input,
        &[
            "switch to trust mode",
            "set trust mode",
            "use trust mode",
            "trust me",
            "no confirmations",
            "\u{C2E0}\u{B8B0} \u{BAA8}\u{B4DC}",
        ],
    ) {
        return Some(InternalCommand::Mode(ExecMode::Trust));
    }

    if starts_with_any(
        input,
        &[
            "switch to auto mode",
            "set auto mode",
            "use auto mode",
            "default mode",
            "\u{C790}\u{B3D9} \u{BAA8}\u{B4DC}",
            "\u{AE30}\u{BCF8} \u{BAA8}\u{B4DC}",
        ],
    ) {
        return Some(InternalCommand::Mode(ExecMode::Auto));
    }

    None
}

fn detect_think(input: &str) -> Option<InternalCommand> {
    if starts_with_any(
        input,
        &[
            "think deeply",
            "deep reasoning",
            "think max",
            "\u{C790}\u{C138}\u{D788}",
            "\u{AE4A}\u{AC8C}",
        ],
    ) {
        return Some(InternalCommand::Think(ThinkLevel::Max));
    }

    if starts_with_any(
        input,
        &[
            "quick answer",
            "brief",
            "think min",
            "\u{AC04}\u{B2E8}\u{D788}",
            "\u{BE68}\u{B9AC}",
        ],
    ) {
        return Some(InternalCommand::Think(ThinkLevel::Min));
    }

    if starts_with_any(
        input,
        &[
            "normal thinking",
            "think mid",
            "\u{BCF4}\u{D1B5}\u{C73C}\u{B85C}",
        ],
    ) {
        return Some(InternalCommand::Think(ThinkLevel::Mid));
    }

    None
}

fn detect_role(input: &str) -> Option<InternalCommand> {
    if starts_with_any(
        input,
        &[
            "coding mode",
            "write code",
            "\u{CF54}\u{B529} \u{BAA8}\u{B4DC}",
            "\u{CF54}\u{B529}\u{D574}\u{C918}",
        ],
    ) {
        return Some(InternalCommand::Role(Role::Coder));
    }

    if starts_with_any(
        input,
        &[
            "writing mode",
            "write for me",
            "\u{AE00}\u{C4F0}\u{AE30} \u{BAA8}\u{B4DC}",
        ],
    ) {
        return Some(InternalCommand::Role(Role::Writer));
    }

    if starts_with_any(
        input,
        &[
            "analyze this",
            "analysis mode",
            "\u{BD84}\u{C11D}\u{D574}\u{C918}",
            "\u{BD84}\u{C11D} \u{BAA8}\u{B4DC}",
        ],
    ) {
        return Some(InternalCommand::Role(Role::Analyst));
    }

    if starts_with_any(
        input,
        &[
            "assistant mode",
            "\u{BE44}\u{C11C} \u{BAA8}\u{B4DC}",
        ],
    ) {
        return Some(InternalCommand::Role(Role::Assistant));
    }

    None
}

fn detect_screenshot(input: &str) -> Option<InternalCommand> {
    for prefix in [
        "capture screen",
        "screenshot",
        "\u{D654}\u{BA74} \u{CEA1}\u{CCD0}",
        "\u{C2A4}\u{D06C}\u{B9B0}\u{C0F7}",
    ] {
        if let Some(rest) = strip_prefix_phrase(input, prefix) {
            let prompt = normalize_optional_prompt(rest);
            return Some(InternalCommand::Screenshot(prompt));
        }
    }

    None
}

fn detect_help(input: &str) -> Option<InternalCommand> {
    if matches_any(
        input,
        &[
            "help",
            "what can you do",
            "\u{B3C4}\u{C6C0}\u{B9D0}",
            "\u{BB50} \u{D560} \u{C218} \u{C788}\u{C5B4}",
        ],
    ) {
        return Some(InternalCommand::Help);
    }

    None
}

fn detect_models(input: &str) -> Option<InternalCommand> {
    if matches_any(
        input,
        &[
            "show models",
            "what models",
            "\u{BAA8}\u{B378} \u{BAA9}\u{B85D}",
            "\u{C0AC}\u{C6A9} \u{AC00}\u{B2A5}\u{D55C} \u{BAA8}\u{B378}",
        ],
    ) {
        return Some(InternalCommand::Models);
    }

    None
}

fn detect_background(input: &str) -> Option<InternalCommand> {
    if matches_any(
        input,
        &[
            "background status",
            "\u{BC31}\u{ADF8}\u{B77C}\u{C6B4}\u{B4DC} \u{C0C1}\u{D0DC}",
        ],
    ) {
        return Some(InternalCommand::Background(BackgroundCmd::Status));
    }

    if matches_any(
        input,
        &[
            "stop background",
            "background off",
            "\u{BC31}\u{ADF8}\u{B77C}\u{C6B4}\u{B4DC} \u{B044}\u{AE30}",
        ],
    ) {
        return Some(InternalCommand::Background(BackgroundCmd::Off));
    }

    if matches_any(
        input,
        &[
            "background auto",
            "restart background auto",
            "\u{BC31}\u{ADF8}\u{B77C}\u{C6B4}\u{B4DC} \u{C790}\u{B3D9}",
        ],
    ) {
        return Some(InternalCommand::Background(BackgroundCmd::Auto));
    }

    None
}

fn detect_model(input: &str) -> Option<InternalCommand> {
    for prefix in [
        "switch to ",
        "use ",
        "set model to ",
        "\u{B85C} \u{BC14}\u{AFD4}",
    ] {
        if let Some(model) = extract_model_after_prefix(input, prefix) {
            return Some(InternalCommand::Model(model));
        }
    }

    if let Some(model) = extract_model_before_suffix(
        input,
        &[
            "\u{B85C} \u{BC14}\u{AFD4}",
            "\u{B85C} \u{C804}\u{D658}",
        ],
    ) {
        return Some(InternalCommand::Model(model));
    }

    None
}

fn detect_skill(input: &str, skill_loader: &SkillLoader) -> Option<InternalCommand> {
    let normalized = normalize(input);
    skill_loader.find_by_trigger(&normalized).and_then(|skill| {
        skill.triggers.iter().find_map(|trigger| {
            let trigger = normalize(trigger);
            if normalized == trigger {
                return Some(InternalCommand::Skill(skill.name.clone(), String::new()));
            }
            normalized
                .strip_prefix(&format!("{trigger} "))
                .map(str::trim)
                .map(|args| InternalCommand::Skill(skill.name.clone(), args.to_string()))
        })
    })
}

fn normalize(input: &str) -> String {
    input
        .trim()
        .to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn matches_any(input: &str, patterns: &[&str]) -> bool {
    patterns.contains(&input)
}

fn starts_with_any(input: &str, patterns: &[&str]) -> bool {
    patterns.iter().any(|pattern| {
        input == *pattern
            || input.starts_with(&format!("{pattern} "))
            || input.starts_with(&format!("{pattern}\u{B85C}"))
    })
}

fn strip_prefix_phrase<'a>(input: &'a str, prefix: &str) -> Option<&'a str> {
    if input == prefix {
        return Some("");
    }

    input.strip_prefix(prefix)
}

fn normalize_optional_prompt(rest: &str) -> Option<String> {
    let trimmed = rest
        .trim()
        .trim_start_matches("and ")
        .trim_start_matches("please ")
        .trim();

    if matches!(
        trimmed,
        "\u{D574}\u{C918}"
            | "\u{D574}\u{C8FC}\u{C138}\u{C694}"
            | "\u{D574}\u{C918}\u{C694}"
    ) {
        return None;
    }

    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn extract_model_after_prefix(input: &str, prefix: &str) -> Option<String> {
    let rest = input.strip_prefix(prefix)?.trim();
    if rest.is_empty() {
        return None;
    }

    if looks_like_non_model_command(rest) || !looks_like_model_target(rest) {
        return None;
    }

    Some(rest.to_string())
}

fn extract_model_before_suffix(input: &str, suffixes: &[&str]) -> Option<String> {
    for suffix in suffixes {
        if let Some(candidate) = input.strip_suffix(suffix) {
            let model = candidate.trim();
            if model.is_empty()
                || looks_like_non_model_command(model)
                || !looks_like_model_target(model)
            {
                continue;
            }
            return Some(model.to_string());
        }
    }

    None
}

fn looks_like_non_model_command(value: &str) -> bool {
    starts_with_any(
        value,
        &[
            "safe mode",
            "trust mode",
            "auto mode",
            "coding mode",
            "writing mode",
            "analysis mode",
            "\u{C548}\u{C804} \u{BAA8}\u{B4DC}",
            "\u{C2E0}\u{B8B0} \u{BAA8}\u{B4DC}",
            "\u{C790}\u{B3D9} \u{BAA8}\u{B4DC}",
            "\u{CF54}\u{B529} \u{BAA8}\u{B4DC}",
            "\u{AE00}\u{C4F0}\u{AE30} \u{BAA8}\u{B4DC}",
            "\u{BD84}\u{C11D} \u{BAA8}\u{B4DC}",
        ],
    )
}

fn looks_like_model_target(value: &str) -> bool {
    [
        "claude",
        "deepseek",
        "gemini",
        "glm",
        "gpt",
        "grok",
        "groq",
        "kimi",
        "llama",
        "mistral",
        "ollama",
        "openrouter",
        "qwen",
    ]
    .iter()
    .any(|token| value.contains(token))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn screenshot_prompt_is_extracted_from_prefix_request() {
        assert_eq!(
            detect_intent("capture screen and analyze the error"),
            Some(InternalCommand::Screenshot(Some(
                "analyze the error".to_string()
            )))
        );
    }

    #[test]
    fn safe_mode_requires_request_context() {
        assert_eq!(detect_intent("I wrote code for safe mode"), None);
        assert_eq!(detect_intent("safe"), None);
    }
}
