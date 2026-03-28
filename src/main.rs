mod background_runtime;
mod bootstrap;
mod config;
mod oauth;
mod provider_registry;

use async_trait::async_trait;
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
use forja_core::emotion::EmotionEngine;
use forja_core::error::{ForjaError, Result};
use forja_core::intent::{BackgroundCmd, InternalCommand, detect_intent_with_skills};
use forja_core::mode::{
    ExecMode, ModeState, Role as ModeRole, SlashCommand, ThinkLevel, detect_image_path,
    parse_image_command, parse_screenshot_command, parse_slash_command,
};
use forja_core::prompt::loader::{PromptLoader, install_prompt_loader};
use forja_core::skill::{
    Skill, SkillLoader, clear_active_skill_context, default_skills_dir, set_active_skill_context,
    set_skill_catalog_summary,
};
use forja_core::skill_eval::{
    BenchmarkResult, EvalResult, SkillAction, benchmark_skill, eval_skill, parse_skill_action,
};
use forja_core::skill_improve::{Suggestion, SuggestionPriority, suggest_improvements};
use forja_core::traits::{Channel as CoreChannelTrait, LlmProvider, MemoryStore, Tool};
use forja_core::{
    BackgroundManager, Channel, Content, Engine, KnowledgeManager, Message, Role,
    SerendipityEngine, ToolDefinition,
};
use forja_llm::LlmClient;
use forja_memory::{
    MemoryCommand, MemoryManagerStore, default_memory_base_dir, parse_memory_command,
};
#[cfg(feature = "vision")]
use forja_tools::XcapBackend;
use forja_tools::confirm::ConfirmationHandler;
use forja_tools::{
    BrowserTool, ClaudeCodeTool, CodexTool, FileTool, GeminiCliTool, GptVisionAnalyzer, InputTool,
    MockCaptureBackend, MockVisionAnalyzer, SearchProvider, SearchTool, ShellTool,
    StdinConfirmation, VisionAnalyzer, VisionTool, WebTool, browser::MockBrowserBackend,
};
use provider_registry::ProviderRegistry;
use serde_json::json;
use std::collections::HashMap;
use std::io::Write;
use std::path::Path;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio_stream::{Stream, StreamExt};

// Mock LLM used for local testing without a real API key.

struct MockLlmProvider;

#[async_trait]
impl LlmProvider for MockLlmProvider {
    async fn chat(
        &self,
        messages: &[Message],
        _tools: Option<&[ToolDefinition]>,
    ) -> Result<Message> {
        let last = messages
            .iter()
            .rev()
            .find(|m| m.role == Role::User)
            .map(|m| match &m.content {
                Content::Text { text, .. } => text.clone(),
                _ => "(no text)".to_string(),
            })
            .unwrap_or_default();

        if last.contains(
            "Analyze the emotional state of the conversation below and respond with JSON only.",
        ) {
            return Ok(Message::text(
                Role::Assistant,
                r#"{"mood":"neutral","intensity":1,"reason":"mock mode","tone_instruction":"Reply in a balanced, respectful tone."}"#,
                None,
            ));
        }

        if last.contains("Write one natural greeting sentence.") {
            return Ok(Message::text(Role::Assistant, "NONE", None));
        }

        if last.contains("Also, if there are unfinished tasks or a useful daily summary") {
            return Ok(Message::text(Role::Assistant, "NONE", None));
        }

        if last.contains("Summarize the daily memory.md records in max 3 lines.") {
            return Ok(Message::text(Role::Assistant, "Mock summary", None));
        }

        if last.contains("Below is the user's recent memory and knowledge base.") {
            return Ok(Message::text(Role::Assistant, "NONE", None));
        }

        Ok(Message::text(
            Role::Assistant,
            format!(
                "[MockLLM] Received message: '{}' (configure a real API key to get a live response.)",
                last
            ),
            None,
        ))
    }

    async fn stream(
        &self,
        messages: &[Message],
        _tools: Option<&[ToolDefinition]>,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<String>> + Send>>> {
        let last = messages
            .iter()
            .rev()
            .find(|m| m.role == Role::User)
            .map(|m| match &m.content {
                Content::Text { text, .. } => text.clone(),
                _ => "(no text)".to_string(),
            })
            .unwrap_or_default();

        let response = format!(
            "[MockStream] Received message: '{}' (streaming effect test...)",
            last
        );

        // Split into word-level tokens to simulate streaming
        let tokens: Vec<String> = response.split(' ').map(|s| format!("{} ", s)).collect();

        let stream = tokio_stream::iter(tokens).map(Ok);
        Ok(Box::pin(stream))
    }
}

struct MemoryAwareChannel {
    inner: Arc<dyn Channel>,
    memory_store: MemoryManagerStore,
}

impl MemoryAwareChannel {
    fn new(inner: Arc<dyn Channel>, memory_store: MemoryManagerStore) -> Self {
        Self { inner, memory_store }
    }
}

#[async_trait]
impl CoreChannelTrait for MemoryAwareChannel {
    async fn receive(&self) -> Result<Message> {
        let message = self.inner.receive().await?;
        if let Content::Text { text, .. } = &message.content
            && message.role == Role::User
        {
            self.memory_store.set_current_query(text.clone());
        }
        Ok(message)
    }

    async fn send(&self, message: Message) -> Result<()> {
        self.inner.send(message).await
    }

    async fn confirm(&self, message: &str) -> Result<bool> {
        self.inner.confirm(message).await
    }

    fn is_cli_source(&self) -> bool {
        self.inner.is_cli_source()
    }

    async fn cancel_typing(&self) {
        self.inner.cancel_typing().await;
    }
}

// Banner

fn print_banner(provider_info: &str) {
    let banner = r#"
    ╔═══════════════════════════════════════╗
    ║                                       ║
    ║     ⚒️  F O R J A                      ║
    ║     Lightweight AI Agent Engine       ║
    ║     v0.1.0                            ║
    ║                                       ║
    ╚═══════════════════════════════════════╝"#;
    println!("{}", banner);
    println!("    {}\n", provider_info);
}

// Utilities: prompt loading

/// Load the project prompt file with priority: AGENTS.md -> FORJA.md -> CLAUDE.md.
fn load_project_prompt() -> Option<(String, String)> {
    let candidates = ["AGENTS.md", "FORJA.md", "CLAUDE.md"];
    for file in candidates.iter() {
        if let Ok(content) = std::fs::read_to_string(file)
            && !content.trim().is_empty()
        {
            return Some((file.to_string(), content.trim().to_string()));
        }
    }
    None
}

fn build_system_prompt(
    bootstrap_paths: &bootstrap::BootstrapPaths,
) -> std::io::Result<(String, Option<String>)> {
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    let bootstrap_prompt = bootstrap::compose_system_prompt_prefix(bootstrap_paths)?;
    let project_prompt = load_project_prompt();
    let loaded_project_file = project_prompt
        .as_ref()
        .map(|(file_name, _)| file_name.clone());

    let mut combined_prompt = String::new();

    if !bootstrap_prompt.trim().is_empty() {
        combined_prompt.push_str(&bootstrap_prompt);
    }

    if let Some((_, project_content)) = project_prompt {
        if !combined_prompt.is_empty() {
            combined_prompt.push_str("\n\n---\n\n");
        }
        combined_prompt.push_str(&project_content);
    }

    if !combined_prompt.is_empty() {
        combined_prompt.push_str(&format!(
            "\n\nToday's date is {today}. This date is correct. If a search result has the same date, treat it as current."
        ));
    }

    Ok((combined_prompt, loaded_project_file))
}

fn build_tool_prompt(
    shell_enabled: bool,
    input_enabled: bool,
    browser_enabled: bool,
    vision_enabled: bool,
) -> String {
    let mut sections = Vec::new();

    if shell_enabled {
        sections.push(
            "You have access to a shell tool that can execute OS commands.\n\
When the user asks you to perform a system task (open app,\n\
manage files, check system info, etc.), use the shell tool.\n\
For Windows, use PowerShell commands.\n\
For macOS/Linux, use bash commands.\n\
Always prefer safe, non-destructive commands.\n\
Example: user says 'open notepad' -> shell: Start-Process notepad\n\
Example: user says 'what time is it' -> shell: Get-Date\n\
Example: user says 'list files' -> shell: Get-ChildItem"
                .to_string(),
        );
    }

    if input_enabled {
        sections.push(
            "Tool: input\n\
Actions: type_text, key_press, hotkey, mouse_move, mouse_click, mouse_double_click, mouse_drag, scroll\n\
Example: {\"tool\":\"input\",\"action\":\"type_text\",\"text\":\"hello\"}\n\
Example: {\"tool\":\"input\",\"action\":\"hotkey\",\"keys\":[\"ctrl\",\"s\"]}\n\
Example: {\"tool\":\"input\",\"action\":\"mouse_click\",\"button\":\"left\",\"x\":500,\"y\":300}\n\
Example: {\"tool\":\"input\",\"action\":\"scroll\",\"direction\":\"down\",\"amount\":3}"
                .to_string(),
        );
    }

    if browser_enabled {
        sections.push(
            "Tool: browser\n\
Actions: open, goto, scroll, click, type_text, read_text, read_page, screenshot, evaluate, tab_list, tab_switch, tab_close, back, forward\n\
Example: {\"tool\":\"browser\",\"action\":\"open\",\"url\":\"https://google.com\"}\n\
Example: {\"tool\":\"browser\",\"action\":\"click\",\"selector\":\"button.submit\"}\n\
Example: {\"tool\":\"browser\",\"action\":\"type_text\",\"selector\":\"input#search\",\"text\":\"forja rust\"}\n\
Example: {\"tool\":\"browser\",\"action\":\"scroll\",\"direction\":\"down\",\"amount\":500}\n\
Example: {\"tool\":\"browser\",\"action\":\"read_text\",\"selector\":\"h1\"}\n\
Example: {\"tool\":\"browser\",\"action\":\"screenshot\"}"
                .to_string(),
        );
    }

    if vision_enabled {
        sections.push(
            "Tool: vision\n\
Actions: capture_screen, capture_region, analyze, analyze_region, find_element, ocr\n\
Example: {\"tool\":\"vision\",\"action\":\"analyze\",\"prompt\":\"What is on the screen?\"}\n\
Example: {\"tool\":\"vision\",\"action\":\"find_element\",\"description\":\"red login button\"}\n\
Example: {\"tool\":\"vision\",\"action\":\"capture_region\",\"x\":100,\"y\":200,\"width\":500,\"height\":300}\n\
Example: {\"tool\":\"vision\",\"action\":\"ocr\",\"x\":0,\"y\":0,\"width\":1920,\"height\":1080}\n\
Note: Chain find_element result with input tool mouse_click to click visual elements."
                .to_string(),
        );
    }

    sections.join("\n\n")
}

fn resolve_prompts_dir(
    forja_cfg: &config::ForjaConfig,
    bootstrap_paths: &bootstrap::BootstrapPaths,
) -> std::path::PathBuf {
    forja_cfg
        .agent
        .prompts_dir
        .clone()
        .unwrap_or_else(|| bootstrap_paths.forja_dir.join("prompts"))
}

fn initialize_prompt_loader(
    forja_cfg: &config::ForjaConfig,
    bootstrap_paths: &bootstrap::BootstrapPaths,
) -> std::io::Result<()> {
    let prompts_dir = resolve_prompts_dir(forja_cfg, bootstrap_paths);
    let prompt_loader = PromptLoader::new(&prompts_dir);
    prompt_loader.ensure_default_files()?;
    install_prompt_loader(prompt_loader).map_err(|_| {
        std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            "prompt loader already initialized",
        )
    })
}

fn resolve_exec_mode(
    force_safe: bool,
    force_trust: bool,
    forja_cfg: &config::ForjaConfig,
) -> ExecMode {
    if force_safe {
        return ExecMode::Safe;
    }
    if force_trust {
        return ExecMode::Trust;
    }
    if let Ok(value) = std::env::var("FORJA_MODE") {
        return config::parse_exec_mode(&value).unwrap_or(ExecMode::Auto);
    }

    forja_cfg
        .agent
        .resolved_exec_mode()
        .unwrap_or(ExecMode::Auto)
}

fn parse_think_level() -> ThinkLevel {
    match std::env::var("FORJA_THINK")
        .unwrap_or_else(|_| "mid".to_string())
        .to_lowercase()
        .as_str()
    {
        "min" => ThinkLevel::Min,
        "max" => ThinkLevel::Max,
        _ => ThinkLevel::Mid,
    }
}

fn exec_mode_label(mode: ExecMode) -> &'static str {
    match mode {
        ExecMode::Safe => "safe",
        ExecMode::Auto => "auto",
        ExecMode::Trust => "trust",
    }
}

fn think_level_label(level: ThinkLevel) -> &'static str {
    match level {
        ThinkLevel::Min => "min",
        ThinkLevel::Mid => "mid",
        ThinkLevel::Max => "max",
    }
}

fn role_label(role: ModeRole) -> &'static str {
    match role {
        ModeRole::Auto => "auto",
        ModeRole::Coder => "coder",
        ModeRole::Writer => "writer",
        ModeRole::Assistant => "assistant",
        ModeRole::Analyst => "analyst",
        ModeRole::Default => "default",
    }
}

fn internal_command_to_input(command: &InternalCommand) -> String {
    match command {
        InternalCommand::Mode(ExecMode::Safe) => "/mode safe".to_string(),
        InternalCommand::Mode(ExecMode::Auto) => "/mode auto".to_string(),
        InternalCommand::Mode(ExecMode::Trust) => "/mode trust".to_string(),
        InternalCommand::Think(ThinkLevel::Min) => "/think min".to_string(),
        InternalCommand::Think(ThinkLevel::Mid) => "/think mid".to_string(),
        InternalCommand::Think(ThinkLevel::Max) => "/think max".to_string(),
        InternalCommand::Role(ModeRole::Coder) => "/role coder".to_string(),
        InternalCommand::Role(ModeRole::Writer) => "/role writer".to_string(),
        InternalCommand::Role(ModeRole::Assistant) => "/role assistant".to_string(),
        InternalCommand::Role(ModeRole::Analyst) => "/role analyst".to_string(),
        InternalCommand::Role(ModeRole::Auto) | InternalCommand::Role(ModeRole::Default) => {
            "/role auto".to_string()
        }
        InternalCommand::Screenshot(Some(prompt)) => format!("/ss {prompt}"),
        InternalCommand::Screenshot(None) => "/ss".to_string(),
        InternalCommand::Help => "/help".to_string(),
        InternalCommand::Models => "/models".to_string(),
        InternalCommand::Model(model) => format!("/model {model}"),
        InternalCommand::Background(BackgroundCmd::Status) => "/background".to_string(),
        InternalCommand::Background(BackgroundCmd::Off) => "/background off".to_string(),
        InternalCommand::Background(BackgroundCmd::Auto) => "/background auto".to_string(),
        InternalCommand::Skill(name, args) if args.is_empty() => format!("/skill run {name}"),
        InternalCommand::Skill(name, args) => format!("/skill run {name} {args}"),
    }
}

fn help_text() -> String {
    [
        "Available commands:",
        "/mode <safe|auto|trust>",
        "/think <min|mid|max>",
        "/role <coder|writer|assistant|analyst|auto>",
        "/models",
        "/model <name>",
        "/ss [prompt]",
        "/image <path> [prompt]",
        "/background",
        "/background off",
        "/background auto",
        "/skill list",
        "/skill run <name> [args]",
        "/skill info <name>",
        "/skill reload",
        "/identity",
    ]
    .join("\n")
}

fn memory_stats_text(stats: &forja_memory::manager::MemoryStats) -> String {
    format!(
        "Memory stats:\n- Session messages: {}\n- Long-term entries: {}\n- Estimated tokens: {}",
        stats.session_messages, stats.longterm_entries, stats.estimated_tokens
    )
}

fn eval_result_text(skill_name: &str, result: &EvalResult) -> String {
    let mut lines = vec![format!(
        "[eval] {skill_name}: {}/{} passed ({}ms)",
        result.passed,
        result.results.len(),
        result.duration_ms
    )];
    lines.extend(result.results.iter().map(|case| {
        if case.passed {
            format!("  - {}: passed", case.case_name)
        } else {
            format!(
                "  - {}: failed ({})",
                case.case_name,
                case.failure_reason.as_deref().unwrap_or("unknown")
            )
        }
    }));
    lines.join("\n")
}

fn benchmark_result_text(skill_name: &str, result: &BenchmarkResult) -> String {
    format!(
        "[benchmark] {skill_name}: pass_rate={:.2}, avg={}ms, min={}ms, max={}ms, runs={}",
        result.pass_rate,
        result.avg_duration_ms,
        result.min_duration_ms,
        result.max_duration_ms,
        result.run_count
    )
}

fn suggestions_text(skill_name: &str, suggestions: &[Suggestion]) -> String {
    if suggestions.is_empty() {
        return format!("[improve] {skill_name}: no suggestions");
    }

    let mut lines = vec![format!("[improve] {skill_name}:")];
    lines.extend(suggestions.iter().map(|suggestion| {
        let priority = match suggestion.priority {
            SuggestionPriority::High => "high",
            SuggestionPriority::Medium => "medium",
            SuggestionPriority::Low => "low",
        };
        format!("- [{priority}] {}", suggestion.description)
    }));
    lines.join("\n")
}

async fn record_skill_history(
    memory_store: &MemoryManagerStore,
    skill_name: &str,
    category: &str,
    text: &str,
) {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let entry = forja_core::types::MemoryEntry {
        id: format!("skill_{category}_{skill_name}_{timestamp}"),
        timestamp,
        tags: vec!["system".to_string()],
        content: format!("[skill:{category}:{skill_name}] {text}"),
        score: 0.0,
        metadata: Default::default(),
    };
    let _ = memory_store.save(&entry).await;
}

fn reload_skill_loader(loader: &mut SkillLoader) -> std::io::Result<usize> {
    let skills = loader.load_all()?;
    set_skill_catalog_summary(loader.summary());
    Ok(skills.len())
}

fn skill_list_text(loader: &SkillLoader) -> String {
    if loader.skills().is_empty() {
        return "No skills installed.".to_string();
    }

    let mut lines = vec!["Installed skills:".to_string()];
    lines.extend(
        loader
            .skills()
            .iter()
            .map(|skill| format!("- {}: {}", skill.name, skill.description)),
    );
    lines.join("\n")
}

fn skill_info_text(skill: &Skill) -> std::io::Result<String> {
    std::fs::read_to_string(skill.base_dir.join("SKILL.md"))
}

fn split_skill_run_target(input: &str) -> Option<(String, String)> {
    let rest = input.strip_prefix("/skill run ")?.trim();
    if rest.is_empty() {
        return None;
    }

    let mut parts = rest.splitn(2, ' ');
    let name = parts.next()?.trim().to_string();
    let args = parts.next().unwrap_or_default().trim().to_string();
    Some((name, args))
}

fn shell_single_quote(value: &str) -> String {
    value.replace('\'', "''")
}

fn resolve_skill_env(skill: &Skill, cfg: &config::ForjaConfig) -> HashMap<String, String> {
    let mut env_map = HashMap::new();
    let configured = cfg.skills.entries.get(&skill.name);

    for env_name in &skill.env {
        if let Some(value) = configured.and_then(|entry| entry.env.get(env_name)) {
            env_map.insert(env_name.clone(), value.clone());
            continue;
        }

        if let Ok(value) = std::env::var(env_name) {
            env_map.insert(env_name.clone(), value);
        }
    }

    env_map
}

fn build_skill_command(
    skill: &Skill,
    script_path: &Path,
    args: &str,
    env_map: &HashMap<String, String>,
) -> String {
    #[cfg(target_os = "windows")]
    {
        let mut command = format!(
            "Set-Location -LiteralPath '{}'; ",
            shell_single_quote(&skill.base_dir.display().to_string())
        );
        for (key, value) in env_map {
            command.push_str(&format!("$env:{key}='{}'; ", shell_single_quote(value)));
        }

        let script = shell_single_quote(&script_path.display().to_string());
        match script_path.extension().and_then(|ext| ext.to_str()) {
            Some("py") => command.push_str(&format!("python '{script}'")),
            Some("sh") => command.push_str(&format!("bash '{script}'")),
            _ => command.push_str(&format!("& '{script}'")),
        }

        if !args.is_empty() {
            command.push(' ');
            command.push_str(args);
        }

        command
    }

    #[cfg(not(target_os = "windows"))]
    {
        let mut parts = vec![format!(
            "cd '{}' &&",
            shell_single_quote(&skill.base_dir.display().to_string())
        )];

        for (key, value) in env_map {
            parts.push(format!("{key}='{}'", shell_single_quote(value)));
        }

        let script = shell_single_quote(&script_path.display().to_string());
        let runner = match script_path.extension().and_then(|ext| ext.to_str()) {
            Some("py") => format!("python '{script}'"),
            Some("sh") => format!("bash '{script}'"),
            _ => format!("'{script}'"),
        };
        parts.push(runner);

        if !args.is_empty() {
            parts.push(args.to_string());
        }

        parts.join(" ")
    }
}

async fn execute_skill(
    skill: &Skill,
    args: &str,
    cfg: &config::ForjaConfig,
    exec_mode_handle: Arc<std::sync::Mutex<ExecMode>>,
    timeout_secs: u64,
) -> Result<String> {
    if skill.scripts.is_empty() {
        return Ok("Skill has no scripts to execute.".to_string());
    }

    let env_map = resolve_skill_env(skill, cfg);
    let confirmation = StdinConfirmation::from_shared(exec_mode_handle.clone());
    let shell_tool = ShellTool::with_settings(
        Arc::new(StdinConfirmation::new(ExecMode::Trust)),
        Duration::from_secs(timeout_secs.max(1)),
        false,
    );
    let mut outputs = Vec::new();

    for script in &skill.scripts {
        let script_path = skill.base_dir.join(script);
        if !script_path.exists() {
            return Err(ForjaError::ToolError(format!(
                "Skill script not found: {}",
                script_path.display()
            )));
        }

        let script_body = std::fs::read_to_string(&script_path).unwrap_or_default();
        let dangerous = ShellTool::is_dangerous_command(&script_body);
        let prompt = format!("Run skill '{}' script '{}'", skill.name, script);
        if !confirmation.confirm(&prompt, dangerous).await {
            return Ok(format!("Skill execution blocked: {script}"));
        }

        let command = build_skill_command(skill, &script_path, args, &env_map);
        let result = shell_tool.execute(json!({ "command": command })).await?;
        let status = result["status"].as_str().unwrap_or("unknown");
        let output = result["output"]
            .as_str()
            .or_else(|| result["detail"].as_str())
            .unwrap_or_default()
            .trim()
            .to_string();

        outputs.push(if output.is_empty() {
            format!("{script}: {status}")
        } else {
            format!("{script}: {status}\n{output}")
        });
    }

    Ok(outputs.join("\n\n"))
}

fn skill_runtime_context(skill: &Skill, args: &str, execution_result: &str) -> String {
    let args_section = if args.is_empty() {
        "No additional skill arguments were provided.".to_string()
    } else {
        format!("Skill arguments: {args}")
    };

    format!(
        "[skill]\nName: {}\nDescription: {}\n{}\n\nInstructions:\n{}\n\nExecution result:\n{}",
        skill.name,
        skill.description,
        args_section,
        skill.instructions,
        execution_result
    )
}

fn load_image_base64(path: &Path) -> std::io::Result<String> {
    let bytes = std::fs::read(path)?;
    Ok(BASE64_STANDARD.encode(bytes))
}

// Entrypoint

#[tokio::main]
async fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() >= 3 && args[1] == "login" {
        oauth::run_login(&args[2]).await;
        std::process::exit(0);
    } else if args.len() == 2 && args[1] == "login" {
        println!("Usage: forja login <provider>");
        println!("<provider> options: openai, gemini, anthropic");
        std::process::exit(1);
    }

    let _auth_data = oauth::AuthData::load();

    ctrlc::set_handler(move || {
        println!("\n[System] Exiting...");
        std::process::exit(0);
    })
    .expect("Error setting Ctrl+C handler");

    // Parse subcommands
    // let args: Vec<String> = std::env::args().collect(); // Already collected above

    // `forja setup` subcommand: run setup and exit
    if args.get(1).map(|s| s.as_str()) == Some("setup") {
        let setup_cfg = config::run_setup();
        initialize_prompt_loader(&setup_cfg, &bootstrap::default_paths())?;
        return Ok(());
    }

    let mut force_setup = false;
    let mut force_safe = false;
    let mut force_trust = false;
    let mut new_provider = None;
    let mut new_model = None;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--setup" => force_setup = true,
            "--safe" => force_safe = true,
            "--trust" => force_trust = true,
            "--provider" => {
                if i + 1 < args.len() {
                    new_provider = Some(args[i + 1].clone());
                    i += 1;
                }
            }
            "--model" => {
                if i + 1 < args.len() {
                    new_model = Some(args[i + 1].clone());
                    i += 1;
                }
            }
            _ => {}
        }
        i += 1;
    }

    if force_safe && force_trust {
        eprintln!("Error: --safe and --trust cannot be used together");
        std::process::exit(1);
    }

    // Load config
    let mut forja_cfg = if force_setup {
        config::run_onboarding()
    } else {
        config::load_config()
    };

    // Run onboarding explicitly if there is no configured provider and no override.
    if forja_cfg.active.provider.is_none() && !force_setup {
        forja_cfg = config::run_onboarding();
    }

    // Apply command-line overrides
    let mut updated = false;
    if let Some(p) = new_provider {
        println!("[System] Switching provider to: {}", p);
        forja_cfg.active.provider = Some(p.clone());

        // Ask for the API key immediately if it is missing.
        if forja_cfg.keys.get_for(&p).is_none() && p != "ollama" {
            print!("\n[WARNING] Missing API key for {}. Enter it now > ", p);
            std::io::stdout().flush().ok();
            let mut key = String::new();
            std::io::stdin().read_line(&mut key).ok();
            let trimmed = key.trim().to_string();
            if !trimmed.is_empty() {
                forja_cfg.keys.set_for(&p, trimmed);
            }
        }
        updated = true;
    }
    if let Some(m) = new_model {
        println!("[System] Setting model to: {}", m);
        forja_cfg.active.model = Some(m);
        updated = true;
    }

    if updated {
        config::save_config(&forja_cfg).ok();
    }

    let info = config::provider_info(&forja_cfg);
    print_banner(&info);
    let shell_enabled = !matches!(
        std::env::var("FORJA_SHELL"),
        Ok(value) if value.eq_ignore_ascii_case("false")
    );
    let input_enabled = !matches!(
        std::env::var("FORJA_INPUT"),
        Ok(value) if value.eq_ignore_ascii_case("false")
    );
    let browser_enabled = !matches!(
        std::env::var("FORJA_BROWSER"),
        Ok(value) if value.eq_ignore_ascii_case("false")
    );
    let vision_enabled = !matches!(
        std::env::var("FORJA_VISION"),
        Ok(value) if value.eq_ignore_ascii_case("false")
    );
    let bootstrap_paths = bootstrap::default_paths();
    initialize_prompt_loader(&forja_cfg, &bootstrap_paths)?;
    let mut skill_loader = SkillLoader::new(default_skills_dir());
    if let Err(error) = reload_skill_loader(&mut skill_loader) {
        eprintln!("[Skill] failed to load skills: {error}");
    }
    let skill_loader = Arc::new(std::sync::Mutex::new(skill_loader));
    let bootstrap_outcome = bootstrap::ensure_bootstrap(&bootstrap_paths)?;
    let assistant_name = bootstrap_outcome.profile.identity.assistant_name.clone();
    let user_name = bootstrap_outcome.profile.identity.user_name.clone();
    let (combined_prompt, loaded_project_file) = build_system_prompt(&bootstrap_paths)?;
    let tool_prompt = build_tool_prompt(
        shell_enabled,
        input_enabled,
        browser_enabled,
        vision_enabled,
    );
    if let Some(file_name) = loaded_project_file {
        println!("[System] Loaded {file_name}");
    }

    // Initialize ProviderRegistry
    let registry = ProviderRegistry::from_config(&forja_cfg);

    // Clone config for slash handler usage before any fields are moved.
    let cfg_for_handler = forja_cfg.clone();

    // Mock mode or live provider
    let use_mock = std::env::var("FORJA_USE_MOCK").is_ok();
    let llm_config = if use_mock {
        None
    } else {
        Some(config::llm_config_from(&forja_cfg).map_err(forja_core::error::ForjaError::LlmError)?)
    };
    let provider: Arc<dyn LlmProvider> = if use_mock {
        println!("[System] MockLlmProvider mode (no live LLM calls)");
        Arc::new(MockLlmProvider)
    } else {
        Arc::new(LlmClient::new(
            llm_config
                .clone()
                .expect("llm_config must exist when not in mock mode"),
        )?)
    };
    let exec_mode = resolve_exec_mode(force_safe, force_trust, &forja_cfg);
    let think_level = parse_think_level();
    let mode_state = ModeState::new(exec_mode, think_level, ModeRole::Auto);
    let exec_mode_handle = Arc::new(std::sync::Mutex::new(exec_mode));
    let background_interval = forja_cfg.background.interval_seconds;
    let background_manager = Arc::new(tokio::sync::Mutex::new(BackgroundManager::new(
        background_interval,
    )));
    let background_status = Arc::new(std::sync::Mutex::new(
        background_runtime::BackgroundStatusSnapshot::disabled(
            background_interval,
            "initializing",
        ),
    ));
    let background_home_dir = bootstrap_paths
        .forja_dir
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| bootstrap_paths.forja_dir.clone());
    let _ = forja_llm::ensure_models_dir(&background_home_dir);

    if forja_cfg.background.provider.eq_ignore_ascii_case("off") {
        if let Ok(mut status) = background_status.lock() {
            *status = background_runtime::BackgroundStatusSnapshot::disabled(
                background_interval,
                "disabled by config",
            );
        }
        println!("Background model: disabled (disabled by config)");
    } else {
        let background_cfg = forja_cfg.clone();
        let background_manager_for_init = background_manager.clone();
        let background_status_for_init = background_status.clone();
        let background_home_dir_for_init = background_home_dir.clone();
        tokio::spawn(async move {
            match background_runtime::discover_background_provider(
                &background_cfg,
                &background_home_dir_for_init,
            )
            .await
            {
                background_runtime::BackgroundDiscovery::Selected { candidate, provider } => {
                    let mut manager = background_manager_for_init.lock().await;
                    background_runtime::apply_background_candidate(
                        &mut manager,
                        &candidate,
                        provider,
                        background_cfg.background.interval_seconds,
                    )
                    .await;
                    let active = manager.is_active();
                    drop(manager);

                    if let Ok(mut status) = background_status_for_init.lock() {
                        *status = background_runtime::BackgroundStatusSnapshot::selected(
                            &candidate,
                            background_cfg.background.interval_seconds,
                            active,
                        );
                    }

                    println!(
                        "Background model: {}/{} ({})",
                        candidate.provider, candidate.model, candidate.kind
                    );
                }
                background_runtime::BackgroundDiscovery::Disabled(reason) => {
                    let mut manager = background_manager_for_init.lock().await;
                    manager.stop().await;
                    manager.disable();
                    drop(manager);

                    if let Ok(mut status) = background_status_for_init.lock() {
                        *status = background_runtime::BackgroundStatusSnapshot::disabled(
                            background_cfg.background.interval_seconds,
                            &reason,
                        );
                    }

                    println!("Background model: disabled ({reason})");
                }
            }
        });
    }

    // Channel setup
    let (channel, interactive_identity_supported, print_initial_prompt): (
        Arc<dyn Channel>,
        bool,
        bool,
    ) = {
        #[cfg(feature = "telegram")]
        {
            let bot_token = forja_cfg
                .channel
                .telegram
                .bot_token
                .clone()
                .or_else(|| std::env::var("TELEGRAM_BOT_TOKEN").ok());

            if let Some(token) = bot_token {
                let allowed = forja_cfg.channel.telegram.allowed_chat_ids.clone();
                if allowed.is_empty() {
                    println!("[WARN] Telegram allowed_chat_ids is empty.");
                } else {
                    println!(
                        "[System] MultiChannel starting with CLI + Telegram (IDs: {:?})",
                        allowed
                    );
                }
                (
                    Arc::new(forja_channel::multi::MultiChannel::new_both(token, allowed).await),
                    false,
                    true,
                )
            } else {
                println!("[System] CLI mode (Telegram not configured)");
                (Arc::new(forja_channel::cli::CliChannel::new()), true, false)
            }
        }
        #[cfg(not(feature = "telegram"))]
        {
            (Arc::new(forja_channel::cli::CliChannel::new()), true, false)
        }
    };

    let memory_base_dir = default_memory_base_dir();
    let memory_store = MemoryManagerStore::new(&memory_base_dir, None).await?;
    memory_store.load().await?;
    let channel: Arc<dyn Channel> = Arc::new(MemoryAwareChannel::new(channel.clone(), memory_store.clone()));

    // System prompt setup
    let mut engine = Engine::new(provider.clone(), channel.clone());
    engine = engine
        .with_mode(mode_state.clone())
        .with_tool_prompt(tool_prompt);
    engine = engine.with_assistant_profile(assistant_name.clone(), user_name.clone());

    if !combined_prompt.is_empty() {
        engine = engine.with_system_prompt(combined_prompt);
    }

    let knowledge_dir = std::env::var("FORJA_KNOWLEDGE_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| bootstrap_paths.forja_dir.join("knowledge"));
    let knowledge_manager = Arc::new(KnowledgeManager::new(knowledge_dir));
    engine = engine.with_knowledge(knowledge_manager.clone());

    let serendipity_enabled = !matches!(
        std::env::var("FORJA_SERENDIPITY"),
        Ok(value) if value.eq_ignore_ascii_case("false")
    );
    if serendipity_enabled {
        engine = engine.with_serendipity(SerendipityEngine::new());
    }

    engine = engine.with_emotion(EmotionEngine::new());

    let capture_backend_for_vision: Arc<dyn forja_tools::ScreenCaptureBackend> = if use_mock {
        Arc::new(MockCaptureBackend::new())
    } else {
        #[cfg(feature = "vision")]
        {
            Arc::new(XcapBackend::new())
        }
        #[cfg(not(feature = "vision"))]
        {
            Arc::new(MockCaptureBackend::new())
        }
    };
    let vision_analyzer_for_vision: Arc<dyn VisionAnalyzer> = if use_mock {
        Arc::new(MockVisionAnalyzer::new())
    } else {
        let llm_config = llm_config
            .as_ref()
            .expect("llm_config must exist when not in mock mode");
        Arc::new(GptVisionAnalyzer::new(
            llm_config.base_url.clone(),
            llm_config.api_key.clone(),
            llm_config.model.clone(),
        ))
    };

    // Register tools
    let file_tool = Arc::new(FileTool::new());
    let web_tool = Arc::new(WebTool::new());
    let search_provider = match forja_cfg.tools.search.provider.as_deref() {
        Some("brave") => {
            let key = forja_cfg
                .tools
                .search
                .brave_api_key
                .clone()
                .unwrap_or_default();
            SearchProvider::Brave { api_key: key }
        }
        Some("grok") | Some("xai") => {
            let key = forja_cfg
                .tools
                .search
                .xai_api_key
                .clone()
                .unwrap_or_default();
            SearchProvider::Grok { api_key: key }
        }
        _ => SearchProvider::DuckDuckGo,
    };
    let search_tool = Arc::new(SearchTool::new(search_provider));

    engine.register_tool(file_tool);
    engine.register_tool(web_tool);
    engine.register_tool(search_tool);
    if shell_enabled {
        let shell_tool = Arc::new(ShellTool::new(Arc::new(StdinConfirmation::new(
            ExecMode::Trust,
        ))));
        engine.register_tool(shell_tool);
    } else {
        println!("[System] Shell tool disabled by FORJA_SHELL=false.");
    }
    if input_enabled {
        match InputTool::new(Arc::new(StdinConfirmation::from_shared(
            exec_mode_handle.clone(),
        ))) {
            Ok(input_tool) => engine.register_tool(Arc::new(input_tool)),
            Err(error) => eprintln!("[System] Input tool initialization failed: {error}"),
        }
    } else {
        println!("[System] Input tool disabled by FORJA_INPUT=false.");
    }
    if browser_enabled {
        let confirmation = Arc::new(StdinConfirmation::from_shared(exec_mode_handle.clone()));
        if use_mock {
            let browser_tool = BrowserTool::with_backend_and_settings(
                Arc::new(MockBrowserBackend::new()),
                confirmation,
                false,
            );
            engine.register_tool(Arc::new(browser_tool));
        } else {
            let browser_tool = BrowserTool::new(confirmation);
            engine.register_tool(Arc::new(browser_tool));
        }
    } else {
        println!("[System] Browser tool disabled by FORJA_BROWSER=false.");
    }
    if vision_enabled {
        let vision_tool = VisionTool::with_backends(
            capture_backend_for_vision.clone(),
            vision_analyzer_for_vision.clone(),
            false,
        );
        engine.register_tool(Arc::new(vision_tool));
    } else {
        println!("[System] Vision tool disabled by FORJA_VISION=false.");
    }

    if ClaudeCodeTool::is_installed().await {
        engine.register_tool(Arc::new(ClaudeCodeTool::new()));
        println!("[System] Claude Code tool registered.");
    }
    if CodexTool::is_installed().await {
        engine.register_tool(Arc::new(CodexTool::new()));
        println!("[System] Codex tool registered.");
    }
    if GeminiCliTool::is_installed().await {
        engine.register_tool(Arc::new(GeminiCliTool::new()));
        println!("[System] Gemini CLI tool registered.");
    }

    // Slash handler with ProviderRegistry captured in a closure
    let registry = std::sync::Mutex::new(registry);
    let channel_for_slash = channel.clone();
    let bootstrap_paths_for_slash = bootstrap_paths.clone();
    let exec_mode_handle_for_slash = exec_mode_handle.clone();
    let vision_enabled_for_slash = vision_enabled;
    let capture_backend_for_slash = capture_backend_for_vision.clone();
    let vision_analyzer_for_slash = vision_analyzer_for_vision.clone();
    let skill_loader_for_slash = skill_loader.clone();
    let memory_store_for_slash = memory_store.clone();
    let background_manager_for_slash = background_manager.clone();
    let background_status_for_slash = background_status.clone();
    let background_home_dir_for_slash = background_home_dir.clone();
    let slash_handler: forja_core::engine::SlashHandler = Arc::new(
        move |text: &str, provider: &mut Arc<dyn LlmProvider>, mode_state: &mut ModeState| {
            let original_text = text.trim();
            clear_active_skill_context();
            let mapped_text = if original_text.starts_with('/') {
                None
            } else {
                skill_loader_for_slash
                    .lock()
                    .ok()
                    .and_then(|loader| detect_intent_with_skills(original_text, &loader))
                    .map(|command| internal_command_to_input(&command))
            };
            let text = mapped_text.as_deref().unwrap_or(original_text);

            if vision_enabled_for_slash {
                if let Some(prompt) = parse_screenshot_command(text) {
                    println!("[Vision] Captured the screen. Analyzing...");
                    let prompt = if prompt.trim().is_empty() {
                        "Describe what you see on screen.".to_string()
                    } else {
                        prompt
                    };
                    let result = tokio::task::block_in_place(|| {
                        tokio::runtime::Handle::current().block_on(async {
                            let capture = capture_backend_for_slash.capture_full().await?;
                            let image_base64 = BASE64_STANDARD.encode(capture);
                            vision_analyzer_for_slash
                                .analyze_image(&image_base64, &prompt)
                                .await
                        })
                    });

                    return Some(forja_core::engine::SlashCommandResult::ReplyAndSave {
                        user_text: text.to_string(),
                        reply: match result {
                            Ok(reply) => reply,
                            Err(error) => format!("❌ Vision analysis failed: {error}"),
                        },
                    });
                }

                if let Some((path, prompt)) = parse_image_command(text) {
                    let prompt = if prompt.trim().is_empty() {
                        "Describe what you see in this image.".to_string()
                    } else {
                        prompt
                    };
                    let image_base64 = match load_image_base64(&path) {
                        Ok(image_base64) => image_base64,
                        Err(error) => {
                            return Some(forja_core::engine::SlashCommandResult::Reply(format!(
                                "❌ Could not read the image file: {error}"
                            )));
                        }
                    };
                    let result = tokio::task::block_in_place(|| {
                        tokio::runtime::Handle::current().block_on(async {
                            vision_analyzer_for_slash
                                .analyze_image(&image_base64, &prompt)
                                .await
                        })
                    });

                    return Some(forja_core::engine::SlashCommandResult::ReplyAndSave {
                        user_text: text.to_string(),
                        reply: match result {
                            Ok(reply) => reply,
                            Err(error) => format!("❌ Vision analysis failed: {error}"),
                        },
                    });
                }

                if let Some((path, prompt)) = detect_image_path(text) {
                    match load_image_base64(&path) {
                        Ok(image_base64) => {
                            let prompt = if prompt.trim().is_empty() {
                                "Describe what you see in this image.".to_string()
                            } else {
                                prompt
                            };
                            let result = tokio::task::block_in_place(|| {
                                tokio::runtime::Handle::current().block_on(async {
                                    vision_analyzer_for_slash
                                        .analyze_image(&image_base64, &prompt)
                                        .await
                                })
                            });

                            return Some(forja_core::engine::SlashCommandResult::ReplyAndSave {
                                user_text: text.to_string(),
                                reply: match result {
                                    Ok(reply) => reply,
                                    Err(error) => format!("❌ Vision analysis failed: {error}"),
                                },
                            });
                        }
                        Err(error) => {
                            eprintln!(
                                "[Vision] failed to load image '{}': {error}",
                                path.display()
                            );
                        }
                    }
                }
            }

            if let Some(command) = parse_slash_command(text) {
                match command {
                    SlashCommand::Mode(mode) => {
                        mode_state.update_exec_mode(mode);
                        if let Ok(mut shared_mode) = exec_mode_handle_for_slash.lock() {
                            *shared_mode = mode;
                        }
                        return Some(forja_core::engine::SlashCommandResult::Reply(format!(
                            "Mode switched to: {}",
                            exec_mode_label(mode)
                        )));
                    }
                    SlashCommand::Think(level) => {
                        mode_state.update_think_level(level);
                        return Some(forja_core::engine::SlashCommandResult::Reply(format!(
                            "[System] Think updated: {}",
                            think_level_label(level)
                        )));
                    }
                    SlashCommand::Role(role) => {
                        mode_state.update_role(role);
                        return Some(forja_core::engine::SlashCommandResult::Reply(format!(
                            "[System] Role updated: {}",
                            role_label(role)
                        )));
                    }
                }
            }

            if text == "/models" {
                let reg = registry.lock().unwrap();
                return Some(forja_core::engine::SlashCommandResult::Reply(
                    reg.list_for_config(&cfg_for_handler),
                ));
            }

            if text == "/help" {
                return Some(forja_core::engine::SlashCommandResult::Reply(help_text()));
            }

            if let Some(memory_command) = parse_memory_command(text) {
                return Some(match memory_command {
                    MemoryCommand::Stats => {
                        let stats = tokio::task::block_in_place(|| {
                            tokio::runtime::Handle::current().block_on(memory_store_for_slash.stats())
                        });
                        match stats {
                            Ok(stats) => {
                                forja_core::engine::SlashCommandResult::Reply(memory_stats_text(&stats))
                            }
                            Err(error) => forja_core::engine::SlashCommandResult::Reply(format!(
                                "Memory stats failed: {error}"
                            )),
                        }
                    }
                    MemoryCommand::Search(query) => {
                        let results = tokio::task::block_in_place(|| {
                            tokio::runtime::Handle::current()
                                .block_on(memory_store_for_slash.search(&query, 5))
                        });
                        match results {
                            Ok(results) if results.is_empty() => {
                                forja_core::engine::SlashCommandResult::Reply(
                                    "No long-term memory matches found.".to_string(),
                                )
                            }
                            Ok(results) => {
                                let mut lines = vec![format!("Top matches for '{query}':")];
                                lines.extend(results.iter().map(|entry| {
                                    let keywords = if entry.keywords.is_empty() {
                                        String::new()
                                    } else {
                                        format!(" [{}]", entry.keywords.join(", "))
                                    };
                                    format!("- {}{}", entry.summary, keywords)
                                }));
                                forja_core::engine::SlashCommandResult::Reply(lines.join("\n"))
                            }
                            Err(error) => forja_core::engine::SlashCommandResult::Reply(format!(
                                "Memory search failed: {error}"
                            )),
                        }
                    }
                    MemoryCommand::ClearSession => {
                        tokio::task::block_in_place(|| {
                            tokio::runtime::Handle::current().block_on(async {
                                memory_store_for_slash.clear_session().await;
                            })
                        });
                        forja_core::engine::SlashCommandResult::Reply(
                            "Session memory cleared.".to_string(),
                        )
                    }
                    MemoryCommand::Flush => {
                        let result = tokio::task::block_in_place(|| {
                            tokio::runtime::Handle::current()
                                .block_on(memory_store_for_slash.flush_manager())
                        });
                        match result {
                            Ok(()) => forja_core::engine::SlashCommandResult::Reply(
                                "Session memory compressed into long-term storage.".to_string(),
                            ),
                            Err(error) => forja_core::engine::SlashCommandResult::Reply(format!(
                                "Memory flush failed: {error}"
                            )),
                        }
                    }
                });
            }

            if text == "/skill list" {
                let reply = skill_loader_for_slash
                    .lock()
                    .map(|loader| skill_list_text(&loader))
                    .unwrap_or_else(|_| "Failed to read installed skills.".to_string());
                return Some(forja_core::engine::SlashCommandResult::Reply(reply));
            }

            if let Some(action) = parse_skill_action(text) {
                let skill_name = match &action {
                    SkillAction::Eval(name)
                    | SkillAction::Improve(name)
                    | SkillAction::Benchmark { name, .. } => name.clone(),
                };

                let skill = match skill_loader_for_slash.lock() {
                    Ok(loader) => loader.find_by_name(&skill_name).cloned(),
                    Err(_) => None,
                };

                let Some(skill) = skill else {
                    return Some(forja_core::engine::SlashCommandResult::Reply(format!(
                        "Skill not found: {skill_name}"
                    )));
                };

                if skill.tests.is_empty() {
                    return Some(forja_core::engine::SlashCommandResult::Reply(format!(
                        "Skill '{skill_name}' has no test cases."
                    )));
                }

                let execute_case = |skill: &Skill, input: &str, timeout_secs: u64| {
                    tokio::task::block_in_place(|| {
                        tokio::runtime::Handle::current().block_on(execute_skill(
                            skill,
                            input,
                            &cfg_for_handler,
                            exec_mode_handle_for_slash.clone(),
                            timeout_secs,
                        ))
                    })
                };

                return Some(match action {
                    SkillAction::Eval(name) => {
                        let result = eval_skill(&skill, &skill.tests, execute_case);
                        let reply = eval_result_text(&name, &result);
                        tokio::task::block_in_place(|| {
                            tokio::runtime::Handle::current().block_on(record_skill_history(
                                &memory_store_for_slash,
                                &name,
                                "eval",
                                &reply,
                            ))
                        });
                        forja_core::engine::SlashCommandResult::Reply(reply)
                    }
                    SkillAction::Improve(name) => {
                        let result = eval_skill(&skill, &skill.tests, execute_case);
                        let suggestions = suggest_improvements(&skill, &result);
                        let reply = format!(
                            "{}\n\n{}",
                            eval_result_text(&name, &result),
                            suggestions_text(&name, &suggestions)
                        );
                        tokio::task::block_in_place(|| {
                            tokio::runtime::Handle::current().block_on(record_skill_history(
                                &memory_store_for_slash,
                                &name,
                                "improve",
                                &reply,
                            ))
                        });
                        forja_core::engine::SlashCommandResult::Reply(reply)
                    }
                    SkillAction::Benchmark { name, runs } => {
                        let result = benchmark_skill(&skill, &skill.tests, runs, execute_case);
                        let reply = benchmark_result_text(&name, &result);
                        tokio::task::block_in_place(|| {
                            tokio::runtime::Handle::current().block_on(record_skill_history(
                                &memory_store_for_slash,
                                &name,
                                "benchmark",
                                &reply,
                            ))
                        });
                        forja_core::engine::SlashCommandResult::Reply(reply)
                    }
                });
            }

            if let Some(name) = text.strip_prefix("/skill info ") {
                let reply = match skill_loader_for_slash.lock() {
                    Ok(loader) => match loader.find_by_name(name.trim()) {
                        Some(skill) => match skill_info_text(skill) {
                            Ok(content) => content,
                            Err(error) => format!("Failed to read skill info: {error}"),
                        },
                        None => format!("Skill not found: {}", name.trim()),
                    },
                    Err(_) => "Failed to read installed skills.".to_string(),
                };
                return Some(forja_core::engine::SlashCommandResult::Reply(reply));
            }

            if text == "/skill reload" {
                let reply = match skill_loader_for_slash.lock() {
                    Ok(mut loader) => match reload_skill_loader(&mut loader) {
                        Ok(count) => format!("Reloaded {count} skills."),
                        Err(error) => format!("Skill reload failed: {error}"),
                    },
                    Err(_) => "Skill reload failed: loader lock unavailable".to_string(),
                };
                return Some(forja_core::engine::SlashCommandResult::Reply(reply));
            }

            if let Some((skill_name, args)) = split_skill_run_target(text) {
                let skill = match skill_loader_for_slash.lock() {
                    Ok(loader) => loader.find_by_name(&skill_name).cloned(),
                    Err(_) => None,
                };

                let Some(skill) = skill else {
                    return Some(forja_core::engine::SlashCommandResult::Reply(format!(
                        "Skill not found: {skill_name}"
                    )));
                };

                let execution_result = tokio::task::block_in_place(|| {
                    tokio::runtime::Handle::current().block_on(execute_skill(
                        &skill,
                        &args,
                        &cfg_for_handler,
                        exec_mode_handle_for_slash.clone(),
                        30,
                    ))
                });

                match execution_result {
                    Ok(result) => {
                        set_active_skill_context(skill_runtime_context(&skill, &args, &result));
                        let user_text = if original_text.starts_with("/skill run ") {
                            if args.is_empty() {
                                format!("Run the skill '{}' and explain the result.", skill.name)
                            } else {
                                format!(
                                    "Run the skill '{}' with args '{}' and explain the result.",
                                    skill.name, args
                                )
                            }
                        } else {
                            original_text.to_string()
                        };
                        return Some(forja_core::engine::SlashCommandResult::ContinueWithUserText {
                            user_text,
                        });
                    }
                    Err(error) => {
                        return Some(forja_core::engine::SlashCommandResult::Reply(format!(
                            "Skill execution failed: {error}"
                        )));
                    }
                }
            }

            if text == "/background" {
                let mut snapshot = background_status_for_slash
                    .lock()
                    .map(|status| status.clone())
                    .unwrap_or_else(|_| {
                        background_runtime::BackgroundStatusSnapshot::disabled(
                            cfg_for_handler.background.interval_seconds,
                            "status unavailable",
                        )
                    });
                let active = tokio::task::block_in_place(|| {
                    tokio::runtime::Handle::current().block_on(async {
                        background_manager_for_slash.lock().await.is_active()
                    })
                });
                snapshot.active = active;
                return Some(forja_core::engine::SlashCommandResult::Reply(
                    snapshot.message(),
                ));
            }

            if text == "/background off" {
                tokio::task::block_in_place(|| {
                    tokio::runtime::Handle::current().block_on(async {
                        let mut manager = background_manager_for_slash.lock().await;
                        manager.stop().await;
                        manager.disable();
                    })
                });

                let reply = "Background model: disabled (disabled by command)".to_string();
                if let Ok(mut status) = background_status_for_slash.lock() {
                    *status = background_runtime::BackgroundStatusSnapshot::disabled(
                        cfg_for_handler.background.interval_seconds,
                        "disabled by command",
                    );
                }
                return Some(forja_core::engine::SlashCommandResult::Reply(reply));
            }

            if text == "/background auto" {
                let mut auto_cfg = cfg_for_handler.clone();
                auto_cfg.background.provider = "auto".to_string();
                auto_cfg.background.model.clear();
                let discovery = tokio::task::block_in_place(|| {
                    tokio::runtime::Handle::current().block_on(
                        background_runtime::discover_background_provider(
                            &auto_cfg,
                            &background_home_dir_for_slash,
                        ),
                    )
                });

                return Some(match discovery {
                    background_runtime::BackgroundDiscovery::Selected { candidate, provider } => {
                        tokio::task::block_in_place(|| {
                            tokio::runtime::Handle::current().block_on(async {
                                let mut manager = background_manager_for_slash.lock().await;
                                background_runtime::apply_background_candidate(
                                    &mut manager,
                                    &candidate,
                                    provider,
                                    cfg_for_handler.background.interval_seconds,
                                )
                                .await;
                            })
                        });

                        if let Ok(mut status) = background_status_for_slash.lock() {
                            *status = background_runtime::BackgroundStatusSnapshot::selected(
                                &candidate,
                                cfg_for_handler.background.interval_seconds,
                                true,
                            );
                        }

                        forja_core::engine::SlashCommandResult::Reply(format!(
                            "Background model: {}/{} ({})",
                            candidate.provider, candidate.model, candidate.kind
                        ))
                    }
                    background_runtime::BackgroundDiscovery::Disabled(reason) => {
                        tokio::task::block_in_place(|| {
                            tokio::runtime::Handle::current().block_on(async {
                                let mut manager = background_manager_for_slash.lock().await;
                                manager.stop().await;
                                manager.disable();
                            })
                        });
                        if let Ok(mut status) = background_status_for_slash.lock() {
                            *status = background_runtime::BackgroundStatusSnapshot::disabled(
                                cfg_for_handler.background.interval_seconds,
                                &reason,
                            );
                        }
                        forja_core::engine::SlashCommandResult::Reply(format!(
                            "Background model: disabled ({reason})"
                        ))
                    }
                });
            }

            if text == "/model" {
                let reg = registry.lock().unwrap();
                let e = reg.active();
                return Some(forja_core::engine::SlashCommandResult::Reply(format!(
                    "Current model: **{}** ({}/{})",
                    e.label, e.provider, e.model_id
                )));
            }

            if let Some(target) = text.strip_prefix("/model ") {
                let mut reg = registry.lock().unwrap();
                match reg.resolve(target, &cfg_for_handler) {
                    None => {
                        return Some(forja_core::engine::SlashCommandResult::Reply(format!(
                            "❌ Could not find model '{}'. Check `/models` for the list.",
                            target
                        )));
                    }
                    Some(idx) => match reg.switch_to(idx, &cfg_for_handler) {
                        Err(e) => {
                            return Some(forja_core::engine::SlashCommandResult::Reply(format!(
                                "❌ Switch failed: {e}"
                            )));
                        }
                        Ok(new_config) => match forja_llm::LlmClient::new(new_config) {
                            Err(e) => {
                                return Some(forja_core::engine::SlashCommandResult::Reply(
                                    format!("❌ Failed to create LlmClient: {e}"),
                                ));
                            }
                            Ok(client) => {
                                let entry = reg.active();
                                *provider = Arc::new(client);
                                return Some(forja_core::engine::SlashCommandResult::Reply(
                                    format!(
                                        "✅ Switched model: **{}** ({}/{})",
                                        entry.label, entry.provider, entry.model_id
                                    ),
                                ));
                            }
                        },
                    },
                }
            }

            if text == "/identity" {
                if !interactive_identity_supported || !channel_for_slash.is_cli_source() {
                    return Some(forja_core::engine::SlashCommandResult::Reply(
                        "This command is only supported in CLI-only mode.".to_string(),
                    ));
                }

                let outcome = match bootstrap::reset_bootstrap(&bootstrap_paths_for_slash) {
                    Ok(outcome) => outcome,
                    Err(error) => {
                        return Some(forja_core::engine::SlashCommandResult::Reply(format!(
                            "❌ Identity reset failed: {error}"
                        )));
                    }
                };

                let system_prompt = match build_system_prompt(&bootstrap_paths_for_slash) {
                    Ok((system_prompt, _)) => system_prompt,
                    Err(error) => {
                        return Some(forja_core::engine::SlashCommandResult::Reply(format!(
                            "❌ Failed to rebuild the system prompt: {error}"
                        )));
                    }
                };

                return Some(forja_core::engine::SlashCommandResult::UpdateSystemPrompt {
                    reply: outcome.profile.greeting(),
                    system_prompt: Some(system_prompt),
                    reset_history: true,
                });
            }

            None
        },
    );

    let identity_name = bootstrap_outcome.profile.identity.assistant_name.clone();
    let displayed_greeting = bootstrap_outcome.greeting;
    let mut engine = engine
        .with_memory(Arc::new(memory_store.clone()))
        .with_slash_handler(slash_handler);

    println!(
        "[System] Mode: {} | Think: {} | Role: {}",
        exec_mode_label(exec_mode),
        think_level_label(think_level),
        role_label(ModeRole::Auto)
    );
    println!("[System] Assistant: {assistant_name}");
    println!("[System] Engine is ready. Type /models to list models, /model <name> to switch.");
    if let Some(greeting) = displayed_greeting {
        println!();
        println!("{identity_name}: {greeting}");
    }
    if print_initial_prompt {
        print!("\n> ");
        std::io::stdout().flush().ok();
    }

    engine
        .run_streaming(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;

    if let Err(error) = memory_store.flush_manager().await {
        eprintln!("[Memory] final flush failed: {error}");
    }

    Ok(())
}






