use crate::config::{self, ForjaConfig};
use crate::local_models::LOCAL_PROVIDER;
use forja_core::mode::{ExecMode, ThinkLevel};
use std::io::Write;

#[derive(Clone)]
pub(crate) struct RuntimeOptions {
    pub(crate) force_setup: bool,
    pub(crate) new_provider: Option<String>,
    pub(crate) new_model: Option<String>,
}

pub(crate) struct RuntimeConfig {
    pub(crate) forja_cfg: ForjaConfig,
    pub(crate) provider_info: String,
    pub(crate) shell_enabled: bool,
    pub(crate) input_enabled: bool,
    pub(crate) browser_enabled: bool,
    pub(crate) vision_enabled: bool,
    pub(crate) exec_mode: ExecMode,
    pub(crate) think_level: ThinkLevel,
}

pub(crate) fn resolve_runtime_config(options: RuntimeOptions) -> RuntimeConfig {
    let mut forja_cfg = if options.force_setup {
        config::run_onboarding()
    } else {
        config::load_config()
    };

    if forja_cfg.active.provider.is_none() && !options.force_setup {
        forja_cfg = config::run_onboarding();
    }

    apply_runtime_overrides(&mut forja_cfg, options.new_provider, options.new_model);

    RuntimeConfig {
        provider_info: config::provider_info(&forja_cfg),
        shell_enabled: env_enabled("FORJA_SHELL"),
        input_enabled: env_enabled("FORJA_INPUT"),
        browser_enabled: env_enabled("FORJA_BROWSER"),
        vision_enabled: env_enabled("FORJA_VISION"),
        exec_mode: parse_exec_mode(),
        think_level: parse_think_level(),
        forja_cfg,
    }
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

        if forja_cfg.keys.get_for(&provider).is_none()
            && provider != "ollama"
            && provider != LOCAL_PROVIDER
        {
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

fn env_enabled(name: &str) -> bool {
    !matches!(
        std::env::var(name),
        Ok(value) if value.eq_ignore_ascii_case("false")
    )
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
