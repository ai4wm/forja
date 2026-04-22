use forja_core::mode::{
    ExecMode, ModeState, Role, SlashCommand, ThinkLevel, detect_image_path, detect_role,
    parse_image_command, parse_natural_language_command, parse_screenshot_command,
    parse_slash_command,
};
use forja_core::prompt::base::base_prompt;
use forja_core::prompt::think::think_prompt;
use forja_core::prompt::{assemble_system_prompt, join_prompt_sections};
use forja_tools::confirm::StdinConfirmation;

#[test]
fn detect_role_prefers_coder_keywords_first() {
    assert_eq!(detect_role("코드 에러 수정해줘"), Role::Coder);
}

#[test]
fn detect_role_matches_writer_keywords() {
    assert_eq!(detect_role("블로그 글 써줘"), Role::Writer);
}

#[test]
fn detect_role_matches_assistant_keywords() {
    assert_eq!(detect_role("내일 미팅 잡아줘"), Role::Assistant);
}

#[test]
fn detect_role_matches_analyst_keywords() {
    assert_eq!(detect_role("가격 비교해줘"), Role::Analyst);
}

#[test]
fn detect_role_returns_default_when_no_keyword_matches() {
    assert_eq!(detect_role("안녕"), Role::Default);
}

#[test]
fn parse_slash_command_mode_trust() {
    assert_eq!(
        parse_slash_command("/mode trust"),
        Some(SlashCommand::Mode(ExecMode::Trust))
    );
}

#[test]
fn parse_slash_command_think_max() {
    assert_eq!(
        parse_slash_command("/think max"),
        Some(SlashCommand::Think(ThinkLevel::Max))
    );
}

#[test]
fn parse_slash_command_role_coder() {
    assert_eq!(
        parse_slash_command("/role coder"),
        Some(SlashCommand::Role(Role::Coder))
    );
}

#[test]
fn parse_slash_command_role_auto() {
    assert_eq!(
        parse_slash_command("/role auto"),
        Some(SlashCommand::Role(Role::Auto))
    );
}

#[test]
fn parse_slash_command_ignores_normal_message() {
    assert_eq!(parse_slash_command("normal message"), None);
}

#[test]
fn parse_slash_command_ignores_invalid_command() {
    assert_eq!(parse_slash_command("/invalid"), None);
}

#[test]
fn think_prompt_min_contains_concise() {
    assert!(think_prompt(ThinkLevel::Min).contains("concise"));
}

#[test]
fn think_prompt_mid_is_empty() {
    assert_eq!(think_prompt(ThinkLevel::Mid), "");
}

#[test]
fn think_prompt_max_contains_thoroughly() {
    assert!(think_prompt(ThinkLevel::Max).contains("thoroughly"));
}

#[test]
fn assemble_system_prompt_includes_base_prompt() {
    let mode_state = ModeState::default();
    let prompt = assemble_system_prompt(
        &mode_state,
        "Forja",
        "사용자님",
        "[identity]\nidentity",
        "[user]\nuser",
        "[tools]\ntools",
        "",
        "",
        "",
        "",
    );

    assert!(prompt.contains("You are Forja, a personal AI assistant."));
    assert!(base_prompt("Forja", "사용자님").contains("Address the user as \"사용자님\""));
}

#[test]
fn assemble_system_prompt_includes_think_prompt_when_not_mid() {
    let mut mode_state = ModeState::default();
    mode_state.update_think_level(ThinkLevel::Max);
    let prompt =
        assemble_system_prompt(&mode_state, "Forja", "사용자님", "", "", "", "", "", "", "");

    assert!(prompt.contains("Think extremely thoroughly before responding."));
}

#[test]
fn assemble_system_prompt_includes_role_prompt_when_role_detected() {
    let mut mode_state = ModeState::default();
    mode_state.update_role(Role::Auto);
    mode_state.update_detected_role(Role::Coder);
    let prompt =
        assemble_system_prompt(&mode_state, "Forja", "사용자님", "", "", "", "", "", "", "");

    assert!(prompt.contains("## Coding Mode Active"));
}

#[test]
fn assemble_system_prompt_respects_section_order() {
    let mut mode_state = ModeState::default();
    mode_state.update_think_level(ThinkLevel::Min);
    mode_state.update_role(Role::Writer);

    let prompt = assemble_system_prompt(
        &mode_state,
        "Forja",
        "사용자님",
        "[identity]",
        "[user]",
        "[tools]",
        "[emotion]",
        "[relationship]",
        "[knowledge]",
        "[memory]",
    );

    let base_index = prompt.find("You are Forja").unwrap();
    let think_index = prompt.find("Be concise.").unwrap();
    let role_index = prompt.find("## Writing Mode Active").unwrap();
    let tools_index = prompt.find("[tools]").unwrap();
    let emotion_index = prompt.find("[emotion]").unwrap();
    let relationship_index = prompt.find("[relationship]").unwrap();
    let knowledge_index = prompt.find("[knowledge]").unwrap();
    let memory_index = prompt.find("[memory]").unwrap();

    assert!(base_index < think_index);
    assert!(think_index < role_index);
    assert!(role_index < tools_index);
    assert!(tools_index < emotion_index);
    assert!(emotion_index < relationship_index);
    assert!(relationship_index < knowledge_index);
    assert!(knowledge_index < memory_index);
}

#[test]
fn join_prompt_sections_skips_empty_values() {
    let joined = join_prompt_sections(["alpha", "", "  ", "beta"], "\n\n");

    assert_eq!(joined, "alpha\n\nbeta");
}

#[test]
fn mode_labels_come_from_core_state_types() {
    assert_eq!(ExecMode::Safe.as_str(), "safe");
    assert_eq!(ThinkLevel::Max.as_str(), "max");
    assert_eq!(Role::Assistant.as_str(), "assistant");
}

#[test]
fn exec_mode_safe_triggers_confirmation_for_safe_commands() {
    let confirmation = StdinConfirmation::new(ExecMode::Safe);

    assert!(confirmation.should_confirm(false));
}

#[test]
fn exec_mode_trust_skips_confirmation_for_dangerous_commands() {
    let confirmation = StdinConfirmation::new(ExecMode::Trust);

    assert!(!confirmation.should_confirm(true));
}

#[test]
fn exec_mode_auto_triggers_confirmation_only_for_dangerous_commands() {
    let confirmation = StdinConfirmation::new(ExecMode::Auto);

    assert!(!confirmation.should_confirm(false));
    assert!(confirmation.should_confirm(true));
}

#[test]
fn cli_process_line_single_line_returns_complete() {
    let mut buffer = String::new();

    let continues = forja_channel::cli::process_line("hello", &mut buffer);

    assert!(!continues);
    assert_eq!(buffer, "hello");
}

#[test]
fn cli_process_line_multiline_two_lines() {
    let mut buffer = String::new();

    let first = forja_channel::cli::process_line("hello\\", &mut buffer);
    let second = forja_channel::cli::process_line("world", &mut buffer);

    assert!(first);
    assert!(!second);
    assert_eq!(buffer, "hello\nworld");
}

#[test]
fn cli_process_line_multiline_three_lines() {
    let mut buffer = String::new();

    assert!(forja_channel::cli::process_line("a\\", &mut buffer));
    assert!(forja_channel::cli::process_line("b\\", &mut buffer));
    assert!(!forja_channel::cli::process_line("c", &mut buffer));
    assert_eq!(buffer, "a\nb\nc");
}

#[test]
fn cli_process_line_empty_input_returns_empty_string() {
    let mut buffer = String::new();

    let continues = forja_channel::cli::process_line("", &mut buffer);

    assert!(!continues);
    assert_eq!(buffer, "");
}

#[test]
fn cli_process_line_backslash_only_adds_newline() {
    let mut buffer = String::new();

    assert!(forja_channel::cli::process_line("\\", &mut buffer));
    assert!(!forja_channel::cli::process_line("next", &mut buffer));
    assert_eq!(buffer, "\nnext");
}

#[test]
fn detect_image_path_windows_path_with_prompt() {
    let detected = detect_image_path(r#"C:\screenshot.png 이거 뭐야"#).unwrap();

    assert_eq!(detected.0.to_string_lossy(), r#"C:\screenshot.png"#);
    assert_eq!(detected.1, "이거 뭐야");
}

#[test]
fn detect_image_path_quoted_path_with_spaces() {
    let detected = detect_image_path(r#""C:\my folder\img.png" explain"#).unwrap();

    assert_eq!(detected.0.to_string_lossy(), r#"C:\my folder\img.png"#);
    assert_eq!(detected.1, "explain");
}

#[test]
fn detect_image_path_unix_path_only() {
    let detected = detect_image_path("/home/user/photo.jpg").unwrap();

    assert_eq!(detected.0.to_string_lossy(), "/home/user/photo.jpg");
    assert_eq!(detected.1, "");
}

#[test]
fn detect_image_path_returns_none_for_normal_text() {
    assert_eq!(detect_image_path("normal text message"), None);
}

#[test]
fn detect_image_path_returns_none_for_non_image_extension() {
    assert_eq!(detect_image_path("test.txt"), None);
}

#[test]
fn parse_screenshot_command_with_prompt() {
    assert_eq!(
        parse_screenshot_command("/ss 이 에러 뭐야?"),
        Some("이 에러 뭐야?".to_string())
    );
}

#[test]
fn parse_image_command_splits_path_and_prompt() {
    let parsed = parse_image_command(r#"/image "C:\my folder\img.png" explain this"#).unwrap();

    assert_eq!(parsed.0.to_string_lossy(), r#"C:\my folder\img.png"#);
    assert_eq!(parsed.1, "explain this");
}

#[test]
fn parse_natural_language_command_maps_korean_mode_request() {
    let mapped = parse_natural_language_command("자동 모드로 바꿔줘");

    assert_eq!(
        mapped,
        Some(forja_core::mode::NaturalLanguageCommand::Mode(
            ExecMode::Auto
        ))
    );
}

#[test]
fn parse_natural_language_command_maps_english_think_request() {
    let mapped = parse_natural_language_command("think deeply");

    assert_eq!(
        mapped,
        Some(forja_core::mode::NaturalLanguageCommand::Think(
            ThinkLevel::Max
        ))
    );
}

#[test]
fn parse_natural_language_command_maps_korean_role_request() {
    let mapped = parse_natural_language_command("코더 역할로 해줘");

    assert_eq!(
        mapped,
        Some(forja_core::mode::NaturalLanguageCommand::Role(Role::Coder))
    );
}

#[test]
fn parse_natural_language_command_maps_model_request() {
    let mapped = parse_natural_language_command("llama3 모델로 바꿔줘");

    assert_eq!(
        mapped,
        Some(forja_core::mode::NaturalLanguageCommand::Model(
            "llama3".to_string()
        ))
    );
}
