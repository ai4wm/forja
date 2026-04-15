use crate::config::{self, ForjaConfig};
use crate::dashboard::DashboardServer;
use crate::provider_registry::ProviderRegistry;
use crate::runtime::mock::MockLlmProvider;
use crate::runtime::prompt::{
    auto_summarize_enabled, build_system_prompt, build_tool_prompt, summarize_memory_block,
};
use crate::runtime::slash::{build_slash_handler, SlashHandlerDeps};
use crate::runtime::tools::{register_tools, ToolRegistrationContext};
use forja_channel::multi::MultiChannel;
use forja_core::audit::logger::AuditLogger;
use forja_core::autonomy::{loop_runner::AutonomousLoop, AutonomyConfig};
use forja_core::budget::{manager::BudgetManager, BudgetMode};
use forja_core::creation::{agents::default_debate_agents, DebateAgent, DebateConfig, DebateEngine};
use forja_core::emotion::{
    generate_startup_greeting, generate_startup_greeting_with_context, EmotionEngine, MoodState,
};
use forja_core::error::{ForjaError, Result};
use forja_core::heartbeat::{scheduler::HeartbeatScheduler, HeartbeatConfig};
use forja_core::mode::{ExecMode, ModeState, Role as ModeRole, ThinkLevel};
use forja_core::traits::{Channel, LlmProvider, MemoryStore, TelegramConnectionStatus};
use forja_core::{Content, Engine, KnowledgeManager, Message, Role, SerendipityEngine};
use forja_llm::LlmClient;
use forja_memory::MarkdownMemoryStore;
use std::io::Write;
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub(crate) struct RuntimeOptions {
    pub(crate) force_setup: bool,
    pub(crate) new_provider: Option<String>,
    pub(crate) new_model: Option<String>,
}

pub(crate) struct AppRuntime {
    pub(crate) engine: Engine,
    pub(crate) dashboard_server: Arc<Mutex<DashboardServer>>,
    pub(crate) channel: Arc<dyn Channel>,
    pub(crate) provider_info: String,
    pub(crate) loaded_project_file: Option<String>,
    pub(crate) assistant_name: String,
    pub(crate) displayed_greeting: Option<String>,
    pub(crate) print_initial_prompt: bool,
    pub(crate) exec_mode: ExecMode,
    pub(crate) think_level: ThinkLevel,
}

pub(crate) async fn build_runtime(options: RuntimeOptions) -> Result<AppRuntime> {
    let mut forja_cfg = if options.force_setup {
        config::run_onboarding()
    } else {
        config::load_config()
    };

    if forja_cfg.active.provider.is_none() && !options.force_setup {
        forja_cfg = config::run_onboarding();
    }

    apply_runtime_overrides(&mut forja_cfg, options.new_provider, options.new_model);

    let provider_info = config::provider_info(&forja_cfg);
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
    let bootstrap_paths = crate::bootstrap::default_paths();
    let bootstrap_outcome = crate::bootstrap::ensure_bootstrap(&bootstrap_paths)
        .map_err(|error| ForjaError::Internal(error.to_string()))?;
    let (combined_prompt, loaded_project_file) = build_system_prompt(&bootstrap_paths)
        .map_err(|error| ForjaError::Internal(error.to_string()))?;
    let tool_prompt = build_tool_prompt(
        shell_enabled,
        input_enabled,
        browser_enabled,
        vision_enabled,
    );
    let assistant_name = forja_cfg
        .assistant_name
        .clone()
        .or_else(|| std::env::var("FORJA_ASSISTANT_NAME").ok())
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| bootstrap_outcome.profile.identity.name.clone());
    let user_title = forja_cfg
        .user_title
        .clone()
        .or_else(|| std::env::var("FORJA_USER_TITLE").ok())
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| bootstrap_outcome.profile.user.name.clone());
    let registry = ProviderRegistry::from_config(&forja_cfg);
    let cfg_for_handler = forja_cfg.clone();
    let use_mock = std::env::var("FORJA_USE_MOCK").is_ok();
    let llm_config = if use_mock {
        None
    } else {
        Some(
            config::llm_config_from(&forja_cfg).map_err(forja_core::error::ForjaError::LlmError)?,
        )
    };
    let provider: Arc<dyn LlmProvider> = if use_mock {
        println!("MockLlmProvider mode (no live LLM calls)");
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
    let exec_mode_handle = Arc::new(Mutex::new(exec_mode));

    #[cfg(feature = "telegram")]
    let bot_token = forja_cfg
        .channel
        .telegram
        .bot_token
        .clone()
        .or_else(|| std::env::var("TELEGRAM_BOT_TOKEN").ok());
    #[cfg(not(feature = "telegram"))]
    let bot_token: Option<String> = None;

    #[cfg(feature = "telegram")]
    let allowed_chat_ids = forja_cfg.channel.telegram.allowed_chat_ids.clone();
    #[cfg(not(feature = "telegram"))]
    let allowed_chat_ids = Vec::new();

    let telegram_requested = bot_token.is_some();
    #[cfg(feature = "telegram")]
    if telegram_requested {
        if allowed_chat_ids.is_empty() {
            println!("[WARN] Telegram allowed_chat_ids is empty.");
        } else {
            println!(
                "MultiChannel starting with CLI + Telegram (IDs: {:?})",
                allowed_chat_ids
            );
        }
    }

    let multi_channel = Arc::new(MultiChannel::new(bot_token, allowed_chat_ids).await);
    let telegram_status = multi_channel
        .telegram_status()
        .unwrap_or(TelegramConnectionStatus::Disconnected);
    match (telegram_requested, telegram_status) {
        (false, _) => {
            println!("MultiChannel starting with CLI only.");
        }
        (true, TelegramConnectionStatus::Connected) => {
            println!("MultiChannel starting with CLI + Telegram connected.");
        }
        (true, TelegramConnectionStatus::Reconnecting) => {
            println!("MultiChannel starting with CLI + Telegram supervisor (reconnecting).");
        }
        (true, TelegramConnectionStatus::Disconnected) => {
            println!("MultiChannel continuing in CLI-only mode.");
        }
    }

    let interactive_identity_supported = !matches!(telegram_status, TelegramConnectionStatus::Connected);
    let print_initial_prompt = true;
    let channel: Arc<dyn Channel> = multi_channel.clone();
    #[cfg(feature = "telegram")]
    let telegram_status_provider = {
        let telegram_status_handle = multi_channel.telegram_status_handle();
        std::sync::Arc::new(move || telegram_status_handle.snapshot())
    };
    #[cfg(not(feature = "telegram"))]
    let telegram_status_provider =
        crate::dashboard::routes::default_telegram_status_provider();

    let max_context_tokens = forja_cfg.agent.max_context_tokens.unwrap_or(128_000);
    let context_model = forja_cfg
        .active
        .model
        .clone()
        .unwrap_or_else(|| "cl100k_base".to_string());
    let mut engine = Engine::new(provider.clone(), channel.clone());
    engine = engine
        .with_mode(mode_state.clone())
        .with_tool_prompt(tool_prompt)
        .with_context_settings(max_context_tokens, context_model)
        .with_assistant_profile(assistant_name.clone(), user_title.clone());

    let audit_db_path = bootstrap_paths.forja_dir.join("audit.db");
    let audit_logger = Arc::new(AuditLogger::new(&audit_db_path)?);
    engine = engine.with_audit_logger(audit_logger);

    let budget_manager = Arc::new(BudgetManager::new(&audit_db_path)?);
    let agent_id = "default".to_string();
    let monthly_token_limit = forja_cfg.agent.monthly_token_limit.unwrap_or(50_000);
    let budget_mode = match forja_cfg.agent.budget_mode.as_deref() {
        Some(mode) if mode.eq_ignore_ascii_case("enforce") => BudgetMode::Enforce,
        _ => BudgetMode::Monitor,
    };
    budget_manager.register_agent(&agent_id, monthly_token_limit)?;
    engine = engine
        .with_agent_id(agent_id.clone())
        .with_budget_mode(budget_mode)
        .with_budget_manager(budget_manager);

    let mut heartbeat_scheduler = HeartbeatScheduler::new();
    if let Some(interval_secs) = forja_cfg.agent.heartbeat_interval_secs {
        heartbeat_scheduler.register(HeartbeatConfig {
            agent_id,
            interval: Duration::from_secs(interval_secs),
            enabled: true,
        });
    }
    engine = engine.with_heartbeat_scheduler(heartbeat_scheduler);

    let debate_agents = if forja_cfg.creation.agents.is_empty() {
        default_debate_agents()
    } else {
        forja_cfg
            .creation
            .agents
            .iter()
            .map(|(id, agent)| DebateAgent {
                id: id.to_string(),
                role: agent.role.clone().unwrap_or_else(|| id.to_string()),
                framework: agent.framework.clone().unwrap_or_default(),
                budget: agent.budget.unwrap_or(5_000),
            })
            .collect()
    };
    let debate_config = DebateConfig {
        diverge_rounds: forja_cfg.creation.diverge_rounds.unwrap_or(2),
        conflict_rounds: forja_cfg.creation.conflict_rounds.unwrap_or(3),
        converge_rounds: forja_cfg.creation.converge_rounds.unwrap_or(1),
        max_agents: forja_cfg.creation.max_agents.unwrap_or(5),
    };
    engine = engine.with_creation_engine(DebateEngine::new(debate_agents, debate_config));

    let dashboard_server = Arc::new(Mutex::new(
        DashboardServer::new(forja_cfg.dashboard.port, audit_db_path.clone())
            .with_telegram_status(telegram_status_provider),
    ));
    let dashboard_server_for_handler = dashboard_server.clone();
    engine = engine.with_dashboard_handler(Arc::new(move || {
        let mut server = dashboard_server_for_handler
            .lock()
            .map_err(|error| ForjaError::Internal(error.to_string()))?;
        server.start()
    }));

    let autonomy = AutonomousLoop::new(
        AutonomyConfig {
            enabled: forja_cfg.autonomy.enabled,
            task_check_interval_secs: forja_cfg.autonomy.task_check_interval_secs,
            skill_threshold: forja_cfg.autonomy.skill_threshold,
            max_retries: forja_cfg.autonomy.max_retries,
            require_approval: forja_cfg.autonomy.require_approval,
        },
        &audit_db_path,
    )?;
    engine = engine.with_autonomy(autonomy);

    let context_summary_provider = provider.clone();
    engine = engine.with_context_summary_callback(Box::new(move |messages: Vec<Message>| {
        let summary_provider = context_summary_provider.clone();
        Box::pin(async move {
            let block = messages
                .into_iter()
                .map(|message| {
                    let role = match message.role {
                        Role::User => "User",
                        Role::Assistant => "Assistant",
                        Role::System => "System",
                        Role::Tool => "Tool",
                    };
                    let body = match message.content {
                        Content::Text { text, .. } => text,
                        Content::ToolCall {
                            tool_name,
                            arguments,
                            reasoning_content,
                            ..
                        } => {
                            let reasoning = reasoning_content.unwrap_or_default();
                            format!("tool={tool_name} arguments={arguments} reasoning={reasoning}")
                        }
                        Content::ToolResult { result, .. } => {
                            format!("tool_result={result}")
                        }
                    };
                    format!("{role}: {body}")
                })
                .collect::<Vec<_>>()
                .join("\n\n");

            summarize_memory_block(summary_provider, block).await
        })
    }));

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
    let restored_mood = EmotionEngine::restore_from_memory(&memory_contents)
        .unwrap_or_else(MoodState::neutral);
    let startup_greeting = if serendipity_enabled {
        generate_startup_greeting_with_context(
            provider.as_ref(),
            &assistant_name,
            &user_title,
            &memory_contents,
            &knowledge_contents,
            bootstrap_outcome.greeting.is_some(),
        )
        .await
        .unwrap_or(None)
    } else {
        generate_startup_greeting(
            provider.as_ref(),
            &assistant_name,
            &user_title,
            &memory_contents,
            bootstrap_outcome.greeting.is_some(),
        )
        .await
        .unwrap_or(None)
    };
    engine = engine.with_emotion(EmotionEngine::new(restored_mood));

    let tool_runtime = register_tools(
        &mut engine,
        ToolRegistrationContext {
            forja_cfg: &forja_cfg,
            exec_mode_handle: exec_mode_handle.clone(),
            llm_config: llm_config.as_ref(),
            use_mock,
            shell_enabled,
            input_enabled,
            browser_enabled,
            vision_enabled,
        },
    )
    .await;

    let slash_handler = build_slash_handler(SlashHandlerDeps {
        cfg_for_handler,
        registry,
        channel: channel.clone(),
        bootstrap_paths: bootstrap_paths.clone(),
        interactive_identity_supported,
        exec_mode_handle,
        vision_enabled,
        capture_backend: tool_runtime.capture_backend,
        vision_analyzer: tool_runtime.vision_analyzer,
    });
    let displayed_greeting = bootstrap_outcome.greeting.or(startup_greeting);
    let engine = engine.with_memory(memory_store).with_slash_handler(slash_handler);

    Ok(AppRuntime {
        engine,
        dashboard_server,
        channel,
        provider_info,
        loaded_project_file,
        assistant_name,
        displayed_greeting,
        print_initial_prompt,
        exec_mode,
        think_level,
    })
}

fn apply_runtime_overrides(
    forja_cfg: &mut ForjaConfig,
    new_provider: Option<String>,
    new_model: Option<String>,
) {
    let mut updated = false;

    if let Some(provider) = new_provider {
        println!("Switching provider to: {provider}");
        forja_cfg.active.provider = Some(provider.clone());

        if forja_cfg.keys.get_for(&provider).is_none() && provider != "ollama" {
            print!("\n[WARNING] Missing API key for {provider}. Enter it now > ");
            std::io::stdout().flush().ok();
            let mut key = String::new();
            std::io::stdin().read_line(&mut key).ok();
            let trimmed = key.trim().to_string();
            if !trimmed.is_empty() {
                forja_cfg.keys.set_for(&provider, trimmed);
            }
        }

        updated = true;
    }

    if let Some(model) = new_model {
        println!("Setting model to: {model}");
        forja_cfg.active.model = Some(model);
        updated = true;
    }

    if updated {
        config::save_config(forja_cfg).ok();
    }
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
