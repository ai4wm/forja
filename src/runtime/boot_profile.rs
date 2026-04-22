use crate::bootstrap::{self, BootstrapPaths};
use crate::config::ForjaConfig;
use crate::runtime::prompt::{build_system_prompt, build_tool_prompt};
use forja_core::error::{ForjaError, Result};

pub(crate) struct ProfileBundle {
    pub(crate) bootstrap_paths: BootstrapPaths,
    pub(crate) combined_prompt: String,
    pub(crate) loaded_project_file: Option<String>,
    pub(crate) tool_prompt: String,
    pub(crate) assistant_name: String,
    pub(crate) user_title: String,
    pub(crate) bootstrap_greeting: Option<String>,
}

pub(crate) fn build_profile_bundle(
    forja_cfg: &ForjaConfig,
    shell_enabled: bool,
    input_enabled: bool,
    browser_enabled: bool,
    vision_enabled: bool,
) -> Result<ProfileBundle> {
    let bootstrap_paths = bootstrap::default_paths();
    let bootstrap_outcome = bootstrap::ensure_bootstrap(&bootstrap_paths)
        .map_err(|error| ForjaError::Internal(error.to_string()))?;
    let (combined_prompt, loaded_project_file) = build_system_prompt(&bootstrap_paths)
        .map_err(|error| ForjaError::Internal(error.to_string()))?;
    let tool_prompt = build_tool_prompt(
        shell_enabled,
        input_enabled,
        browser_enabled,
        vision_enabled,
    );

    Ok(ProfileBundle {
        assistant_name: configured_name(
            forja_cfg.assistant_name.clone(),
            "FORJA_ASSISTANT_NAME",
            &bootstrap_outcome.profile.identity.name,
        ),
        user_title: configured_name(
            forja_cfg.user_title.clone(),
            "FORJA_USER_TITLE",
            &bootstrap_outcome.profile.user.name,
        ),
        bootstrap_greeting: bootstrap_outcome.greeting,
        bootstrap_paths,
        combined_prompt,
        loaded_project_file,
        tool_prompt,
    })
}

fn configured_name(configured_value: Option<String>, env_name: &str, fallback: &str) -> String {
    configured_value
        .or_else(|| std::env::var(env_name).ok())
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| fallback.to_string())
}
