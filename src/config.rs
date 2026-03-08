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

/// 프로바이더 정의: (key, 회사명)  — 모델명은 Step 3에서 별도 표시
const PROVIDERS: &[(&str, &str)] = &[
    ("openai",    "OpenAI"),
    ("anthropic", "Anthropic"),
    ("gemini",    "Google Gemini"),
    ("deepseek",  "DeepSeek"),
    ("glm",       "GLM / Zhipu"),
    ("moonshot",  "Moonshot Kimi"),
    ("ollama",    "Ollama (로컈, API 키 불필요)"),
];

/// 프로바이더별 모델 목록: (model_id, label)
pub fn models_for(provider: &str) -> Vec<(&'static str, &'static str)> {
    match provider {
        "openai"    => vec![
            ("gpt-5.4",       "GPT-5.4 (플래그십)"),
            ("gpt-5.4-mini",  "GPT-5.4 Mini (경량)"),
            ("gpt-5.3-codex", "GPT-5.3 Codex (코딩)"),
        ],
        "anthropic" => vec![
            ("claude-opus-4-6", "Claude Opus 4.6 (플래그십)"),
            ("claude-sonnet-4", "Claude Sonnet 4 (경량)"),
        ],
        "gemini"    => vec![
            ("gemini-3.1-pro",   "Gemini 3.1 Pro (플래그십)"),
            ("gemini-2.5-flash", "Gemini 2.5 Flash (경량)"),
        ],
        "deepseek"  => vec![
            ("deepseek-chat",     "DeepSeek V3 (기본)"),
            ("deepseek-reasoner", "DeepSeek R1 (추론)"),
        ],
        "glm"       => vec![
            ("glm-5",    "GLM-5 (플래그십)"),
            ("glm-4.5v", "GLM-4.5V (경량)"),
        ],
        "moonshot"  => vec![
            ("kimi-k2.5", "Kimi K2.5 (기본)"),
        ],
        "ollama"    => vec![
            ("qwen3.5:9b", "Qwen3.5 9B (기본)"),
            ("llama3:8b",  "Llama3 8B"),
            ("mistral:7b", "Mistral 7B"),
        ],
        _ => vec![],
    }
}

fn mask_key(key: &str) -> String {
    if key.len() <= 8 {
        return "****".to_string();
    }
    format!("{}...{}", &key[..4], &key[key.len()-4..])
}

fn prompt_line(label: &str) -> String {
    let stdin = io::stdin();
    let mut out = io::stdout();
    print!("{}", label);
    out.flush().ok();
    let mut line = String::new();
    stdin.lock().read_line(&mut line).ok();
    line.trim().to_string()
}

/// 3단계 설정 위저드 (forja setup 또는 최초 실행 시 호출)
pub fn run_setup() -> ForjaConfig {
    // 기존 config가 있으면 불러와서 키를 보존
    let mut config = load_from_file().unwrap_or_default();

    println!("\n⚒️  Forja 설정 위저드\n");

    // ── Step 1: 프로바이더 선택 ──────────────────────────
    println!("\n【Step 1/3】 기본 프로바이더를 선택하세요:\n");
    for (i, (key, label)) in PROVIDERS.iter().enumerate() {
        let current = if config.active.provider.as_deref() == Some(key) { " ←현재" } else { "" };
        println!("  {}. {}{}", i + 1, label, current);
    }

    let provider_key = loop {
        let input = prompt_line("\n번호 입력 > ");
        if input.is_empty() {
            if let Some(p) = &config.active.provider {
                let p = p.clone();
                println!("  → 기존 유지: {}", p);
                break p;
            }
        }
        if let Ok(n) = input.parse::<usize>() {
            if n >= 1 && n <= PROVIDERS.len() {
                break PROVIDERS[n - 1].0.to_string();
            }
        }
        println!("  ⚠️  1~{} 사이의 숫자를 입력하세요.", PROVIDERS.len());
    };

    config.active.provider = Some(provider_key.clone());

    // ── Step 2: 인증 (Ollama는 스킵) ──────────────────────────────
    if provider_key != "ollama" {
        let existing = config.keys.get_for(&provider_key);

        println!("\n【Step 2/3】 {} 인증 방식을 선택하세요:\n", provider_key);
        println!("  1. API 키 직접 입력");
        println!("  2. OAuth 로그인 (아직 미구현 — API 키로 대체)");

        let _auth = prompt_line("\n번호 입력 (Enter = 1) > ");
        println!("  → API 키 방식으로 진행합니다.\n");

        let hint = if let Some(ref k) = existing {
            format!(" [현재: {} | Enter로 유지]", mask_key(k))
        } else {
            String::new()
        };
        println!("  {} API 키를 입력하세요:{}", provider_key, hint);
        let key_input = prompt_line("  키 > ");

        if !key_input.is_empty() {
            config.keys.set_for(&provider_key, key_input);
            println!("  ✅ 키 저장됨");
        } else if existing.is_some() {
            println!("  → 기존 키 유지");
        } else {
            println!("  ⚠️  키 없이 계속합니다. 나중에 `forja setup`으로 설정하세요.");
        }
    } else {
        println!("\n【Step 2/3】 Ollama는 인증이 필요하지 않습니다. ✅ (스킵)");
    }

    // ── Step 3: 모델 선택 ────────────────────────────────
    let models = models_for(&provider_key);
    println!("\n【Step 3/3】 사용할 모델을 선택하세요:\n");
    for (i, (id, label)) in models.iter().enumerate() {
        let current = if config.active.model.as_deref() == Some(id) { " ←현재" } else { "" };
        println!("  {}. {} ({}){}", i + 1, label, id, current);
    }

    let model_id = loop {
        let input = prompt_line("\n번호 입력 (Enter = 기본값) > ");
        if input.is_empty() {
            // 기본값 = 첫 번째 모델
            let default = models.first().map(|(id, _)| *id).unwrap_or("");
            println!("  → 기본값 선택: {}", default);
            break default.to_string();
        }
        if let Ok(n) = input.parse::<usize>() {
            if n >= 1 && n <= models.len() {
                break models[n - 1].0.to_string();
            }
        }
        println!("  ⚠️  1~{} 사이의 숫자를 입력하세요.", models.len());
    };

    config.active.model = Some(model_id);

    // ── 저장 ─────────────────────────────────────────────
    if let Err(e) = save_config(&config) {
        eprintln!("⚠️  저장 실패: {}", e);
    } else {
        println!("\n💾 설정을 {} 에 저장했습니다.", config_path().display());
    }
    println!("✅ 완료! Forja가 {} 프로바이더로 시작됩니다.\n",
        config.active.provider.as_deref().unwrap_or("?"));
    config
}

// 하위 호환 alias
pub fn run_onboarding() -> ForjaConfig {
    run_setup()
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
        "ollama"      => presets::ollama(cfg.active.model.as_deref().unwrap_or("qwen3.5:9b")),
        other         => return Err(format!("알 수 없는 프로바이더: {}", other)),
    };

    if let Some(model) = &cfg.active.model {
        if provider != "ollama" {
            lc.model = model.clone();
        }
    }

    Ok(lc)
}

pub fn provider_info(cfg: &ForjaConfig) -> String {
    let provider = cfg.active.provider.as_deref().unwrap_or("?");
    let model    = cfg.active.model.as_deref().unwrap_or("preset default");
    format!("[Provider: {} | Model: {}]", provider, model)
}


