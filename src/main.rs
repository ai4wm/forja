mod bootstrap;
mod config;
mod oauth;
mod provider_registry;

use async_trait::async_trait;
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
use forja_core::emotion::{
    EmotionEngine, MoodState, generate_startup_greeting, generate_startup_greeting_with_context,
};
use forja_core::error::{ForjaError, Result};
use forja_core::mode::{
    ExecMode, ModeState, Role as ModeRole, SlashCommand, ThinkLevel, detect_image_path,
    parse_image_command, parse_screenshot_command, parse_slash_command,
};
use forja_core::prompt::loader::{PromptLoader, install_prompt_loader};
use forja_core::traits::{LlmProvider, MemoryStore};
use forja_core::{
    Channel, Content, Engine, KnowledgeManager, Message, Role, SerendipityEngine, ToolDefinition,
};
use forja_llm::LlmClient;
use forja_memory::MarkdownMemoryStore;
#[cfg(feature = "vision")]
use forja_tools::XcapBackend;
use forja_tools::{
    BrowserTool, ClaudeCodeTool, CodexTool, FileTool, GeminiCliTool, GptVisionAnalyzer, InputTool,
    MockCaptureBackend, MockVisionAnalyzer, SearchProvider, SearchTool, ShellTool,
    StdinConfirmation, VisionAnalyzer, VisionTool, WebTool, browser::MockBrowserBackend,
};
use provider_registry::ProviderRegistry;
use std::io::Write;
use std::path::Path;
use std::pin::Pin;
use std::sync::Arc;
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

fn parse_exec_mode() -> ExecMode {
    match std::env::var("FORJA_MODE")
        .unwrap_or_else(|_| "auto".to_string())
        .to_lowercase()
        .as_str()
    {
        "safe" => ExecMode::Safe,
        "trust" => ExecMode::Trust,
        _ => ExecMode::Auto,
    }
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

fn load_image_base64(path: &Path) -> std::io::Result<String> {
    let bytes = std::fs::read(path)?;
    Ok(BASE64_STANDARD.encode(bytes))
}

fn auto_summarize_enabled() -> bool {
    !matches!(
        std::env::var("FORJA_AUTO_SUMMARIZE"),
        Ok(value) if value.eq_ignore_ascii_case("false")
    )
}

async fn summarize_memory_block(provider: Arc<dyn LlmProvider>, block: String) -> Result<String> {
    let response = provider
        .chat(
            &[
                Message::text(
                    Role::System,
                    "Summarize one daily memory.md block into at most three plain-text lines. Return only the summary lines.",
                    None,
                ),
                Message::text(
                    Role::User,
                    format!(
                        "Summarize the following daily memory.md records in max 3 lines.\n\
Keep only important preferences, decisions, and ongoing work. Answer in plain text only.\n\
\n{block}"
                    ),
                    None,
                ),
            ],
            None,
        )
        .await?;

    match response.content {
        Content::Text { text, .. } => Ok(text),
        _ => Err(ForjaError::LlmError(
            "memory summary response was not text".to_string(),
        )),
    }
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
    let mut new_provider = None;
    let mut new_model = None;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--setup" => force_setup = true,
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
    let assistant_name = forja_cfg
        .assistant_name
        .clone()
        .or_else(|| std::env::var("FORJA_ASSISTANT_NAME").ok())
        .unwrap_or_else(|| "Forja".to_string());
    let user_title = forja_cfg
        .user_title
        .clone()
        .or_else(|| std::env::var("FORJA_USER_TITLE").ok())
        .unwrap_or_else(|| "User".to_string());

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
    let bootstrap_outcome = bootstrap::ensure_bootstrap(&bootstrap_paths)?;
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
    let exec_mode = parse_exec_mode();
    let think_level = parse_think_level();
    let mode_state = ModeState::new(exec_mode, think_level, ModeRole::Auto);
    let exec_mode_handle = Arc::new(std::sync::Mutex::new(exec_mode));

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

    // System prompt setup
    let mut engine = Engine::new(provider.clone(), channel.clone());
    engine = engine
        .with_mode(mode_state.clone())
        .with_tool_prompt(tool_prompt);
    engine = engine.with_assistant_profile(assistant_name.clone(), user_title.clone());

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

    // Initialize memory store
    let memory_dir = std::env::var("FORJA_MEMORY_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| {
            dirs_next::home_dir()
                .unwrap_or_default()
                .join(".forja")
                .join("memory")
        });
    let memory_path = memory_dir.join("memory.md");
    let memory_store = Arc::new(MarkdownMemoryStore::new(memory_path).await?);

    if auto_summarize_enabled() {
        let summary_provider = provider.clone();
        if let Err(error) = memory_store
            .flush_and_summarize(|block: String| {
                tokio::task::block_in_place(|| {
                    tokio::runtime::Handle::current()
                        .block_on(summarize_memory_block(summary_provider.clone(), block))
                })
            })
            .await
        {
            eprintln!("[Memory] auto summarize failed: {error}");
        }
    }

    let memory_contents = match memory_store.load_all().await {
        Ok(contents) => contents,
        Err(error) => {
            eprintln!("[Memory] failed to load memory for emotion bootstrap: {error}");
            String::new()
        }
    };
    let knowledge_contents = match knowledge_manager.load_all_context() {
        Ok(contents) => contents,
        Err(error) => {
            eprintln!("[Knowledge] failed to load knowledge for startup greeting: {error}");
            String::new()
        }
    };
    let restored_mood =
        EmotionEngine::restore_from_memory(&memory_contents).unwrap_or_else(MoodState::neutral);
    let startup_greeting = if serendipity_enabled {
        generate_startup_greeting_with_context(
            provider.as_ref(),
            &bootstrap_outcome.profile.identity.name,
            &bootstrap_outcome.profile.user.name,
            &memory_contents,
            &knowledge_contents,
            bootstrap_outcome.greeting.is_some(),
        )
        .await.unwrap_or_else(|e| { eprintln!("[DEBUG] greeting error1: {e}"); None })
    } else {
        generate_startup_greeting(
            provider.as_ref(),
            &bootstrap_outcome.profile.identity.name,
            &bootstrap_outcome.profile.user.name,
            &memory_contents,
            bootstrap_outcome.greeting.is_some(),
        )
        .await.unwrap_or_else(|e| { eprintln!("[DEBUG] greeting error2: {e}"); None })
    };
    engine = engine.with_emotion(EmotionEngine::new(restored_mood));

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
        let shell_tool = Arc::new(ShellTool::new(Arc::new(StdinConfirmation::from_shared(
            exec_mode_handle.clone(),
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
    let slash_handler: forja_core::engine::SlashHandler = Arc::new(
        move |text: &str, provider: &mut Arc<dyn LlmProvider>, mode_state: &mut ModeState| {
            let text = text.trim();

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
                            "[System] Mode updated: {}",
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

    let identity_name = bootstrap_outcome.profile.identity.name.clone();
    let displayed_greeting = bootstrap_outcome.greeting.or(startup_greeting);
    let mut engine = engine
        .with_memory(memory_store)
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

    Ok(())
}





