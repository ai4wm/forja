use forja_llm::LlmConfig;
use crate::config::{ForjaConfig, llm_config_from};

// Model entries

pub struct ModelEntry {
    pub provider: &'static str,
    pub model_id: &'static str,
    pub label:    &'static str,
    pub aliases:  &'static [&'static str],
}

/// Full registered model table using current model IDs.
pub static MODEL_TABLE: &[ModelEntry] = &[
    // OpenAI API
    ModelEntry { provider: "openai",       model_id: "gpt-5.4",              label: "GPT-5.4 (API paid)",           aliases: &["smart", "gpt5"] },
    ModelEntry { provider: "openai",       model_id: "gpt-5.4-mini",         label: "GPT-5.4 Mini (API paid)",      aliases: &["mini"] },
    ModelEntry { provider: "openai",       model_id: "gpt-5.3-codex",        label: "GPT-5.3 Codex (API paid)",     aliases: &["codex"] },

    // OpenAI OAuth (subscription)
    ModelEntry { provider: "openai_oauth", model_id: "gpt-5.4",              label: "GPT-5.4 (subscription ★)",           aliases: &["smart5"] },
    ModelEntry { provider: "openai_oauth", model_id: "gpt-5.3-codex",       label: "GPT-5.3 Codex (subscription)",        aliases: &["codex53"] },
    ModelEntry { provider: "openai_oauth", model_id: "gpt-5.3-codex-spark", label: "GPT-5.3 Codex Spark (subscription, ultra-fast)", aliases: &["spark"] },
    ModelEntry { provider: "openai_oauth", model_id: "o3-pro",              label: "o3-Pro (subscription)",                aliases: &["o3pro"] },

    // Anthropic
    ModelEntry { provider: "anthropic",    model_id: "claude-opus-4-6",      label: "Claude Opus 4.6 (API paid)",   aliases: &["opus"] },
    ModelEntry { provider: "anthropic",    model_id: "claude-sonnet-4-6",    label: "Claude Sonnet 4.6 (API paid)", aliases: &["sonnet"] },

    // Gemini API
    ModelEntry { provider: "gemini",       model_id: "gemini-3.1-pro-preview", label: "Gemini 3.1 Pro (API paid ★)",  aliases: &["gemini", "pro31"] },
    ModelEntry { provider: "gemini",       model_id: "gemini-3-flash-preview", label: "Gemini 3 Flash (free)",      aliases: &["flash", "flash3"] },
    ModelEntry { provider: "gemini",       model_id: "gemini-3.1-flash-lite-preview", label: "Gemini 3.1 Flash-Lite (free)", aliases: &["lite"] },
    ModelEntry { provider: "gemini",       model_id: "gemini-2.5-pro",         label: "Gemini 2.5 Pro (free)",      aliases: &["gemini25"] },
    ModelEntry { provider: "gemini",       model_id: "gemini-2.5-flash",       label: "Gemini 2.5 Flash (free)",    aliases: &["flash25"] },

    // Gemini OAuth (CLI subscription)
    ModelEntry { provider: "gemini_oauth", model_id: "gemini-3.1-pro-preview", label: "Gemini 3.1 Pro (CLI subscription ★)",  aliases: &["gempro31"] },
    ModelEntry { provider: "gemini_oauth", model_id: "gemini-3-flash-preview", label: "Gemini 3 Flash (CLI subscription)",    aliases: &["gemflash3"] },
    ModelEntry { provider: "gemini_oauth", model_id: "gemini-2.5-pro",         label: "Gemini 2.5 Pro (CLI subscription)",   aliases: &["gempro"] },
    ModelEntry { provider: "gemini_oauth", model_id: "gemini-2.5-flash",       label: "Gemini 2.5 Flash (CLI subscription)",  aliases: &["gemflash"] },

    // Groq
    ModelEntry { provider: "groq",         model_id: "llama-3.1-8b-instant",   label: "Llama 3.1 8B Instant (free)",  aliases: &["groq", "groq-llama"] },
    ModelEntry { provider: "groq",         model_id: "gemma2-9b-it",           label: "Gemma 2 9B IT (free)",         aliases: &["groq-gemma"] },

    // OpenRouter
    ModelEntry { provider: "openrouter",   model_id: "meta-llama/llama-3.1-8b-instruct:free", label: "Llama 3.1 8B Instruct (free)", aliases: &["or-llama-free"] },
    ModelEntry { provider: "openrouter",   model_id: "google/gemma-2-9b-it:free",             label: "Gemma 2 9B IT (free)",         aliases: &["or-gemma-free"] },

    // DeepSeek
    ModelEntry { provider: "deepseek",     model_id: "deepseek-chat",          label: "DeepSeek V3.2 (API paid)",   aliases: &["ds"] },
    ModelEntry { provider: "deepseek",     model_id: "deepseek-reasoner",      label: "DeepSeek R1 (API paid)",     aliases: &["dsr"] },

    // GLM
    ModelEntry { provider: "glm",          model_id: "glm-5",                  label: "GLM-5 (API paid)",           aliases: &["glm"] },
    ModelEntry { provider: "glm",          model_id: "glm-4.5v",               label: "GLM-4.5V (API paid)",        aliases: &["glmv"] },

    // Moonshot
    ModelEntry { provider: "moonshot",     model_id: "kimi-k2.5",              label: "Kimi K2.5 (API paid)",       aliases: &["kimi", "fast"] },

    // xAI
    ModelEntry { provider: "xai",          model_id: "grok-3",                 label: "Grok-3 (API paid)",          aliases: &["grok"] },
    ModelEntry { provider: "xai",          model_id: "grok-3-mini",            label: "Grok-3 Mini (API paid)",     aliases: &["grokmini"] },

    // Ollama
    ModelEntry { provider: "ollama",       model_id: "qwen3.5:9b",             label: "Qwen3.5 9B (local)",         aliases: &["local", "ollama"] },
    ModelEntry { provider: "ollama",       model_id: "llama3:8b",               label: "Llama3 8B (local)",          aliases: &["llama"] },
    ModelEntry { provider: "ollama",       model_id: "mistral:7b",              label: "Mistral 7B (local)",         aliases: &["mistral"] },
];

// ProviderRegistry

pub struct ProviderRegistry {
    active_idx: usize,
}

impl ProviderRegistry {
    /// Initialize from the active model index found in config.
    pub fn from_config(cfg: &ForjaConfig) -> Self {
        let provider = cfg.active.provider.as_deref().unwrap_or("");
        let model    = cfg.active.model.as_deref().unwrap_or("");

        let idx = MODEL_TABLE.iter().position(|e| {
            e.provider == provider && e.model_id == model
        }).or_else(|| {
            MODEL_TABLE.iter().position(|e| e.provider == provider)
        }).unwrap_or(0);

        Self { active_idx: idx }
    }

    /// Current active entry.
    pub fn active(&self) -> &'static ModelEntry {
        &MODEL_TABLE[self.active_idx]
    }

    /// Check whether a provider is available.
    fn is_provider_available(provider: &str, cfg: &ForjaConfig, auth: &crate::oauth::AuthData) -> bool {
        match provider {
            "ollama" => true,
            "openai_oauth" => auth.openai.is_some(),
            "gemini_oauth" => auth.gemini.is_some(),
            _ => cfg.keys.get_for(provider).is_some(),
        }
    }

    /// Render `/models` using only providers available in config.
    pub fn list_for_config(&self, cfg: &ForjaConfig) -> String {
        let mut s = String::from("Available models (configured providers):\n");
        let mut display_idx = 1usize;
        let auth = crate::oauth::AuthData::load();
        for (i, e) in MODEL_TABLE.iter().enumerate() {
            if !Self::is_provider_available(e.provider, cfg, &auth) { continue; }
            let cur = if i == self.active_idx { " <- current" } else { "" };
            s.push_str(&format!(
                "  {:2}. [{}] {} — {}{}\n",
                display_idx, e.provider, e.label, e.model_id, cur
            ));
            display_idx += 1;
        }
        s.push_str("\nUse `/model <number>` or `/model <name/alias>` to switch.");
        s
    }

    /// Render `/models` for the full table regardless of availability.
    #[allow(dead_code)]
    pub fn list_display(&self) -> String {
        let mut s = String::from("All models:\n");
        for (i, e) in MODEL_TABLE.iter().enumerate() {
            let cur = if i == self.active_idx { " <- current" } else { "" };
            s.push_str(&format!(
                "  {:2}. [{}] {} — {}{}\\n",
                i + 1, e.provider, e.label, e.model_id, cur
            ));
        }
        s.push_str("\nUse `/model <number>` or `/model <name/alias>` to switch.");
        s
    }

    /// Resolve `/model <input>` by number, model ID, alias, or substring.
    pub fn resolve(&self, input: &str, cfg: &ForjaConfig) -> Option<usize> {
        let input = input.trim().to_lowercase();
        let auth = crate::oauth::AuthData::load();

        if let Some((provider, model)) = input.split_once('/')
            && let Some((idx, _)) = MODEL_TABLE.iter().enumerate().find(|(_, e)| {
                e.provider == provider
                    && e.model_id == model
                    && Self::is_provider_available(e.provider, cfg, &auth)
            })
        {
            return Some(idx);
        }

        #[allow(clippy::collapsible_if)]
        if let Ok(n) = input.parse::<usize>() {
            let mut available_count = 0;
            for (i, e) in MODEL_TABLE.iter().enumerate() {
                if Self::is_provider_available(e.provider, cfg, &auth) {
                    available_count += 1;
                    if available_count == n {
                        return Some(i);
                    }
                }
            }
        }

        if let Some((idx, _)) = MODEL_TABLE.iter().enumerate()
            .find(|(_, e)| e.model_id == input && Self::is_provider_available(e.provider, cfg, &auth)) {
            return Some(idx);
        }

        if let Some((idx, _)) = MODEL_TABLE.iter().enumerate()
            .find(|(_, e)| e.aliases.contains(&input.as_str()) && Self::is_provider_available(e.provider, cfg, &auth)) {
            return Some(idx);
        }

        MODEL_TABLE.iter().enumerate()
            .find(|(_, e)| e.model_id.contains(input.as_str()) && Self::is_provider_available(e.provider, cfg, &auth))
            .map(|(idx, _)| idx)
    }

    /// Switch and return the new LlmConfig.
    pub fn switch_to(&mut self, idx: usize, cfg: &ForjaConfig) -> Result<LlmConfig, String> {
        let entry = &MODEL_TABLE[idx];
        let mut tmp = cfg.clone();
        tmp.active.provider = Some(entry.provider.to_string());
        tmp.active.model    = Some(entry.model_id.to_string());
        let lc = llm_config_from(&tmp)?;
        self.active_idx = idx;
        Ok(lc)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn configured() -> ForjaConfig {
        let mut cfg = ForjaConfig::default();
        cfg.keys.groq = Some("groq-key".to_string());
        cfg.keys.openrouter = Some("or-key".to_string());
        cfg
    }

    #[test]
    fn resolve_accepts_provider_slash_model_for_groq() {
        let cfg = configured();
        let registry = ProviderRegistry::from_config(&cfg);

        let idx = registry
            .resolve("groq/llama-3.1-8b-instant", &cfg)
            .expect("groq direct target should resolve");

        assert_eq!(MODEL_TABLE[idx].provider, "groq");
        assert_eq!(MODEL_TABLE[idx].model_id, "llama-3.1-8b-instant");
    }

    #[test]
    fn resolve_accepts_provider_slash_model_for_openrouter_free_suffix() {
        let cfg = configured();
        let registry = ProviderRegistry::from_config(&cfg);

        let idx = registry
            .resolve("openrouter/meta-llama/llama-3.1-8b-instruct:free", &cfg)
            .expect("openrouter direct target should resolve");

        assert_eq!(MODEL_TABLE[idx].provider, "openrouter");
    }
}


