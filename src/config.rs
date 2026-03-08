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
    pub xai: Option<String>,
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
            "xai" => self.xai.clone(),
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
            "xai" => self.xai = Some(key),
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
    ("xai",       "xAI (Grok)"),
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
            ("claude-opus-4-6",  "Claude Opus 4.6 (플래그십)"),
            ("claude-sonnet-4-6", "Claude Sonnet 4.6 (경량)"),
        ],
        "gemini"    => vec![
            ("gemini-3.1-pro-preview",  "Gemini 3.1 Pro (플래그십)"),
            ("gemini-3-flash-preview",  "Gemini 3 Flash (경량)"),
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
        "xai"       => vec![
            ("grok-3",      "Grok-3 (플래그십)"),
            ("grok-3-mini", "Grok-3 Mini (경량)"),
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

// 등록 프로바이더 목록 제천
fn print_provider_status(config: &ForjaConfig) {
    let active = config.active.provider.as_deref().unwrap_or("");
    println!("");
    for (key, label) in PROVIDERS {
        let has_key = *key == "ollama" || config.keys.get_for(key).is_some();
        let icon = if has_key { "✅" } else { "  " };
        let model_tag = if *key == active {
            format!(" — {} ★기본", config.active.model.as_deref().unwrap_or(""))
        } else {
            String::new()
        };
        println!("  [{}] {}{}", icon, label, model_tag);
    }
    println!("");
    println!("  a = 프로바이더 추가/수정  |  m = 기본 모델 변경  |  q = 저장 후 종료");
}

/// 메인 메뉴 루프 설정 위저드
/// - config.toml 없을 때도 사용 가능 (복수 프로바이더 등록 지원)
pub fn run_setup() -> ForjaConfig {
    let mut config = load_from_file().unwrap_or_default();

    let stdin = io::stdin();
    let mut stdin_lock = stdin.lock();
    let mut read = |label: &str| -> String {
        print!("{}", label);
        io::stdout().flush().ok();
        let mut line = String::new();
        stdin_lock.read_line(&mut line).ok();
        line.trim().to_string()
    };

    println!("\n⛒️  Forja 프로바이더 설정");

    loop {
        print_provider_status(&config);
        let cmd = read("명령 > ");

        match cmd.as_str() {
            "q" => break,

            "a" => {
                // 프로바이더 선택
                println!("\n추가/수정할 프로바이더:");
                for (i, (_, label)) in PROVIDERS.iter().enumerate() {
                    println!("  {}. {}", i + 1, label);
                }
                let n_str = read("번호 > ");
                let idx = match n_str.parse::<usize>() {
                    Ok(n) if n >= 1 && n <= PROVIDERS.len() => n - 1,
                    _ => { println!("⚠️  잘못된 번호"); continue; }
                };
                let (pkey, plabel) = PROVIDERS[idx];

                // API 키 (옵라마 스킵)
                if pkey != "ollama" {
                    let existing = config.keys.get_for(pkey);
                    let hint = if let Some(ref k) = existing {
                        format!(" [현재: {} | Enter로 유지]", mask_key(k))
                    } else { String::new() };
                    println!("\n{} API 키{}:", plabel, hint);
                    let key_in = read("  키 > ");
                    if !key_in.is_empty() {
                        config.keys.set_for(pkey, key_in);
                        println!("  ✅ 키 저장됨");
                    } else if existing.is_some() {
                        println!("  → 기존 키 유지");
                    } else {
                        println!("  ⚠️  키 미입력 — 나중에 `forja setup`으로 다시 추가하세요.");
                        // 키 미입력이어도 모델 선택은 계속
                    }
                }

                // 모델 선택
                let models = models_for(pkey);
                println!("\n모델 선택:");
                for (i, (id, label)) in models.iter().enumerate() {
                    println!("  {}. {} — {}", i + 1, label, id);
                }
                let m_str = read("번호 (Enter = 기본) > ");
                let model_id = m_str.parse::<usize>()
                    .ok()
                    .filter(|&n| n >= 1 && n <= models.len())
                    .map(|n| models[n - 1].0.to_string())
                    .unwrap_or_else(|| models[0].0.to_string());

                println!("  ✅ {} / {} 등록 완료", plabel, model_id);

                // 기본으로 설정?
                let set_def = read("기본 모델로 설정할까요? (y/N) > ");
                if set_def.eq_ignore_ascii_case("y") {
                    config.active.provider = Some(pkey.to_string());
                    config.active.model    = Some(model_id);
                    println!("  ★ 기본 모델 설정 완료");
                }
            }

            "m" => {
                // 키가 있는 프로바이더만
                let available: Vec<(&str, &str)> = PROVIDERS.iter()
                    .filter(|(k, _)| *k == "ollama" || config.keys.get_for(k).is_some())
                    .map(|(k, l)| (*k, *l))
                    .collect();
                if available.is_empty() {
                    println!("⚠️  등록된 프로바이더가 없습니다. 먼저 `a`로 추가하세요.");
                    continue;
                }
                println!("\n기본 모델 변경 — 프로바이더 선택:");
                for (i, (_, label)) in available.iter().enumerate() {
                    println!("  {}. {}", i + 1, label);
                }
                let n_str = read("번호 > ");
                let idx = match n_str.parse::<usize>() {
                    Ok(n) if n >= 1 && n <= available.len() => n - 1,
                    _ => { println!("⚠️  잘못된 번호"); continue; }
                };
                let (pkey, _) = available[idx];
                let models = models_for(pkey);
                println!("\n모델 선택:");
                for (i, (id, label)) in models.iter().enumerate() {
                    println!("  {}. {} — {}", i + 1, label, id);
                }
                let m_str = read("번호 (Enter = 기본) > ");
                let model_id = m_str.parse::<usize>()
                    .ok()
                    .filter(|&n| n >= 1 && n <= models.len())
                    .map(|n| models[n - 1].0.to_string())
                    .unwrap_or_else(|| models[0].0.to_string());
                config.active.provider = Some(pkey.to_string());
                config.active.model    = Some(model_id);
                println!("  ★ 기본 모델 변경 완료");
            }

            _ => {
                println!("  a, m, q 중 선택하세요.");
            }
        }
    }

    drop(stdin_lock);

    if let Err(e) = save_config(&config) {
        eprintln!("⚠️  저장 실패: {}", e);
    } else {
        println!("\n💾 저장 완료: {}", config_path().display());
    }
    println!("✅ Forja가 {} 프로바이더로 시작됩니다.\n",
        config.active.provider.as_deref().unwrap_or("미설정"));
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
        "xai"         => presets::xai(&api_key),
        "xai_mini"    => presets::xai_mini(&api_key),
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


