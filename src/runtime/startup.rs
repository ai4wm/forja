use crate::dashboard::DashboardServer;
use crate::runtime::boot_autonomy::build_autonomy_runtime;
use crate::runtime::boot_channel::build_channel_bundle;
use crate::runtime::boot_config::resolve_runtime_config;
use crate::runtime::boot_dashboard::build_dashboard_bundle;
use crate::runtime::boot_engine::build_engine_bundle;
use crate::runtime::boot_memory::build_memory_bundle;
use crate::runtime::boot_profile::build_profile_bundle;
use crate::runtime::boot_provider::build_provider_bundle;
use crate::runtime::slash::{build_slash_handler, SlashHandlerDeps};
use crate::runtime::tools::{register_tools, ToolRegistrationContext};
use forja_core::emotion::EmotionEngine;
use forja_core::error::Result;
use forja_core::mode::{ExecMode, ThinkLevel};
use forja_core::traits::Channel;
use forja_core::Engine;
use std::sync::{Arc, Mutex};

pub(crate) use crate::runtime::boot_config::RuntimeOptions;

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
    let runtime_config = resolve_runtime_config(options);
    let profile = build_profile_bundle(
        &runtime_config.forja_cfg,
        runtime_config.shell_enabled,
        runtime_config.input_enabled,
        runtime_config.browser_enabled,
        runtime_config.vision_enabled,
    )?;
    let provider_bundle = build_provider_bundle(&runtime_config.forja_cfg)?;
    let channel_bundle = build_channel_bundle(&runtime_config.forja_cfg).await;
    let dashboard_bundle = build_dashboard_bundle(
        &runtime_config.forja_cfg,
        &profile.bootstrap_paths,
        channel_bundle.telegram_status_provider,
    );
    let autonomy_runtime = build_autonomy_runtime(&runtime_config.forja_cfg)?;
    let mut engine_bundle = build_engine_bundle(
        provider_bundle.provider.clone(),
        channel_bundle.channel.clone(),
        &runtime_config,
        &profile,
        dashboard_bundle.dashboard_handler,
        dashboard_bundle.tui_handler,
        autonomy_runtime,
    )?;
    let memory_bundle = build_memory_bundle(
        provider_bundle.provider.clone(),
        engine_bundle.knowledge_manager.clone(),
        profile.assistant_name.as_str(),
        profile.user_title.as_str(),
        profile.bootstrap_greeting.clone(),
        engine_bundle.serendipity_enabled,
    )
    .await?;
    engine_bundle.engine = engine_bundle
        .engine
        .with_emotion(EmotionEngine::new(memory_bundle.restored_mood));

    let exec_mode_handle = Arc::new(Mutex::new(runtime_config.exec_mode));
    let tool_runtime = register_tools(
        &mut engine_bundle.engine,
        ToolRegistrationContext {
            forja_cfg: &runtime_config.forja_cfg,
            exec_mode_handle: exec_mode_handle.clone(),
            llm_config: provider_bundle.llm_config.as_ref(),
            use_mock: provider_bundle.use_mock,
            shell_enabled: runtime_config.shell_enabled,
            input_enabled: runtime_config.input_enabled,
            browser_enabled: runtime_config.browser_enabled,
            vision_enabled: runtime_config.vision_enabled,
        },
    )
    .await;

    let slash_handler = build_slash_handler(SlashHandlerDeps {
        cfg_for_handler: runtime_config.forja_cfg.clone(),
        registry: provider_bundle.registry,
        channel: channel_bundle.channel.clone(),
        bootstrap_paths: profile.bootstrap_paths.clone(),
        interactive_identity_supported: channel_bundle.interactive_identity_supported,
        exec_mode_handle,
        vision_enabled: runtime_config.vision_enabled,
        capture_backend: tool_runtime.capture_backend,
        vision_analyzer: tool_runtime.vision_analyzer,
        state_change_confirmer: None,
        skill_registry: engine_bundle.skill_registry.clone(),
    });
    let engine = engine_bundle
        .engine
        .with_memory(memory_bundle.memory_store)
        .with_slash_handler(slash_handler);

    Ok(AppRuntime {
        engine,
        dashboard_server: dashboard_bundle.dashboard_server,
        channel: channel_bundle.channel,
        provider_info: runtime_config.provider_info,
        loaded_project_file: profile.loaded_project_file,
        assistant_name: profile.assistant_name,
        displayed_greeting: memory_bundle.displayed_greeting,
        print_initial_prompt: channel_bundle.print_initial_prompt,
        exec_mode: runtime_config.exec_mode,
        think_level: runtime_config.think_level,
    })
}
