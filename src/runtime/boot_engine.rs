use crate::runtime::boot_config::RuntimeConfig;
use crate::runtime::boot_profile::ProfileBundle;
use crate::runtime::prompt::summarize_memory_block;
use forja_core::audit::logger::AuditLogger;
use forja_core::autonomy::{loop_runner::AutonomousLoop, AutonomyConfig, AutonomyExecutionRuntime};
use forja_core::budget::{manager::BudgetManager, BudgetMode};
use forja_core::creation::{agents::default_debate_agents, DebateAgent, DebateConfig, DebateEngine};
use forja_core::engine::{DashboardHandler, DreamRuntimeConfig, TuiHandler};
use forja_core::error::Result;
use forja_core::heartbeat::{scheduler::HeartbeatScheduler, HeartbeatConfig};
use forja_core::mode::{ModeState, Role as ModeRole};
use forja_core::skill::{default_skill_roots, SkillRegistry};
use forja_core::traits::{Channel, LlmProvider};
use forja_core::{Content, Engine, KnowledgeManager, Message, Role, SerendipityEngine};
use std::sync::Arc;
use std::time::Duration;

pub(crate) struct EngineBundle {
    pub(crate) engine: Engine,
    pub(crate) knowledge_manager: Arc<KnowledgeManager>,
    pub(crate) skill_registry: Arc<SkillRegistry>,
    pub(crate) serendipity_enabled: bool,
}

pub(crate) fn build_engine_bundle(
    provider: Arc<dyn LlmProvider>,
    channel: Arc<dyn Channel>,
    runtime_config: &RuntimeConfig,
    profile: &ProfileBundle,
    dashboard_handler: DashboardHandler,
    tui_handler: TuiHandler,
    autonomy_runtime: Option<AutonomyExecutionRuntime>,
) -> Result<EngineBundle> {
    let mode_state = ModeState::new(
        runtime_config.exec_mode,
        runtime_config.think_level,
        ModeRole::Auto,
    );
    let max_context_tokens = runtime_config
        .forja_cfg
        .agent
        .max_context_tokens
        .unwrap_or(128_000);
    let context_model = runtime_config
        .forja_cfg
        .active
        .model
        .clone()
        .unwrap_or_else(|| "cl100k_base".to_string());
    let mut engine = Engine::new(provider.clone(), channel)
        .with_mode(mode_state)
        .with_tool_prompt(profile.tool_prompt.clone())
        .with_context_settings(max_context_tokens, context_model)
        .with_assistant_profile(profile.assistant_name.clone(), profile.user_title.clone());

    let audit_db_path = profile.bootstrap_paths.forja_dir.join("audit.db");
    let audit_logger = Arc::new(AuditLogger::new(&audit_db_path)?);
    engine = engine.with_audit_logger(audit_logger);
    let skill_registry = Arc::new(SkillRegistry::new(&audit_db_path, &default_skill_roots())?);
    engine = engine.with_skill_registry(skill_registry.clone());

    let budget_manager = Arc::new(BudgetManager::new(&audit_db_path)?);
    let agent_id = "default".to_string();
    let monthly_token_limit = runtime_config
        .forja_cfg
        .agent
        .monthly_token_limit
        .unwrap_or(50_000);
    let budget_mode = match runtime_config.forja_cfg.agent.budget_mode.as_deref() {
        Some(mode) if mode.eq_ignore_ascii_case("enforce") => BudgetMode::Enforce,
        _ => BudgetMode::Monitor,
    };
    budget_manager.register_agent(&agent_id, monthly_token_limit)?;
    engine = engine
        .with_agent_id(agent_id.clone())
        .with_budget_mode(budget_mode)
        .with_budget_manager(budget_manager);

    let mut heartbeat_scheduler = HeartbeatScheduler::new();
    if let Some(interval_secs) = runtime_config.forja_cfg.agent.heartbeat_interval_secs {
        heartbeat_scheduler.register(HeartbeatConfig {
            agent_id,
            interval: Duration::from_secs(interval_secs),
            enabled: true,
        });
    }
    engine = engine.with_heartbeat_scheduler(heartbeat_scheduler);

    let debate_agents = if runtime_config.forja_cfg.creation.agents.is_empty() {
        default_debate_agents()
    } else {
        runtime_config
            .forja_cfg
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
        diverge_rounds: runtime_config
            .forja_cfg
            .creation
            .diverge_rounds
            .unwrap_or(2),
        conflict_rounds: runtime_config
            .forja_cfg
            .creation
            .conflict_rounds
            .unwrap_or(3),
        combination_rounds: runtime_config
            .forja_cfg
            .creation
            .combination_rounds
            .unwrap_or(1),
        mutation_rounds: runtime_config
            .forja_cfg
            .creation
            .mutation_rounds
            .unwrap_or(1),
        converge_rounds: runtime_config
            .forja_cfg
            .creation
            .converge_rounds
            .unwrap_or(1),
        min_agents: runtime_config.forja_cfg.creation.min_agents.unwrap_or(3),
        max_agents: runtime_config.forja_cfg.creation.max_agents.unwrap_or(5),
        auto_team_sizing: runtime_config
            .forja_cfg
            .creation
            .auto_team_sizing
            .unwrap_or(true),
    };
    engine = engine.with_creation_engine(DebateEngine::new(debate_agents, debate_config));
    engine = engine.with_dashboard_handler(dashboard_handler);
    engine = engine.with_tui_handler(tui_handler);

    let autonomy = AutonomousLoop::new(
        AutonomyConfig {
            enabled: runtime_config.forja_cfg.autonomy.enabled,
            task_check_interval_secs: runtime_config.forja_cfg.autonomy.task_check_interval_secs,
            skill_threshold: runtime_config.forja_cfg.autonomy.skill_threshold,
            max_retries: runtime_config.forja_cfg.autonomy.max_retries,
            require_approval: runtime_config.forja_cfg.autonomy.require_approval,
        },
        &audit_db_path,
    )?;
    engine = engine.with_autonomy(autonomy);
    if let Some(autonomy_runtime) = autonomy_runtime {
        engine = engine.with_autonomy_runtime(autonomy_runtime);
    }
    engine = engine.with_dream_runtime(DreamRuntimeConfig {
        enabled: runtime_config.forja_cfg.dream.enabled,
        idle_after: Duration::from_secs(runtime_config.forja_cfg.dream.idle_threshold_secs),
        shutdown_after: Duration::from_secs(runtime_config.forja_cfg.dream.shutdown_threshold_secs),
    });

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

    if !profile.combined_prompt.is_empty() {
        engine = engine.with_system_prompt(profile.combined_prompt.clone());
    }

    let knowledge_dir = std::env::var("FORJA_KNOWLEDGE_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| profile.bootstrap_paths.forja_dir.join("knowledge"));
    let knowledge_manager = Arc::new(KnowledgeManager::new(knowledge_dir));
    engine = engine.with_knowledge(knowledge_manager.clone());

    let serendipity_enabled = !matches!(
        std::env::var("FORJA_SERENDIPITY"),
        Ok(value) if value.eq_ignore_ascii_case("false")
    );
    if serendipity_enabled {
        engine = engine.with_serendipity(SerendipityEngine::new());
    }

    Ok(EngineBundle {
        engine,
        knowledge_manager,
        skill_registry,
        serendipity_enabled,
    })
}
