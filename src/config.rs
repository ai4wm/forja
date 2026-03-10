use forja_llm::{presets, LlmConfig};
use crate::provider_registry::MODEL_TABLE;
use serde::{Deserialize, Serialize};
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
    ("openai",       "OpenAI (API 키)"),
    ("openai_oauth", "OpenAI Codex (구독 OAuth)"),
    ("anthropic",    "Anthropic (API 키)"),
    ("gemini",       "Google Gemini (API 키)"),
    ("gemini_oauth", "Google Gemini CLI (구독 OAuth)"),
    ("deepseek",     "DeepSeek"),
    ("glm",          "GLM (Zhipu)"),
    ("moonshot",     "Moonshot (Kimi)"),
    ("xai",          "xAI (Grok)"),
    ("ollama",       "Ollama (로컬, API 키 불필요)"),
];

/// 프로바이더별 모델 목록: (model_id, label)
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

/// ForjaConfig 설정 위저드 — dialoguer 화살표 UI 사용
///
/// 흐름:
///   ① 프로바이더 등록 루프 (Select) → API 키 입력 (Input) → 등록
///   ② "완료"  선택 → 기본 모델 Select → save_config() 한 번
pub fn run_setup() -> ForjaConfig {
    use dialoguer::{Input, Select, theme::ColorfulTheme};

    let mut config = load_from_file().unwrap_or_default();
    let theme = ColorfulTheme::default();

    println!("\n⚒️  Forja 프로바이더 설정\n");

    // ── ① 프로바이더 등록 루프 ────────────────────────────────────────────────
    loop {
        // 목록 아이템 생성: "✅ Moonshot Kimi" / "   OpenAI" + "── 완료 후 저장 ──"
        let active_prov = config.active.provider.as_deref().unwrap_or("");
        let mut items: Vec<String> = PROVIDERS.iter().map(|(key, label)| {
            let has = *key == "ollama" || config.keys.get_for(key).is_some();
            let check = if has { "✅" } else { "  " };
            let star  = if *key == active_prov { " ★기본" } else { "" };
            format!("[{}] {}{}", check, label, star)
        }).collect();
        items.push("── 완료 후 저장 ──".to_string());

        let sel = Select::with_theme(&theme)
            .with_prompt("프로바이더 선택 (↑↓, Enter)")
            .items(&items)
            .default(0)
            .interact_opt()
            .unwrap_or(None);

        let sel = match sel {
            None => break,        // ESC → 완료
            Some(i) if i == PROVIDERS.len() => break,  // "완료" 선택
            Some(i) => i,
        };

        let (pkey, plabel) = PROVIDERS[sel];

        // 인증 방식 선택 (Ollama 스킵)
        if pkey == "ollama" {
            println!("  ✅ {} 등록 완료 (API 키 불필요)", plabel);
        } else if pkey == "openai_oauth" || pkey == "gemini_oauth" {
            // OAuth 전용 — 바로 브라우저 로그인
            let login_provider = match pkey {
                "openai_oauth" => "openai",
                "gemini_oauth" => "gemini",
                _ => pkey,
            };
            println!("  🌐 {} OAuth 로그인 시작...", plabel);
            tokio::task::block_in_place(|| {
                tokio::runtime::Handle::current().block_on(crate::oauth::run_login(login_provider))
            });
            println!("  ✅ {} OAuth 로그인 완료", plabel);
        } else {
            let auth_methods = vec![
                format!("API 키 입력"),
                format!("OAuth 로그인 (브라우저)"),
            ];
            let auth_sel = Select::with_theme(&theme)
                .with_prompt(format!("{} 인증 방식", plabel))
                .items(&auth_methods)
                .default(0)
                .interact()
                .unwrap();

            if auth_sel == 0 {
                // API 키 입력 (기존 로직)
                let existing = config.keys.get_for(pkey);
                let hint = if let Some(ref k) = existing {
                    format!("현재: {} — 빈 Enter로 유지", mask_key(k))
                } else {
                    format!("{} API 키를 입력하세요", plabel)
                };

                let key_in: String = Input::with_theme(&theme)
                    .with_prompt(hint)
                    .allow_empty(true)
                    .interact_text()
                    .unwrap();

                if !key_in.is_empty() {
                    config.keys.set_for(pkey, key_in);
                    println!("  ✅ {} 키 저장됨", plabel);
                } else if existing.is_some() {
                    println!("  → {} 기존 키 유지", plabel);
                } else {
                    println!("  ⚠️  키 미입력 — 나중에 `forja setup`으로 추가 가능");
                }
            } else {
                // OAuth 로그인
                println!("  🌐 {} OAuth 로그인 시작...", plabel);
                tokio::task::block_in_place(|| {
                    tokio::runtime::Handle::current().block_on(crate::oauth::run_login(pkey))
                });
                println!("  ✅ {} OAuth 로그인 완료", plabel);
            }
        }
    }

    // ── ② 기본 모델 선택 ─────────────────────────────────────────────────────
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
        println!("\n⚠️  등록된 프로바이더가 없습니다. 기본 모델을 설정하지 않고 저장합니다.");
    } else {
        let model_items: Vec<String> = registered_models.iter().map(|(prov, id, label)| {
            format!("[{}] {} — {}", prov, label, id)
        }).collect();

        println!();
        let sel = Select::with_theme(&theme)
            .with_prompt("기본 모델 선택 (↑↓, Enter)")
            .items(&model_items)
            .default(0)
            .interact_opt()
            .unwrap_or(None);

        if let Some(i) = sel {
            let (prov, model_id, label) = registered_models[i];
            config.active.provider = Some(prov.to_string());
            config.active.model    = Some(model_id.to_string());
            println!("  ★ 기본 모델: {} — {}", label, model_id);
        }
    }

    // ── ③ 저장 (한 번만) ─────────────────────────────────────────────────────
    if let Err(e) = save_config(&config) {
        eprintln!("\n⚠️  저장 실패: {}", e);
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
    let mut api_key = cfg.keys.get_for(provider).unwrap_or_default();

    if api_key.is_empty() && provider != "ollama" {
        let auth = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(
                crate::oauth::AuthData::refresh_token_if_needed(provider)
            )
        });
        
        // Handle OAuth specific data (like project_id for Gemini)
        if matches!(provider, "gemini_oauth" | "gemini_flash" | "gemini") {
            if let Some(gemini_token) = &auth.gemini {
                if let Some(proj) = &gemini_token.project_id {
                    unsafe {
                        std::env::set_var("FORJA_GEMINI_PROJECT", proj);
                    }
                }
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
            return Err(format!("'{}' 프로바이더의 API 키가 설정되지 않았습니다.", provider));
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
        other         => return Err(format!("알 수 없는 프로바이더: {}", other)),
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


