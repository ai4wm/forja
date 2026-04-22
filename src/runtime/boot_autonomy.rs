use crate::config::{self, ForjaConfig};
use crate::local_models::{LOCAL_PROVIDER, discover_local_models};
use crate::oauth::AuthData;
use crate::provider_registry::MODEL_TABLE;
use forja_core::autonomy::{AutonomyExecutionRuntime, AutonomyTarget};
use forja_core::error::Result;
use forja_core::traits::LlmProvider;
use forja_llm::LlmClient;
use std::sync::Arc;

const CLOUD_PROVIDER_PRIORITY: &[&str] = &[
    "openai_oauth",
    "openai",
    "anthropic",
    "gemini_oauth",
    "gemini",
    "deepseek",
    "moonshot",
    "xai",
    "glm",
];

pub(crate) fn build_autonomy_runtime(
    forja_cfg: &ForjaConfig,
) -> Result<Option<AutonomyExecutionRuntime>> {
    let local_target = infer_local_target(forja_cfg);
    if local_target.is_none() {
        return Ok(None);
    }

    let cloud_target = infer_cloud_target(forja_cfg);
    let local_monitor = match &local_target {
        Some(target) => Some(build_provider_from_target(forja_cfg, target)?),
        None => None,
    };

    Ok(Some(AutonomyExecutionRuntime {
        local_monitor,
        local_target,
        cloud_target,
        cloud_escalation_requires_confirmation: true,
        cloud_escalation_confirmer: None,
    }))
}

fn infer_local_target(forja_cfg: &ForjaConfig) -> Option<AutonomyTarget> {
    let active_provider = forja_cfg.active.provider.as_deref().unwrap_or_default();
    let active_model = forja_cfg.active.model.as_deref().unwrap_or_default();

    if active_provider == LOCAL_PROVIDER && !active_model.is_empty() {
        return Some(AutonomyTarget {
            provider: LOCAL_PROVIDER.to_string(),
            model: active_model.to_string(),
            label: format!("llama.cpp/{active_model}"),
            local: true,
        });
    }

    if active_provider == "ollama" && !active_model.is_empty() {
        return Some(AutonomyTarget {
            provider: "ollama".to_string(),
            model: active_model.to_string(),
            label: format!("ollama/{active_model}"),
            local: true,
        });
    }

    discover_local_models()
        .ok()
        .and_then(|models| models.into_iter().next())
        .map(|model| AutonomyTarget {
            provider: LOCAL_PROVIDER.to_string(),
            model: model.model_id.clone(),
            label: format!("llama.cpp/{}", model.model_id),
            local: true,
        })
}

fn infer_cloud_target(forja_cfg: &ForjaConfig) -> Option<AutonomyTarget> {
    let active_provider = forja_cfg.active.provider.as_deref().unwrap_or_default();
    let active_model = forja_cfg.active.model.as_deref().unwrap_or_default();
    if !is_local_provider(active_provider)
        && !active_provider.is_empty()
        && !active_model.is_empty()
    {
        return Some(AutonomyTarget {
            provider: active_provider.to_string(),
            model: active_model.to_string(),
            label: format!("{active_provider}/{active_model}"),
            local: false,
        });
    }

    let auth = AuthData::load();
    CLOUD_PROVIDER_PRIORITY.iter().find_map(|provider| {
        if !provider_available(provider, forja_cfg, &auth) {
            return None;
        }
        MODEL_TABLE
            .iter()
            .find(|entry| entry.provider == *provider)
            .map(|entry| AutonomyTarget {
                provider: entry.provider.to_string(),
                model: entry.model_id.to_string(),
                label: format!("{}/{}", entry.provider, entry.model_id),
                local: false,
            })
    })
}

fn build_provider_from_target(
    forja_cfg: &ForjaConfig,
    target: &AutonomyTarget,
) -> Result<Arc<dyn LlmProvider>> {
    let mut temp = forja_cfg.clone();
    temp.active.provider = Some(target.provider.clone());
    temp.active.model = Some(target.model.clone());
    let config = config::llm_config_from(&temp).map_err(forja_core::error::ForjaError::LlmError)?;
    Ok(Arc::new(LlmClient::new(config)?))
}

fn provider_available(provider: &str, cfg: &ForjaConfig, auth: &AuthData) -> bool {
    match provider {
        "openai_oauth" => auth.openai.is_some(),
        "gemini_oauth" => auth.gemini.is_some(),
        "anthropic" => auth.anthropic.is_some() || cfg.keys.get_for(provider).is_some(),
        _ => cfg.keys.get_for(provider).is_some(),
    }
}

fn is_local_provider(provider: &str) -> bool {
    matches!(provider, "ollama" | LOCAL_PROVIDER)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{ActiveSection, ForjaConfig, KeysSection};

    #[test]
    fn build_autonomy_runtime_returns_none_without_local_target() {
        let _guard = crate::test_support::env_lock().lock().unwrap();
        let temp_dir = std::env::temp_dir().join(format!(
            "forja_autonomy_runtime_none_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        let original = std::env::var("FORJA_HOME_DIR").ok();
        unsafe {
            std::env::set_var("FORJA_HOME_DIR", &temp_dir);
        }
        let mut cfg = ForjaConfig::default();
        cfg.active = ActiveSection {
            provider: Some("openai".to_string()),
            model: Some("gpt-5.4".to_string()),
        };
        cfg.keys = KeysSection {
            openai: Some("sk-test".to_string()),
            ..KeysSection::default()
        };

        let runtime = build_autonomy_runtime(&cfg).unwrap();

        assert!(runtime.is_none());

        if let Some(original) = original {
            unsafe { std::env::set_var("FORJA_HOME_DIR", original) };
        } else {
            unsafe { std::env::remove_var("FORJA_HOME_DIR") };
        }
        let _ = std::fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn build_autonomy_runtime_prefers_local_and_cloud_targets_when_both_exist() {
        let _guard = crate::test_support::env_lock().lock().unwrap();
        let temp_dir = std::env::temp_dir().join(format!(
            "forja_autonomy_runtime_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        let model_dir = temp_dir.join(".forja").join("models").join("owner--repo");
        std::fs::create_dir_all(&model_dir).unwrap();
        std::fs::write(model_dir.join("tiny.gguf"), b"model").unwrap();
        let original = std::env::var("FORJA_HOME_DIR").ok();
        unsafe {
            std::env::set_var("FORJA_HOME_DIR", &temp_dir);
        }

        let mut cfg = ForjaConfig::default();
        cfg.active = ActiveSection {
            provider: Some("openai".to_string()),
            model: Some("gpt-5.4".to_string()),
        };
        cfg.keys = KeysSection {
            openai: Some("sk-test".to_string()),
            ..KeysSection::default()
        };

        let runtime = build_autonomy_runtime(&cfg).unwrap().unwrap();

        assert!(runtime.local_target.is_some());
        assert!(runtime.cloud_target.is_some());
        assert!(runtime.local_monitor.is_some());

        if let Some(original) = original {
            unsafe { std::env::set_var("FORJA_HOME_DIR", original) };
        } else {
            unsafe { std::env::remove_var("FORJA_HOME_DIR") };
        }
        let _ = std::fs::remove_dir_all(temp_dir);
    }
}
