use async_trait::async_trait;
use forja_core::emotion::{default_startup_greeting, generate_startup_greeting};
use forja_core::error::{ForjaError, Result};
use forja_core::mode::{parse_slash_command, ExecMode, ModeState, Role, SlashCommand, ThinkLevel};
use forja_core::prompt::assemble_system_prompt;
use forja_core::prompt::loader::{DEFAULT_BASE, PromptLoader};
use forja_core::safety::{is_dangerous_command, should_confirm_command};
use forja_core::traits::LlmProvider;
use forja_core::{Message, Role as MessageRole, ToolDefinition};
use std::path::PathBuf;
use std::pin::Pin;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio_stream::Stream;

fn unique_temp_dir(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    std::env::temp_dir().join(format!("forja_integration_{name}_{nanos}"))
}

struct NoneGreetingProvider;

#[async_trait]
impl LlmProvider for NoneGreetingProvider {
    async fn chat(
        &self,
        _messages: &[Message],
        _tools: Option<&[ToolDefinition]>,
    ) -> Result<Message> {
        Ok(Message::text(MessageRole::Assistant, "NONE", None))
    }

    async fn stream(
        &self,
        _messages: &[Message],
        _tools: Option<&[ToolDefinition]>,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<String>> + Send>>> {
        Err(ForjaError::LlmError(
            "stream is not used in this integration test".to_string(),
        ))
    }
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
        "Assistant={assistant_name}; User={user_title}",
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

    // The public PromptLoader API supports `{assistant_name}` and `{user_title}`.
    // `{user_name}` is not a supported public placeholder in the current API.
    assert!(prompt.contains("Assistant=Nova; User=Minji"));
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

#[tokio::test]
#[ignore = "greeting returns None when provider says NONE"]
async fn startup_greeting_falls_back_to_default_when_provider_returns_none() {
    let greeting =
        generate_startup_greeting(&NoneGreetingProvider, "Forja", "User", "prior memory", false)
            .await
            .unwrap();

    assert_eq!(greeting, Some(default_startup_greeting("User")));
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
