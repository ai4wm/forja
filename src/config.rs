use forja_llm::{presets, LlmConfig};
use crate::provider_registry::MODEL_TABLE;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

// Structures

#[derive(Debug, Deserialize, Serialize, Default, Clone)]
pub struct ForjaConfig {
    #[serde(default)]
    pub active: ActiveSection,
    #[serde(default)]
    pub keys: KeysSection,
    pub assistant_name: Option<String>,
    pub user_title: Option<String>,
    #[serde(default)]
    pub agent: AgentSection,
    #[serde(default)]
    pub creation: CreationSection,
    #[serde(default)]
    pub channel: ChannelSection,
    #[serde(default)]
    pub tools: ToolsSection,
}

#[derive(Debug, Deserialize, Serialize, Default, Clone)]
pub struct ActiveSection {
    pub provider: Option<String>,
    pub model: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Default, Clone)]
pub struct ToolsSection {
    #[serde(default)]
    pub search: SearchToolSection,
}

#[derive(Debug, Deserialize, Serialize, Default, Clone)]
pub struct SearchToolSection {
    pub provider: Option<String>,
    pub brave_api_key: Option<String>,
    pub xai_api_key: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Default, Clone)]
pub struct KeysSection {
    pub openai: Option<String>,
    pub anthropic: Option<String>,
    pub gemini: Option<String>,
    pub deepseek: Option<String>,
    pub glm: Option<String>,
    pub moonshot: Option<String>,
    pub xai: Option<String>,
}

impl KeysSection {
    pub fn get_for(&self, provider: &str) -> Option<String> {
        match provider {
            "openai" | "openai_mini" | "openai_oauth" => self.openai.clone(),
            "anthropic" | "anthropic_sonnet" => self.anthropic.clone(),
            "gemini" | "gemini_flash" => self.gemini.clone(),
            "deepseek" | "deepseek_reasoner" => self.deepseek.clone(),
            "glm" | "glm_lite" => self.glm.clone(),
            "moonshot" => self.moonshot.clone(),
            "xai" => self.xai.clone(),
            _ => None,
        }
    }

    pub fn set_for(&mut self, provider: &str, key: String) {
        match provider {
            "openai" | "openai_mini" | "openai_oauth" => self.openai = Some(key),
            "anthropic" | "anthropic_sonnet" => self.anthropic = Some(key),
            "gemini" | "gemini_flash" => self.gemini = Some(key),
            "deepseek" | "deepseek_reasoner" => self.deepseek = Some(key),
            "glm" | "glm_lite" => self.glm = Some(key),
            "moonshot" => self.moonshot = Some(key),
            "xai" => self.xai = Some(key),
            _ => {}
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Default, Clone)]
pub struct AgentSection {
    pub system_prompt: Option<String>,
    pub max_context_tokens: Option<usize>,
    pub monthly_token_limit: Option<usize>,
    pub heartbeat_interval_secs: Option<u64>,
    pub budget_mode: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct CreationSection {
    pub diverge_rounds: Option<usize>,
    pub conflict_rounds: Option<usize>,
    pub converge_rounds: Option<usize>,
    pub max_agents: Option<usize>,
    #[serde(default)]
    pub agents: BTreeMap<String, CreationAgentSection>,
}

#[derive(Debug, Deserialize, Serialize, Default, Clone)]
pub struct CreationAgentSection {
    pub role: Option<String>,
    pub framework: Option<String>,
    pub budget: Option<usize>,
}

impl Default for CreationSection {
    fn default() -> Self {
        Self {
            diverge_rounds: Some(2),
            conflict_rounds: Some(3),
            converge_rounds: Some(1),
            max_agents: Some(5),
            agents: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Default, Clone)]
pub struct ChannelSection {
    #[serde(default)]
    pub telegram: TelegramChannelConfig,
}

#[derive(Debug, Deserialize, Serialize, Default, Clone)]
pub struct TelegramChannelConfig {
    pub bot_token: Option<String>,
    #[serde(default)]
    pub allowed_chat_ids: Vec<i64>,
}

// Path helpers

pub fn config_path() -> PathBuf {
    let base = dirs_next::home_dir().unwrap_or_else(|| PathBuf::from("."));
    base.join(".forja").join("config.toml")
}

// Load

pub fn load_config() -> ForjaConfig {
    let mut config = load_from_file().unwrap_or_default();

    // Environment variable overrides
    if let Ok(v) = std::env::var("FORJA_PROVIDER") { config.active.provider = Some(v); }
    if let Ok(v) = std::env::var("FORJA_MODEL")    { config.active.model = Some(v); }
    if let Ok(v) = std::env::var("FORJA_SYSTEM_PROMPT") { config.agent.system_prompt = Some(v); }
    if let Ok(v) = std::env::var("FORJA_MAX_CONTEXT_TOKENS")
        && let Ok(parsed) = v.parse::<usize>() {
            config.agent.max_context_tokens = Some(parsed);
        }
    if let Ok(v) = std::env::var("FORJA_MONTHLY_TOKEN_LIMIT")
        && let Ok(parsed) = v.parse::<usize>() {
            config.agent.monthly_token_limit = Some(parsed);
        }
    if let Ok(v) = std::env::var("FORJA_HEARTBEAT_INTERVAL_SECS")
        && let Ok(parsed) = v.parse::<u64>() {
            config.agent.heartbeat_interval_secs = Some(parsed);
        }
    if let Ok(v) = std::env::var("FORJA_BUDGET_MODE") {
        config.agent.budget_mode = Some(v);
    }

    // API key environment override for the current provider
    if let Ok(key) = std::env::var("FORJA_API_KEY")
        && let Some(p) = &config.active.provider {
            config.keys.set_for(p, key);
        }

    config
}

pub fn load_from_file() -> Option<ForjaConfig> {
    let path = config_path();
    let text = std::fs::read_to_string(&path).ok()?;
    toml::from_str(&text).ok()
}

// Save

pub fn save_config(config: &ForjaConfig) -> std::io::Result<()> {
    let path = config_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let text = toml::to_string_pretty(config)
        .unwrap_or_else(|_| String::new());
    std::fs::write(&path, text)
}

// Onboarding

/// Provider definitions: (key, company label). Model names are shown later.
const PROVIDERS: &[(&str, &str)] = &[
    ("openai",       "OpenAI (API key)"),
    ("openai_oauth", "OpenAI Codex (OAuth subscription)"),
    ("anthropic",    "Anthropic (API key)"),
    ("gemini",       "Google Gemini (API key)"),
    ("gemini_oauth", "Google Gemini CLI (OAuth subscription)"),
    ("deepseek",     "DeepSeek"),
    ("glm",          "GLM (Zhipu)"),
    ("moonshot",     "Moonshot (Kimi)"),
    ("xai",          "xAI (Grok)"),
    ("ollama",       "Ollama (local, no API key required)"),
];

/// Model list by provider: (model_id, label)
pub fn models_for(provider: &str) -> Vec<(&'static str, &'static str)> {
    MODEL_TABLE
        .iter()
        .filter(|e| e.provider == provider)
        .map(|e| (e.model_id, e.label))
        .collect()
}

fn mask_key(key: &str) -> String {
    if key.len() <= 8 {
        return "****".to_string();
    }
    format!("{}...{}", &key[..4], &key[key.len()-4..])
}

/// ForjaConfig setup wizard using dialoguer arrow UI.
///
/// Flow:
///   1. Provider registration loop (Select) -> API key input (Input) -> register
///   2. "Done" -> choose default model -> save_config() once
pub fn run_setup() -> ForjaConfig {
    use dialoguer::{Input, Select, theme::ColorfulTheme};

    let mut config = load_from_file().unwrap_or_default();
    let theme = ColorfulTheme::default();

    println!("\n⚒️  Forja Provider Setup\n");

    // 1. Provider registration loop
    loop {
        // Build menu items: "✅ Moonshot" / "  OpenAI" + "Done"
        let active_prov = config.active.provider.as_deref().unwrap_or("");
        let mut items: Vec<String> = PROVIDERS.iter().map(|(key, label)| {
            let has = *key == "ollama" || config.keys.get_for(key).is_some();
            let check = if has { "✅" } else { "  " };
            let star  = if *key == active_prov { " ★default" } else { "" };
            format!("[{}] {}{}", check, label, star)
        }).collect();
        items.push("── Save and continue ──".to_string());

        let sel = Select::with_theme(&theme)
            .with_prompt("Select provider (↑↓, Enter)")
            .items(&items)
            .default(0)
            .interact_opt()
            .unwrap_or(None);

        let sel = match sel {
            None => break,
            Some(i) if i == PROVIDERS.len() => break,
            Some(i) => i,
        };

        let (pkey, plabel) = PROVIDERS[sel];

        // Select authentication method (skip for Ollama)
        if pkey == "ollama" {
            println!("  ✅ {} configured (no API key required)", plabel);
        } else if pkey == "openai_oauth" || pkey == "gemini_oauth" {
            // OAuth-only path: open browser login immediately
            let login_provider = match pkey {
                "openai_oauth" => "openai",
                "gemini_oauth" => "gemini",
                _ => pkey,
            };
            println!("  🌐 Starting OAuth login for {}...", plabel);
            tokio::task::block_in_place(|| {
                tokio::runtime::Handle::current().block_on(crate::oauth::run_login(login_provider))
            });
            println!("  ✅ OAuth login completed for {}", plabel);
        } else {
            let auth_methods = vec![
                "Enter API key".to_string(),
                "OAuth login (browser)".to_string(),
            ];
            let auth_sel = Select::with_theme(&theme)
                .with_prompt(format!("Authentication method for {}", plabel))
                .items(&auth_methods)
                .default(0)
                .interact()
                .unwrap();

            if auth_sel == 0 {
                // API key input
                let existing = config.keys.get_for(pkey);
                let hint = if let Some(ref k) = existing {
                    format!("Current: {} — press Enter to keep it", mask_key(k))
                } else {
                    format!("Enter the API key for {}", plabel)
                };

                let key_in: String = Input::with_theme(&theme)
                    .with_prompt(hint)
                    .allow_empty(true)
                    .interact_text()
                    .unwrap();

                if !key_in.is_empty() {
                    config.keys.set_for(pkey, key_in);
                    println!("  ✅ Saved key for {}", plabel);
                } else if existing.is_some() {
                    println!("  → Keeping existing key for {}", plabel);
                } else {
                    println!("  ⚠️  No key entered — you can add it later with `forja setup`");
                }
            } else {
                // OAuth login
                println!("  🌐 Starting OAuth login for {}...", plabel);
                tokio::task::block_in_place(|| {
                    tokio::runtime::Handle::current().block_on(crate::oauth::run_login(pkey))
                });
                println!("  ✅ OAuth login completed for {}", plabel);
            }
        }
    }

    // 2. Choose the default model
    let auth_data = crate::oauth::AuthData::load();
    let registered_models: Vec<(&str, &str, &str)> = PROVIDERS.iter()
        .filter(|(k, _)| {
            *k == "ollama"
            || config.keys.get_for(k).is_some()
            || match *k {
                "openai" | "openai_oauth" => auth_data.openai.is_some(),
                "gemini" | "gemini_oauth" => auth_data.gemini.is_some(),
                "anthropic" => auth_data.anthropic.is_some(),
                _ => false,
            }
        })
        .flat_map(|(k, _)| models_for(k).into_iter().map(|(id, label)| (*k, id, label)).collect::<Vec<_>>())
        .collect();

    if registered_models.is_empty() {
        println!("\n⚠️  No providers are configured. Saving without a default model.");
    } else {
        let model_items: Vec<String> = registered_models.iter().map(|(prov, id, label)| {
            format!("[{}] {} — {}", prov, label, id)
        }).collect();

        println!();
        let sel = Select::with_theme(&theme)
            .with_prompt("Select default model (↑↓, Enter)")
            .items(&model_items)
            .default(0)
            .interact_opt()
            .unwrap_or(None);

        if let Some(i) = sel {
            let (prov, model_id, label) = registered_models[i];
            config.active.provider = Some(prov.to_string());
            config.active.model    = Some(model_id.to_string());
            println!("  ★ Default model: {} — {}", label, model_id);
        }
    }

    println!("\n🤖 Assistant Settings\n");
    let name: String = Input::with_theme(&theme)
        .with_prompt("Assistant name (default: Forja)")
        .default("Forja".to_string())
        .interact_text()
        .unwrap();
    config.assistant_name = Some(name);

    let title: String = Input::with_theme(&theme)
        .with_prompt("User title (default: User)")
        .default("User".to_string())
        .interact_text()
        .unwrap();
    config.user_title = Some(title);

    // 3. Save once
    if let Err(e) = save_config(&config) {
        eprintln!("\n⚠️  Save failed: {}", e);
    } else {
        println!("\n💾 Saved: {}", config_path().display());
    }
    println!("✅ Forja will start with provider: {}\n",
        config.active.provider.as_deref().unwrap_or("unset"));
    config
}

// Backward-compatible alias
pub fn run_onboarding() -> ForjaConfig {
    run_setup()
}

// LlmConfig conversion

pub fn llm_config_from(cfg: &ForjaConfig) -> Result<LlmConfig, String> {
    let provider = cfg.active.provider.as_deref().unwrap_or("moonshot");
    let mut api_key = cfg.keys.get_for(provider).unwrap_or_default();

    if api_key.is_empty() && provider != "ollama" {
        let auth = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(
                crate::oauth::AuthData::refresh_token_if_needed(provider)
            )
        });
        
        // Handle OAuth specific data (like project_id for Gemini)
        if matches!(provider, "gemini_oauth" | "gemini_flash" | "gemini")
            && let Some(gemini_token) = &auth.gemini
            && let Some(proj) = &gemini_token.project_id {
                unsafe {
                    std::env::set_var("FORJA_GEMINI_PROJECT", proj);
                }
        }
        
        let oauth_key = match provider {
            "openai" | "openai_mini" | "openai_oauth" => auth.openai.map(|t| t.access_token),
            "gemini" | "gemini_flash" | "gemini_oauth" => auth.gemini.map(|t| t.access_token),
            "anthropic" | "anthropic_sonnet" => auth.anthropic.map(|t| t.access_token),
            _ => None,
        };
        
        if let Some(token) = oauth_key {
            api_key = token;
        } else {
            return Err(format!("The API key for provider '{}' is not configured.", provider));
        }
    }

    let mut lc = match provider {
        "openai"      => presets::openai(&api_key),
        "openai_mini" => presets::openai_mini(&api_key),
        "openai_oauth" => presets::openai_oauth(&api_key),
        "anthropic"   => presets::anthropic(&api_key),
        "anthropic_sonnet" => presets::anthropic_sonnet(&api_key),
        "gemini"      => presets::gemini(&api_key),
        "gemini_flash"=> presets::gemini_flash(&api_key),
        "gemini_oauth" => presets::gemini_oauth(&api_key),
        "deepseek"    => presets::deepseek(&api_key),
        "deepseek_reasoner" => presets::deepseek_reasoner(&api_key),
        "glm"         => presets::glm(&api_key),
        "glm_lite"    => presets::glm_lite(&api_key),
        "moonshot"    => presets::moonshot(&api_key),
        "xai"         => presets::xai(&api_key),
        "xai_mini"    => presets::xai_mini(&api_key),
        "ollama"      => presets::ollama(cfg.active.model.as_deref().unwrap_or("qwen3.5:9b")),
        other         => return Err(format!("Unknown provider: {}", other)),
    };

    if let Some(model) = &cfg.active.model
        && provider != "ollama" {
            lc.model = model.clone();
        }

    Ok(lc)
}

pub fn provider_info(cfg: &ForjaConfig) -> String {
    let provider = cfg.active.provider.as_deref().unwrap_or("?");
    let model    = cfg.active.model.as_deref().unwrap_or("preset default");
    format!("[Provider: {} | Model: {}]", provider, model)
}


