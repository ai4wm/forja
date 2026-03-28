use crate::config::{self, ForjaConfig};
use forja_core::{BackgroundManager, LlmProvider, Message, Role};
use forja_llm::{LlmClient, LocalModelProvider, detect_local_models};
use std::path::Path;
use std::sync::Arc;

const GROQ_FREE_MODELS: &[&str] = &["llama-3.1-8b-instant", "gemma2-9b-it"];
const GEMINI_FREE_MODELS: &[&str] = &["gemini-3-flash-preview", "gemini-2.5-flash"];
const OPENROUTER_FREE_MODELS: &[&str] = &[
    "meta-llama/llama-3.1-8b-instruct:free",
    "google/gemma-2-9b-it:free",
];
const OLLAMA_DEFAULT_MODEL: &str = "qwen3.5:9b";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackgroundCandidate {
    pub provider: String,
    pub model: String,
    pub kind: String,
}

impl BackgroundCandidate {
    fn new(provider: &str, model: &str, kind: &str) -> Self {
        Self {
            provider: provider.to_string(),
            model: model.to_string(),
            kind: kind.to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackgroundStatusSnapshot {
    pub provider: String,
    pub model: String,
    pub interval_seconds: u64,
    pub active: bool,
    pub note: String,
}

impl BackgroundStatusSnapshot {
    pub fn disabled(interval_seconds: u64, note: &str) -> Self {
        Self {
            provider: "disabled".to_string(),
            model: String::new(),
            interval_seconds,
            active: false,
            note: note.to_string(),
        }
    }

    pub fn selected(candidate: &BackgroundCandidate, interval_seconds: u64, active: bool) -> Self {
        Self {
            provider: candidate.provider.clone(),
            model: candidate.model.clone(),
            interval_seconds,
            active,
            note: candidate.kind.clone(),
        }
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn message(&self) -> String {
        if self.provider == "disabled" {
            return format!(
                "Background model: disabled ({})",
                self.note
            );
        }

        format!(
            "Background model: {}/{} ({}) | interval={}s | active={}",
            self.provider,
            self.model,
            self.note,
            self.interval_seconds,
            self.active
        )
    }
}

pub enum BackgroundDiscovery {
    Disabled(String),
    Selected {
        candidate: BackgroundCandidate,
        provider: Arc<dyn LlmProvider>,
    },
}

pub fn build_background_candidates(
    cfg: &ForjaConfig,
    home_dir: &Path,
) -> std::io::Result<Vec<BackgroundCandidate>> {
    let local_models = detect_local_models(home_dir)?;
    for model in &local_models {
        println!("Local model found: {}", model.file_name);
    }

    let configured_provider = cfg.background.provider.trim().to_lowercase();
    let configured_model = cfg.background.model.trim();

    let mut candidates = Vec::new();
    match configured_provider.as_str() {
        "off" => {}
        "auto" => {
            candidates.extend(GROQ_FREE_MODELS.iter().map(|model| {
                BackgroundCandidate::new("groq", model, "free")
            }));
            candidates.extend(GEMINI_FREE_MODELS.iter().map(|model| {
                BackgroundCandidate::new("gemini", model, "free")
            }));
            candidates.extend(OPENROUTER_FREE_MODELS.iter().map(|model| {
                BackgroundCandidate::new("openrouter", model, "free")
            }));
            candidates.push(BackgroundCandidate::new("ollama", OLLAMA_DEFAULT_MODEL, "local"));
            if let Some(local_model) = local_models.first() {
                candidates.push(BackgroundCandidate::new(
                    "local",
                    &local_model.file_name,
                    "local",
                ));
            }
        }
        "groq" => {
            let model = if configured_model.is_empty() {
                GROQ_FREE_MODELS[0]
            } else {
                configured_model
            };
            candidates.push(BackgroundCandidate::new("groq", model, "free"));
        }
        "gemini" => {
            let model = if configured_model.is_empty() {
                GEMINI_FREE_MODELS[0]
            } else {
                configured_model
            };
            candidates.push(BackgroundCandidate::new("gemini", model, "free"));
        }
        "openrouter" => {
            let model = if configured_model.is_empty() {
                OPENROUTER_FREE_MODELS[0]
            } else {
                configured_model
            };
            candidates.push(BackgroundCandidate::new("openrouter", model, "free"));
        }
        "ollama" => {
            let model = if configured_model.is_empty() {
                OLLAMA_DEFAULT_MODEL
            } else {
                configured_model
            };
            candidates.push(BackgroundCandidate::new("ollama", model, "local"));
        }
        "local" => {
            if configured_model.is_empty() {
                if let Some(local_model) = local_models.first() {
                    candidates.push(BackgroundCandidate::new(
                        "local",
                        &local_model.file_name,
                        "local",
                    ));
                }
            } else {
                candidates.push(BackgroundCandidate::new("local", configured_model, "local"));
            }
        }
        other => {
            let model = if configured_model.is_empty() { "" } else { configured_model };
            if !model.is_empty() {
                candidates.push(BackgroundCandidate::new(other, model, "custom"));
            }
        }
    }

    Ok(candidates)
}

pub async fn discover_background_provider(
    cfg: &ForjaConfig,
    home_dir: &Path,
) -> BackgroundDiscovery {
    let candidates = match build_background_candidates(cfg, home_dir) {
        Ok(candidates) => candidates,
        Err(error) => {
            return BackgroundDiscovery::Disabled(format!(
                "local model discovery failed: {error}"
            ));
        }
    };

    if candidates.is_empty() {
        return BackgroundDiscovery::Disabled("no free provider available".to_string());
    }

    for candidate in candidates {
        if let Some(provider) = probe_candidate(cfg, home_dir, &candidate).await {
            return BackgroundDiscovery::Selected { candidate, provider };
        }
    }

    BackgroundDiscovery::Disabled("no free provider available".to_string())
}

pub async fn apply_background_candidate(
    manager: &mut BackgroundManager,
    candidate: &BackgroundCandidate,
    provider: Arc<dyn LlmProvider>,
    interval_seconds: u64,
) {
    manager.stop().await;
    manager.configure(
        candidate.provider.clone(),
        candidate.model.clone(),
        provider,
        interval_seconds,
    );
    manager.start();
}

async fn probe_candidate(
    cfg: &ForjaConfig,
    home_dir: &Path,
    candidate: &BackgroundCandidate,
) -> Option<Arc<dyn LlmProvider>> {
    match candidate.provider.as_str() {
        "local" => {
            let model = detect_local_models(home_dir)
                .ok()?
                .into_iter()
                .find(|model| model.file_name == candidate.model)?;
            Some(Arc::new(LocalModelProvider::new(model)))
        }
        _ => {
            let mut probe_cfg = cfg.clone();
            probe_cfg.active.provider = Some(candidate.provider.clone());
            probe_cfg.active.model = Some(candidate.model.clone());

            let mut llm_config = config::llm_config_from(&probe_cfg).ok()?;
            llm_config.max_tokens = 1;

            let provider: Arc<dyn LlmProvider> = Arc::new(LlmClient::new(llm_config).ok()?);
            let probe_result = provider
                .chat(&[Message::text(Role::User, "ping", None)], None)
                .await;
            if probe_result.is_ok() {
                Some(provider)
            } else {
                None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_temp_dir(name: &str) -> std::path::PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        std::env::temp_dir().join(format!("forja_background_{name}_{nanos}"))
    }

    #[test]
    fn build_background_candidates_uses_auto_order() {
        let home_dir = unique_temp_dir("auto_order");
        let models_dir = forja_llm::ensure_models_dir(&home_dir).unwrap();
        std::fs::write(models_dir.join("phi-4-mini.gguf"), "stub").unwrap();
        let cfg = ForjaConfig::default();

        let candidates = build_background_candidates(&cfg, &home_dir).unwrap();
        let providers = candidates
            .iter()
            .map(|candidate| candidate.provider.as_str())
            .collect::<Vec<_>>();

        assert_eq!(
            providers,
            vec![
                "groq",
                "groq",
                "gemini",
                "gemini",
                "openrouter",
                "openrouter",
                "ollama",
                "local",
            ]
        );

        let _ = std::fs::remove_dir_all(home_dir);
    }

    #[test]
    fn build_background_candidates_respects_explicit_provider_and_model() {
        let home_dir = unique_temp_dir("explicit");
        let mut cfg = ForjaConfig::default();
        cfg.background.provider = "openrouter".to_string();
        cfg.background.model = "meta-llama/llama-3.1-8b-instruct:free".to_string();

        let candidates = build_background_candidates(&cfg, &home_dir).unwrap();

        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].provider, "openrouter");
        assert_eq!(
            candidates[0].model,
            "meta-llama/llama-3.1-8b-instruct:free"
        );
    }

    #[test]
    fn background_status_snapshot_formats_disabled_message() {
        let status = BackgroundStatusSnapshot::disabled(30, "no free provider available");

        assert_eq!(
            status.message(),
            "Background model: disabled (no free provider available)"
        );
    }
}
