use forja_llm::{presets, LlmConfig};
use serde::{Deserialize, Serialize};
use std::io::{self, BufRead, Write};
use std::path::PathBuf;

// ─── 구조체 ──────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize, Serialize, Default, Clone)]
pub struct ForjaConfig {
    #[serde(default)]
    pub active: ActiveSection,
    #[serde(default)]
    pub keys: KeysSection,
    #[serde(default)]
    pub agent: AgentSection,
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
}

impl KeysSection {
    pub fn get_for(&self, provider: &str) -> Option<String> {
        match provider {
            "openai" | "openai_mini" => self.openai.clone(),
            "anthropic" | "anthropic_sonnet" => self.anthropic.clone(),
            "gemini" | "gemini_flash" => self.gemini.clone(),
            "deepseek" | "deepseek_reasoner" => self.deepseek.clone(),
            "glm" | "glm_lite" => self.glm.clone(),
            "moonshot" => self.moonshot.clone(),
            _ => None,
        }
    }

    pub fn set_for(&mut self, provider: &str, key: String) {
        match provider {
            "openai" | "openai_mini" => self.openai = Some(key),
            "anthropic" | "anthropic_sonnet" => self.anthropic = Some(key),
            "gemini" | "gemini_flash" => self.gemini = Some(key),
            "deepseek" | "deepseek_reasoner" => self.deepseek = Some(key),
            "glm" | "glm_lite" => self.glm = Some(key),
            "moonshot" => self.moonshot = Some(key),
            _ => {}
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Default, Clone)]
pub struct AgentSection {
    pub system_prompt: Option<String>,
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

// ─── 경로 헬퍼 ───────────────────────────────────────────────────────────────

pub fn config_path() -> PathBuf {
    let base = dirs_next::home_dir().unwrap_or_else(|| PathBuf::from("."));
    base.join(".forja").join("config.toml")
}

// ─── 로드 ────────────────────────────────────────────────────────────────────

pub fn load_config() -> ForjaConfig {
    let mut config = load_from_file().unwrap_or_default();

    // 환경변수 오버라이드
    if let Ok(v) = std::env::var("FORJA_PROVIDER") { config.active.provider = Some(v); }
    if let Ok(v) = std::env::var("FORJA_MODEL")    { config.active.model = Some(v); }
    if let Ok(v) = std::env::var("FORJA_SYSTEM_PROMPT") { config.agent.system_prompt = Some(v); }

    // API 키 환경변수 오버라이드 (현재 프로바이더 용)
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

// ─── 저장 ────────────────────────────────────────────────────────────────────

pub fn save_config(config: &ForjaConfig) -> std::io::Result<()> {
    let path = config_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let text = toml::to_string_pretty(config)
        .unwrap_or_else(|_| String::new());
    std::fs::write(&path, text)
}

// ─── 온보딩 ──────────────────────────────────────────────────────────────────

const PROVIDERS: &[(&str, &str)] = &[
    ("openai",     "OpenAI (gpt-5.2)"),
    ("openai_mini","OpenAI Mini (gpt-5-mini)"),
    ("anthropic",  "Anthropic (claude-opus-4-6)"),
    ("gemini",     "Google Gemini (gemini-3.1-pro-preview)"),
    ("deepseek",   "DeepSeek (deepseek-chat)"),
    ("glm",        "GLM / Zhipu (glm-5)"),
    ("moonshot",   "Moonshot Kimi (kimi-k2.5)"),
    ("ollama",     "Ollama (로컬, API 키 불필요)"),
];

pub fn run_onboarding() -> ForjaConfig {
    let stdin  = io::stdin();
    let mut out = io::stdout();
    let mut config = ForjaConfig::default();

    println!("\n🔧 Forja 설정을 시작합니다.\n");
    println!("각 프로바이더의 API 키를 입력하세요. 없으면 Enter로 스킵 가능합니다.");

    for &(key, label) in PROVIDERS {
        if key == "ollama" || key.contains("_mini") { continue; } // 특수 케이스 제외
        
        print!("  - {} API 키 > ", label);
        out.flush().ok();
        let mut input = String::new();
        stdin.lock().read_line(&mut input).ok();
        let trimmed = input.trim().to_string();
        if !trimmed.is_empty() {
            config.keys.set_for(key, trimmed);
        }
    }

    println!("\n기본 프로바이더를 선택하세요:");
    for (i, (_, label)) in PROVIDERS.iter().enumerate() {
        println!("  {}. {}", i + 1, label);
    }

    let provider_key = loop {
        print!("\n번호 입력 > ");
        out.flush().ok();
        let mut line = String::new();
        stdin.lock().read_line(&mut line).ok();
        if let Ok(n) = line.trim().parse::<usize>()
            && n >= 1 && n <= PROVIDERS.len() {
                break PROVIDERS[n - 1].0;
            }
        println!("  ⚠️  1~{} 사이의 숫자를 입력하세요.", PROVIDERS.len());
    };

    config.active.provider = Some(provider_key.to_string());

    // 저장
    if let Err(e) = save_config(&config) {
        eprintln!("⚠️  저장 실패: {}", e);
    } else {
        println!("\n💾 설정을 {} 에 저장했습니다.", config_path().display());
    }
    
    println!("✅ 완료! Forja가 {} 프로바이더로 시작됩니다.\n", provider_key);
    config
}

// ─── LlmConfig 변환 ──────────────────────────────────────────────────────────

pub fn llm_config_from(cfg: &ForjaConfig) -> Result<LlmConfig, String> {
    let provider = cfg.active.provider.as_deref().unwrap_or("moonshot");
    let api_key = cfg.keys.get_for(provider).unwrap_or_default();

    if api_key.is_empty() && provider != "ollama" {
        return Err(format!("'{}' 프로바이더의 API 키가 설정되지 않았습니다.", provider));
    }

    let mut lc = match provider {
        "openai"      => presets::openai(&api_key),
        "openai_mini" => presets::openai_mini(&api_key),
        "anthropic"   => presets::anthropic(&api_key),
        "anthropic_sonnet" => presets::anthropic_sonnet(&api_key),
        "gemini"      => presets::gemini(&api_key),
        "gemini_flash"=> presets::gemini_flash(&api_key),
        "deepseek"    => presets::deepseek(&api_key),
        "deepseek_reasoner" => presets::deepseek_reasoner(&api_key),
        "glm"         => presets::glm(&api_key),
        "glm_lite"    => presets::glm_lite(&api_key),
        "moonshot"    => presets::moonshot(&api_key),
        "ollama"      => presets::ollama(cfg.active.model.as_deref().unwrap_or("qwen2.5:14b")),
        other         => return Err(format!("알 수 없는 프로바이더: {}", other)),
    };

    if let Some(model) = &cfg.active.model {
        lc.model = model.clone();
    }

    Ok(lc)
}

pub fn provider_info(cfg: &ForjaConfig) -> String {
    let provider = cfg.active.provider.as_deref().unwrap_or("?");
    let model    = cfg.active.model.as_deref().unwrap_or("preset default");
    format!("[Provider: {} | Model: {}]", provider, model)
}
