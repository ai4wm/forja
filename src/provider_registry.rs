use crate::config::{ForjaConfig, llm_config_from};
use crate::local_models::{LOCAL_PROVIDER, discover_local_models, has_local_models};
use forja_llm::LlmConfig;

pub struct ModelEntry {
    pub provider: &'static str,
    pub model_id: &'static str,
    pub label: &'static str,
    pub aliases: &'static [&'static str],
}

#[derive(Clone, Debug)]
pub struct RuntimeModelEntry {
    pub provider: String,
    pub model_id: String,
    pub label: String,
    pub aliases: Vec<String>,
}

pub static MODEL_TABLE: &[ModelEntry] = &[
    ModelEntry {
        provider: "openai",
        model_id: "gpt-5.4",
        label: "GPT-5.4 (API paid)",
        aliases: &["smart", "gpt5"],
    },
    ModelEntry {
        provider: "openai",
        model_id: "gpt-5.4-mini",
        label: "GPT-5.4 Mini (API paid)",
        aliases: &["mini"],
    },
    ModelEntry {
        provider: "openai",
        model_id: "gpt-5.3-codex",
        label: "GPT-5.3 Codex (API paid)",
        aliases: &["codex"],
    },
    ModelEntry {
        provider: "openai_oauth",
        model_id: "gpt-5.4",
        label: "GPT-5.4 (subscription ★)",
        aliases: &["smart5"],
    },
    ModelEntry {
        provider: "openai_oauth",
        model_id: "gpt-5.3-codex",
        label: "GPT-5.3 Codex (subscription)",
        aliases: &["codex53"],
    },
    ModelEntry {
        provider: "openai_oauth",
        model_id: "gpt-5.3-codex-spark",
        label: "GPT-5.3 Codex Spark (subscription, ultra-fast)",
        aliases: &["spark"],
    },
    ModelEntry {
        provider: "openai_oauth",
        model_id: "o3-pro",
        label: "o3-Pro (subscription)",
        aliases: &["o3pro"],
    },
    ModelEntry {
        provider: "anthropic",
        model_id: "claude-opus-4-6",
        label: "Claude Opus 4.6 (API paid)",
        aliases: &["opus"],
    },
    ModelEntry {
        provider: "anthropic",
        model_id: "claude-sonnet-4-6",
        label: "Claude Sonnet 4.6 (API paid)",
        aliases: &["sonnet"],
    },
    ModelEntry {
        provider: "gemini",
        model_id: "gemini-3.1-pro-preview",
        label: "Gemini 3.1 Pro (API paid ★)",
        aliases: &["gemini", "pro31"],
    },
    ModelEntry {
        provider: "gemini",
        model_id: "gemini-3-flash-preview",
        label: "Gemini 3 Flash (free)",
        aliases: &["flash", "flash3"],
    },
    ModelEntry {
        provider: "gemini",
        model_id: "gemini-3.1-flash-lite-preview",
        label: "Gemini 3.1 Flash-Lite (free)",
        aliases: &["lite"],
    },
    ModelEntry {
        provider: "gemini",
        model_id: "gemini-2.5-pro",
        label: "Gemini 2.5 Pro (free)",
        aliases: &["gemini25"],
    },
    ModelEntry {
        provider: "gemini",
        model_id: "gemini-2.5-flash",
        label: "Gemini 2.5 Flash (free)",
        aliases: &["flash25"],
    },
    ModelEntry {
        provider: "gemini_oauth",
        model_id: "gemini-3.1-pro-preview",
        label: "Gemini 3.1 Pro (CLI subscription ★)",
        aliases: &["gempro31"],
    },
    ModelEntry {
        provider: "gemini_oauth",
        model_id: "gemini-3-flash-preview",
        label: "Gemini 3 Flash (CLI subscription)",
        aliases: &["gemflash3"],
    },
    ModelEntry {
        provider: "gemini_oauth",
        model_id: "gemini-2.5-pro",
        label: "Gemini 2.5 Pro (CLI subscription)",
        aliases: &["gempro"],
    },
    ModelEntry {
        provider: "gemini_oauth",
        model_id: "gemini-2.5-flash",
        label: "Gemini 2.5 Flash (CLI subscription)",
        aliases: &["gemflash"],
    },
    ModelEntry {
        provider: "deepseek",
        model_id: "deepseek-chat",
        label: "DeepSeek V3.2 (API paid)",
        aliases: &["ds"],
    },
    ModelEntry {
        provider: "deepseek",
        model_id: "deepseek-reasoner",
        label: "DeepSeek R1 (API paid)",
        aliases: &["dsr"],
    },
    ModelEntry {
        provider: "glm",
        model_id: "glm-5",
        label: "GLM-5 (API paid)",
        aliases: &["glm"],
    },
    ModelEntry {
        provider: "glm",
        model_id: "glm-4.5v",
        label: "GLM-4.5V (API paid)",
        aliases: &["glmv"],
    },
    ModelEntry {
        provider: "moonshot",
        model_id: "kimi-k2.5",
        label: "Kimi K2.5 (API paid)",
        aliases: &["kimi", "fast"],
    },
    ModelEntry {
        provider: "xai",
        model_id: "grok-3",
        label: "Grok-3 (API paid)",
        aliases: &["grok"],
    },
    ModelEntry {
        provider: "xai",
        model_id: "grok-3-mini",
        label: "Grok-3 Mini (API paid)",
        aliases: &["grokmini"],
    },
    ModelEntry {
        provider: "ollama",
        model_id: "qwen3.5:9b",
        label: "Qwen3.5 9B (local)",
        aliases: &["local", "ollama"],
    },
    ModelEntry {
        provider: "ollama",
        model_id: "llama3:8b",
        label: "Llama3 8B (local)",
        aliases: &["llama"],
    },
    ModelEntry {
        provider: "ollama",
        model_id: "mistral:7b",
        label: "Mistral 7B (local)",
        aliases: &["mistral"],
    },
];

pub struct ProviderRegistry {
    available_entries: Vec<RuntimeModelEntry>,
    active_entry: RuntimeModelEntry,
}

impl ProviderRegistry {
    pub fn from_config(cfg: &ForjaConfig) -> Self {
        let auth = crate::oauth::AuthData::load();
        let available_entries = build_available_entries(cfg, &auth);
        let active_entry = resolve_active_entry(cfg, &available_entries);

        Self {
            available_entries,
            active_entry,
        }
    }

    pub fn active(&self) -> &RuntimeModelEntry {
        &self.active_entry
    }

    fn is_provider_available(
        provider: &str,
        cfg: &ForjaConfig,
        auth: &crate::oauth::AuthData,
    ) -> bool {
        match provider {
            "ollama" => true,
            LOCAL_PROVIDER => has_local_models(),
            "openai_oauth" => auth.openai.is_some(),
            "gemini_oauth" => auth.gemini.is_some(),
            _ => cfg.keys.get_for(provider).is_some(),
        }
    }

    pub fn list_for_config(&self, _cfg: &ForjaConfig) -> String {
        let mut output = String::from("Available models (configured providers):\n");
        for (index, entry) in self.available_entries.iter().enumerate() {
            let current = if is_same_entry(entry, &self.active_entry) {
                " <- current"
            } else {
                ""
            };
            output.push_str(&format!(
                "  {:2}. [{}] {} — {}{}\n",
                index + 1,
                entry.provider,
                entry.label,
                entry.model_id,
                current
            ));
        }
        output.push_str("\nUse `/model <number>` or `/model <name/alias>` to switch.");
        output
    }

    pub fn resolve(&self, input: &str, _cfg: &ForjaConfig) -> Option<usize> {
        let normalized = input.trim().to_lowercase();
        if let Ok(number) = normalized.parse::<usize>() {
            return number
                .checked_sub(1)
                .filter(|index| *index < self.available_entries.len());
        }

        self.available_entries.iter().position(|entry| {
            entry.model_id.eq_ignore_ascii_case(&normalized)
                || entry
                    .aliases
                    .iter()
                    .any(|alias| alias.eq_ignore_ascii_case(&normalized))
                || entry.label.to_lowercase().contains(&normalized)
        })
    }

    pub fn switch_to(&mut self, index: usize, cfg: &ForjaConfig) -> Result<LlmConfig, String> {
        let entry = self
            .available_entries
            .get(index)
            .cloned()
            .ok_or_else(|| format!("Invalid model index: {index}"))?;
        let mut temp = cfg.clone();
        temp.active.provider = Some(entry.provider.clone());
        temp.active.model = Some(entry.model_id.clone());
        let config = llm_config_from(&temp)?;
        self.active_entry = entry;
        Ok(config)
    }

    pub fn refresh(&mut self, cfg: &ForjaConfig) {
        let refreshed = Self::from_config(cfg);
        self.available_entries = refreshed.available_entries;
        self.active_entry = refreshed.active_entry;
    }
}

fn build_available_entries(
    cfg: &ForjaConfig,
    auth: &crate::oauth::AuthData,
) -> Vec<RuntimeModelEntry> {
    let mut entries = MODEL_TABLE
        .iter()
        .filter(|entry| ProviderRegistry::is_provider_available(entry.provider, cfg, auth))
        .map(runtime_entry_from_static)
        .collect::<Vec<_>>();
    entries.extend(runtime_local_entries());
    entries
}

fn resolve_active_entry(
    cfg: &ForjaConfig,
    available_entries: &[RuntimeModelEntry],
) -> RuntimeModelEntry {
    let provider = cfg.active.provider.as_deref().unwrap_or("");
    let model = cfg.active.model.as_deref().unwrap_or("");

    if let Some(entry) = available_entries
        .iter()
        .find(|entry| entry.provider == provider && entry.model_id == model)
    {
        return entry.clone();
    }

    if let Some(entry) = MODEL_TABLE
        .iter()
        .find(|entry| entry.provider == provider && entry.model_id == model)
    {
        return runtime_entry_from_static(entry);
    }

    if provider == LOCAL_PROVIDER
        && let Ok(Some(model)) = crate::local_models::resolve_local_model(model)
    {
        return runtime_entry_from_local(&model);
    }

    available_entries
        .first()
        .cloned()
        .unwrap_or_else(|| runtime_entry_from_static(&MODEL_TABLE[0]))
}

fn runtime_entry_from_static(entry: &ModelEntry) -> RuntimeModelEntry {
    RuntimeModelEntry {
        provider: entry.provider.to_string(),
        model_id: entry.model_id.to_string(),
        label: entry.label.to_string(),
        aliases: entry
            .aliases
            .iter()
            .map(|alias| alias.to_string())
            .collect(),
    }
}

fn runtime_local_entries() -> Vec<RuntimeModelEntry> {
    discover_local_models()
        .unwrap_or_default()
        .into_iter()
        .map(|model| runtime_entry_from_local(&model))
        .collect()
}

fn runtime_entry_from_local(model: &crate::local_models::LocalModel) -> RuntimeModelEntry {
    RuntimeModelEntry {
        provider: LOCAL_PROVIDER.to_string(),
        model_id: model.model_id.clone(),
        label: model.display_name.clone(),
        aliases: model.aliases.clone(),
    }
}

fn is_same_entry(left: &RuntimeModelEntry, right: &RuntimeModelEntry) -> bool {
    left.provider == right.provider && left.model_id == right.model_id
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with_temp_home<T>(name: &str, test: T)
    where
        T: FnOnce(),
    {
        let _guard = crate::test_support::env_lock().lock().unwrap();
        let temp_dir = std::env::temp_dir().join(format!(
            "forja_registry_{}_{}",
            name,
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        let original = std::env::var("FORJA_HOME_DIR").ok();
        unsafe {
            std::env::set_var("FORJA_HOME_DIR", &temp_dir);
        }
        test();
        if let Some(original) = original {
            unsafe { std::env::set_var("FORJA_HOME_DIR", original) };
        } else {
            unsafe { std::env::remove_var("FORJA_HOME_DIR") };
        }
        let _ = std::fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn provider_registry_lists_local_models_when_present() {
        with_temp_home("local_models", || {
            let models_dir = crate::local_models::models_dir().join("owner--repo");
            std::fs::create_dir_all(&models_dir).unwrap();
            std::fs::write(models_dir.join("tiny.gguf"), b"model").unwrap();

            let registry = ProviderRegistry::from_config(&ForjaConfig::default());
            let listed = registry.list_for_config(&ForjaConfig::default());

            assert!(listed.contains("[llama_cpp]"));
            assert!(listed.contains("tiny (llama.cpp local)"));
        });
    }
}
