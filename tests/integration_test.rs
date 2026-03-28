use forja_core::intent::{BackgroundCmd, InternalCommand, detect_intent, detect_intent_with_skills};
use forja_core::mode::{parse_slash_command, ExecMode, ModeState, Role, SlashCommand, ThinkLevel};
use forja_core::notification::{NotifyCommand, parse_notify_command};
use forja_core::prompt::assemble_system_prompt;
use forja_core::prompt::loader::{DEFAULT_BASE, PromptLoader};
use forja_core::safety::{is_dangerous_command, should_confirm_command};
use forja_core::skill_eval::{SkillAction, benchmark_skill, eval_skill, parse_skill_action};
use forja_core::skill_improve::{SuggestionPriority, suggest_improvements};
use forja_core::skill::{SkillLoader, skills_dir_from_home};
use forja_memory::{MemoryCommand, parse_memory_command};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

fn unique_temp_dir(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    std::env::temp_dir().join(format!("forja_integration_{name}_{nanos}"))
}

#[test]
fn prompt_loader_loads_custom_base_file() {
    let prompts_dir = unique_temp_dir("custom_base");
    std::fs::create_dir_all(&prompts_dir).unwrap();
    std::fs::write(prompts_dir.join("base.md"), "CUSTOM_BASE").unwrap();

    let loader = PromptLoader::new(prompts_dir.as_path());

    assert_eq!(loader.load_base("Forja", "User"), "CUSTOM_BASE");
}

#[test]
fn prompt_loader_falls_back_to_default_base() {
    let prompts_dir = unique_temp_dir("fallback_base");
    let loader = PromptLoader::new(prompts_dir.as_path());
    let expected = DEFAULT_BASE
        .replace("{assistant_name}", "Forja")
        .replace("{user_name}", "User")
        .replace("{user_title}", "User");

    assert_eq!(loader.load_base("Forja", "User"), expected);
}

#[test]
fn prompt_loader_initialization_does_not_overwrite_existing_base_file() {
    let prompts_dir = unique_temp_dir("no_overwrite");
    std::fs::create_dir_all(&prompts_dir).unwrap();
    std::fs::write(prompts_dir.join("base.md"), "USER_CUSTOM").unwrap();

    let _loader = PromptLoader::new(prompts_dir.as_path());

    assert_eq!(
        std::fs::read_to_string(prompts_dir.join("base.md")).unwrap(),
        "USER_CUSTOM"
    );
}

#[test]
fn prompt_loader_replaces_supported_base_placeholders_in_assembled_prompt() {
    let prompts_dir = unique_temp_dir("placeholders");
    std::fs::create_dir_all(&prompts_dir).unwrap();
    std::fs::write(
        prompts_dir.join("base.md"),
        "Assistant={assistant_name}; User={user_name}",
    )
    .unwrap();

    let loader = PromptLoader::new(prompts_dir.as_path());
    let mode_state = ModeState::default();
    let assistant_name = "Nova";
    let user_name = "Minji";
    let prompt = assemble_system_prompt(
        &loader,
        &mode_state,
        assistant_name,
        user_name,
        "",
        "",
        "",
        "",
        "",
        "",
        "",
    );

    assert!(prompt.contains("Assistant=Nova; User=Minji"));
}

#[test]
fn prompt_loader_initialization_creates_default_emotion_prompt() {
    let prompts_dir = unique_temp_dir("emotion_defaults");
    let loader = PromptLoader::new(prompts_dir.as_path());

    loader.ensure_default_files().unwrap();

    let emotion = std::fs::read_to_string(prompts_dir.join("emotion.md")).unwrap();
    assert!(emotion.contains("# Emotion Signals"));
    assert!(emotion.contains("late_night_detected"));
}

#[test]
fn slash_mode_commands_update_mode_state() {
    let mut mode_state = ModeState::default();

    assert_eq!(
        parse_slash_command("/mode safe"),
        Some(SlashCommand::Mode(ExecMode::Safe))
    );
    mode_state.update_exec_mode(ExecMode::Safe);
    assert_eq!(mode_state.exec_mode, ExecMode::Safe);

    assert_eq!(
        parse_slash_command("/mode auto"),
        Some(SlashCommand::Mode(ExecMode::Auto))
    );
    mode_state.update_exec_mode(ExecMode::Auto);
    assert_eq!(mode_state.exec_mode, ExecMode::Auto);

    assert_eq!(
        parse_slash_command("/mode trust"),
        Some(SlashCommand::Mode(ExecMode::Trust))
    );
    mode_state.update_exec_mode(ExecMode::Trust);
    assert_eq!(mode_state.exec_mode, ExecMode::Trust);
}

#[test]
fn slash_think_commands_update_mode_state() {
    let mut mode_state = ModeState::default();

    assert_eq!(
        parse_slash_command("/think min"),
        Some(SlashCommand::Think(ThinkLevel::Min))
    );
    mode_state.update_think_level(ThinkLevel::Min);
    assert_eq!(mode_state.think_level, ThinkLevel::Min);

    assert_eq!(
        parse_slash_command("/think mid"),
        Some(SlashCommand::Think(ThinkLevel::Mid))
    );
    mode_state.update_think_level(ThinkLevel::Mid);
    assert_eq!(mode_state.think_level, ThinkLevel::Mid);

    assert_eq!(
        parse_slash_command("/think max"),
        Some(SlashCommand::Think(ThinkLevel::Max))
    );
    mode_state.update_think_level(ThinkLevel::Max);
    assert_eq!(mode_state.think_level, ThinkLevel::Max);
}

#[test]
fn slash_role_commands_update_mode_state() {
    let mut mode_state = ModeState::default();

    assert_eq!(
        parse_slash_command("/role coder"),
        Some(SlashCommand::Role(Role::Coder))
    );
    mode_state.update_role(Role::Coder);
    assert_eq!(mode_state.role, Role::Coder);

    assert_eq!(
        parse_slash_command("/role writer"),
        Some(SlashCommand::Role(Role::Writer))
    );
    mode_state.update_role(Role::Writer);
    assert_eq!(mode_state.role, Role::Writer);

    assert_eq!(
        parse_slash_command("/role assistant"),
        Some(SlashCommand::Role(Role::Assistant))
    );
    mode_state.update_role(Role::Assistant);
    assert_eq!(mode_state.role, Role::Assistant);

    assert_eq!(
        parse_slash_command("/role analyst"),
        Some(SlashCommand::Role(Role::Analyst))
    );
    mode_state.update_role(Role::Analyst);
    assert_eq!(mode_state.role, Role::Analyst);
}

#[test]
#[ignore = "/help not yet implemented"]
fn help_command_reply_is_not_reachable_via_public_forja_core_api() {
    // `/help` is handled inside the binary's private slash-handler closure in `src/main.rs`.
    // There is no public forja_core API to exercise that branch from an integration test yet.
}

#[test]
fn exec_mode_confirmation_behavior_matches_mode_and_command_risk() {
    assert!(should_confirm_command(ExecMode::Safe, "ls -la"));
    assert!(should_confirm_command(ExecMode::Safe, "rm -rf /"));

    assert!(!should_confirm_command(ExecMode::Auto, "ls -la"));
    assert!(should_confirm_command(ExecMode::Auto, "rm -rf /"));

    assert!(!should_confirm_command(ExecMode::Trust, "ls -la"));
    assert!(!should_confirm_command(ExecMode::Trust, "rm -rf /"));
}

#[test]
fn dangerous_command_checker_matches_expected_patterns() {
    assert!(is_dangerous_command("rm -rf /"));
    assert!(!is_dangerous_command("ls -la"));
}

#[test]
fn detect_intent_maps_safe_mode_request() {
    assert_eq!(
        detect_intent("switch to safe mode"),
        Some(InternalCommand::Mode(ExecMode::Safe))
    );
}

#[test]
fn detect_intent_maps_korean_screenshot_request() {
    assert_eq!(
        detect_intent("\u{D654}\u{BA74} \u{CEA1}\u{CCD0}\u{D574}\u{C918}"),
        Some(InternalCommand::Screenshot(None))
    );
}

#[test]
fn detect_intent_maps_deep_thinking_request() {
    assert_eq!(
        detect_intent("think deeply about this"),
        Some(InternalCommand::Think(ThinkLevel::Max))
    );
}

#[test]
fn detect_intent_avoids_false_positive_for_safe_mode_discussion() {
    assert_eq!(detect_intent("I wrote code for safe mode"), None);
}

#[test]
fn detect_intent_returns_none_for_normal_chat() {
    assert_eq!(detect_intent("hello how are you"), None);
}

#[test]
fn detect_intent_maps_korean_coder_role_request() {
    assert_eq!(
        detect_intent(
            "\u{CF54}\u{B529} \u{BAA8}\u{B4DC}\u{B85C} \u{C804}\u{D658}\u{D574}\u{C918}"
        ),
        Some(InternalCommand::Role(Role::Coder))
    );
}

#[test]
fn detect_intent_maps_background_status_request() {
    assert_eq!(
        detect_intent("background status"),
        Some(InternalCommand::Background(BackgroundCmd::Status))
    );
}

#[test]
fn detect_intent_maps_tui_command() {
    assert_eq!(detect_intent("/tui"), Some(InternalCommand::Tui));
}

#[test]
fn skill_loader_parses_frontmatter_and_body() {
    let home_dir = unique_temp_dir("skill_loader_parse");
    let skill_dir = skills_dir_from_home(home_dir.as_path()).join("deploy-vercel");
    std::fs::create_dir_all(&skill_dir).unwrap();
    std::fs::write(
        skill_dir.join("SKILL.md"),
        "---\nname: deploy-vercel\ndescription: Deploy current project to Vercel\ntriggers:\n  - deploy\n  - vercel\nscripts:\n  - deploy.sh\nenv:\n  - VERCEL_TOKEN\ntests:\n  - name: basic deploy\n    input: deploy\n    expected_contains:\n      - success\n    expected_not_contains:\n      - error\n---\n\n# Deploy to Vercel\n\nRun the deploy steps.",
    )
    .unwrap();

    let mut loader = SkillLoader::new(skills_dir_from_home(home_dir.as_path()));
    let skills = loader.load_all().unwrap();

    assert_eq!(skills.len(), 1);
    assert_eq!(skills[0].name, "deploy-vercel");
    assert_eq!(skills[0].description, "Deploy current project to Vercel");
    assert_eq!(skills[0].triggers, vec!["deploy", "vercel"]);
    assert_eq!(skills[0].scripts, vec!["deploy.sh"]);
    assert_eq!(skills[0].env, vec!["VERCEL_TOKEN"]);
    assert_eq!(skills[0].tests.len(), 1);
    assert_eq!(skills[0].tests[0].name, "basic deploy");
    assert_eq!(skills[0].tests[0].input, "deploy");
    assert_eq!(skills[0].tests[0].expected_contains, vec!["success"]);
    assert_eq!(skills[0].tests[0].expected_not_contains, vec!["error"]);
    assert!(skills[0].instructions.contains("# Deploy to Vercel"));
}

#[test]
fn skill_loader_finds_skill_by_trigger() {
    let home_dir = unique_temp_dir("skill_loader_trigger");
    let skill_dir = skills_dir_from_home(home_dir.as_path()).join("deploy-vercel");
    std::fs::create_dir_all(&skill_dir).unwrap();
    std::fs::write(
        skill_dir.join("SKILL.md"),
        "---\nname: deploy-vercel\ndescription: Deploy current project\ntriggers:\n  - deploy\nscripts:\n  - deploy.sh\nenv:\n  - VERCEL_TOKEN\n---\n\nUse deploy.sh.",
    )
    .unwrap();

    let mut loader = SkillLoader::new(skills_dir_from_home(home_dir.as_path()));
    let _ = loader.load_all().unwrap();

    assert_eq!(
        loader.find_by_trigger("deploy").map(|skill| skill.name.as_str()),
        Some("deploy-vercel")
    );
}

#[test]
fn skill_loader_returns_none_for_random_chat_message() {
    let home_dir = unique_temp_dir("skill_loader_none");
    let mut loader = SkillLoader::new(skills_dir_from_home(home_dir.as_path()));
    let _ = loader.load_all().unwrap();

    assert_eq!(loader.find_by_trigger("random chat message"), None);
}

#[test]
fn internal_command_skill_variant_matches() {
    let command = InternalCommand::Skill("deploy-vercel".to_string(), "--prod".to_string());

    assert_eq!(
        command,
        InternalCommand::Skill("deploy-vercel".to_string(), "--prod".to_string())
    );
}

#[test]
fn detect_intent_with_skills_matches_skill_trigger() {
    let home_dir = unique_temp_dir("skill_intent");
    let skill_dir = skills_dir_from_home(home_dir.as_path()).join("deploy-vercel");
    std::fs::create_dir_all(&skill_dir).unwrap();
    std::fs::write(
        skill_dir.join("SKILL.md"),
        "---\nname: deploy-vercel\ndescription: Deploy current project\ntriggers:\n  - deploy\nscripts:\n  - deploy.sh\nenv:\n  - VERCEL_TOKEN\n---\n\nUse deploy.sh.",
    )
    .unwrap();

    let mut loader = SkillLoader::new(skills_dir_from_home(home_dir.as_path()));
    let _ = loader.load_all().unwrap();

    assert_eq!(
        detect_intent_with_skills("deploy", &loader),
        Some(InternalCommand::Skill(
            "deploy-vercel".to_string(),
            String::new()
        ))
    );
}

#[test]
fn skills_directory_is_created_automatically() {
    let home_dir = unique_temp_dir("skills_dir");
    let skills_dir = skills_dir_from_home(home_dir.as_path());

    let mut loader = SkillLoader::new(skills_dir.clone());
    let _ = loader.load_all().unwrap();

    assert!(skills_dir.exists());
}

#[test]
fn parse_memory_command_matches_supported_variants() {
    assert_eq!(parse_memory_command("/memory"), Some(MemoryCommand::Stats));
    assert_eq!(
        parse_memory_command("/memory search deploy"),
        Some(MemoryCommand::Search("deploy".to_string()))
    );
    assert_eq!(
        parse_memory_command("/memory clear session"),
        Some(MemoryCommand::ClearSession)
    );
    assert_eq!(parse_memory_command("/memory flush"), Some(MemoryCommand::Flush));
}

#[test]
fn skill_eval_counts_pass_and_fail_cases() {
    let skill = forja_core::skill::Skill {
        name: "demo".to_string(),
        description: "Demo".to_string(),
        triggers: vec!["demo".to_string()],
        scripts: vec!["demo.sh".to_string()],
        env: Vec::new(),
        instructions: "demo".to_string(),
        base_dir: unique_temp_dir("skill_eval_counts"),
        tests: vec![
            forja_core::skill_eval::SkillTestCase {
                name: "pass".to_string(),
                input: "demo".to_string(),
                expected_contains: vec!["success".to_string()],
                expected_not_contains: vec!["error".to_string()],
                timeout_secs: 30,
            },
            forja_core::skill_eval::SkillTestCase {
                name: "fail".to_string(),
                input: "demo".to_string(),
                expected_contains: vec!["missing".to_string()],
                expected_not_contains: Vec::new(),
                timeout_secs: 30,
            },
        ],
    };

    let result = eval_skill(&skill, &skill.tests, |_skill, _input, _timeout_secs| {
        Ok("success output".to_string())
    });

    assert_eq!(result.passed, 1);
    assert_eq!(result.failed, 1);
    assert_eq!(result.results.len(), 2);
}

#[test]
fn skill_improve_generates_timeout_and_error_suggestions() {
    let skill = forja_core::skill::Skill {
        name: "demo".to_string(),
        description: "Demo".to_string(),
        triggers: vec!["demo".to_string()],
        scripts: vec!["demo.sh".to_string()],
        env: Vec::new(),
        instructions: "demo".to_string(),
        base_dir: unique_temp_dir("skill_improve"),
        tests: Vec::new(),
    };
    let eval_result = forja_core::skill_eval::EvalResult {
        passed: 0,
        failed: 2,
        results: vec![
            forja_core::skill_eval::TestResult {
                case_name: "timeout".to_string(),
                passed: false,
                actual_output: String::new(),
                failure_reason: Some("timeout".to_string()),
            },
            forja_core::skill_eval::TestResult {
                case_name: "error".to_string(),
                passed: false,
                actual_output: "fatal error".to_string(),
                failure_reason: Some("expected_not_contains matched: error".to_string()),
            },
        ],
        duration_ms: 100,
    };

    let suggestions = suggest_improvements(&skill, &eval_result);

    assert_eq!(suggestions.len(), 2);
    assert!(suggestions
        .iter()
        .any(|suggestion| suggestion.priority == SuggestionPriority::High));
}

#[test]
fn skill_benchmark_aggregates_pass_rate_and_duration() {
    let skill = forja_core::skill::Skill {
        name: "demo".to_string(),
        description: "Demo".to_string(),
        triggers: vec!["demo".to_string()],
        scripts: vec!["demo.sh".to_string()],
        env: Vec::new(),
        instructions: "demo".to_string(),
        base_dir: unique_temp_dir("skill_benchmark"),
        tests: vec![forja_core::skill_eval::SkillTestCase {
            name: "pass".to_string(),
            input: "demo".to_string(),
            expected_contains: vec!["success".to_string()],
            expected_not_contains: Vec::new(),
            timeout_secs: 30,
        }],
    };

    let benchmark = benchmark_skill(&skill, &skill.tests, 3, |_skill, _input, _timeout_secs| {
        Ok("success output".to_string())
    });

    assert_eq!(benchmark.run_count, 3);
    assert!(benchmark.pass_rate >= 1.0);
    assert!(benchmark.avg_duration_ms <= benchmark.max_duration_ms);
}

#[test]
fn parse_skill_action_matches_eval_improve_and_benchmark_commands() {
    assert_eq!(
        parse_skill_action("/skill eval git-summary"),
        Some(SkillAction::Eval("git-summary".to_string()))
    );
    assert_eq!(
        parse_skill_action("/skill improve git-summary"),
        Some(SkillAction::Improve("git-summary".to_string()))
    );
    assert_eq!(
        parse_skill_action("/skill benchmark git-summary 5"),
        Some(SkillAction::Benchmark {
            name: "git-summary".to_string(),
            runs: 5,
        })
    );
}

#[test]
fn parse_notify_command_matches_runtime_commands() {
    assert_eq!(parse_notify_command("/notify test"), Some(NotifyCommand::Test));
    assert_eq!(parse_notify_command("/notify off"), Some(NotifyCommand::Off));
    assert_eq!(parse_notify_command("/notify on"), Some(NotifyCommand::On));
    assert_eq!(parse_notify_command("/notify status"), Some(NotifyCommand::Status));
    assert_eq!(
        parse_notify_command("/notify history 5"),
        Some(NotifyCommand::History(5))
    );
}
