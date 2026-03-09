use forja_llm::LlmConfig;
use crate::config::{ForjaConfig, llm_config_from};

// ─── 모델 엔트리 ──────────────────────────────────────────────────────────────

pub struct ModelEntry {
    pub provider: &'static str,
    pub model_id: &'static str,
    pub label:    &'static str,
    pub aliases:  &'static [&'static str],
}

/// 전체 등록 모델 테이블 (최신 모델 ID 기준)
pub static MODEL_TABLE: &[ModelEntry] = &[
    ModelEntry { provider: "openai",    model_id: "gpt-5.4",                  label: "GPT-5.4 (플래그십)",           aliases: &["smart", "gpt5"] },
    ModelEntry { provider: "openai",    model_id: "gpt-5.4-mini",             label: "GPT-5.4 Mini (경량)",           aliases: &["mini"] },
    ModelEntry { provider: "openai_oauth", model_id: "gpt-5-codex",   label: "GPT-5 Codex (구독)",          aliases: &["codex5"] },
    ModelEntry { provider: "openai_oauth", model_id: "o3-pro",        label: "o3-Pro (구독)",               aliases: &["o3pro"] },
    ModelEntry { provider: "anthropic", model_id: "claude-opus-4-6",          label: "Claude Opus 4.6 (플래그십)",    aliases: &["opus"] },
    ModelEntry { provider: "anthropic", model_id: "claude-sonnet-4-6",        label: "Claude Sonnet 4.6 (경량)",      aliases: &["sonnet"] },
    ModelEntry { provider: "gemini",    model_id: "gemini-3.1-pro-preview",   label: "Gemini 3.1 Pro (유료)",         aliases: &["gemini"] },
    ModelEntry { provider: "gemini",    model_id: "gemini-3.1-flash",         label: "Gemini 3.1 Flash (무료)",       aliases: &["flash31"] },
    ModelEntry { provider: "gemini",    model_id: "gemini-2.5-pro",           label: "Gemini 2.5 Pro (무료)",         aliases: &["gemini25"] },
    ModelEntry { provider: "gemini",    model_id: "gemini-2.5-flash",         label: "Gemini 2.5 Flash (무료, 추천)",  aliases: &["flash", "flash25"] },
    ModelEntry { provider: "gemini_oauth", model_id: "gemini-2.5-pro",        label: "Gemini 2.5 Pro (CLI 구독)",   aliases: &["gempro"] },
    ModelEntry { provider: "gemini_oauth", model_id: "gemini-2.5-flash",      label: "Gemini 2.5 Flash (CLI 구독)", aliases: &["gemflash"] },
    ModelEntry { provider: "deepseek",  model_id: "deepseek-chat",            label: "DeepSeek V3 (기본)",            aliases: &["ds"] },
    ModelEntry { provider: "deepseek",  model_id: "deepseek-reasoner",        label: "DeepSeek R1 (추론)",            aliases: &["r1"] },
    ModelEntry { provider: "glm",       model_id: "glm-5",                    label: "GLM-5 (플래그십)",              aliases: &["glm"] },
    ModelEntry { provider: "glm",       model_id: "glm-4.5v",                 label: "GLM-4.5V (경량)",               aliases: &["glm-lite"] },
    ModelEntry { provider: "moonshot",  model_id: "kimi-k2.5",                label: "Kimi K2.5",                     aliases: &["kimi", "fast"] },
    ModelEntry { provider: "xai",       model_id: "grok-3",                   label: "Grok-3 (플래그십)",             aliases: &["grok"] },
    ModelEntry { provider: "xai",       model_id: "grok-3-mini",              label: "Grok-3 Mini (경량)",            aliases: &["grok-mini"] },
    ModelEntry { provider: "ollama",    model_id: "qwen3.5:9b",               label: "Ollama Qwen3.5 9B (로컬)",      aliases: &["local", "ollama"] },
    ModelEntry { provider: "ollama",    model_id: "llama3:8b",                label: "Ollama Llama3 8B (로컬)",       aliases: &["llama"] },
    ModelEntry { provider: "ollama",    model_id: "mistral:7b",               label: "Ollama Mistral 7B (로컬)",      aliases: &["mistral"] },
];

// ─── ProviderRegistry ─────────────────────────────────────────────────────────

pub struct ProviderRegistry {
    active_idx: usize,
}

impl ProviderRegistry {
    /// config에서 활성 모델 인덱스를 찾아 초기화
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

    /// 현재 활성 엔트리
    pub fn active(&self) -> &'static ModelEntry {
        &MODEL_TABLE[self.active_idx]
    }

    /// `/models` 출력: config에 등록된 프로바이더의 모델만 표시
    pub fn list_for_config(&self, cfg: &ForjaConfig) -> String {
        let mut s = String::from("📋 사용 가능한 모델 (등록된 프로바이더):\n");
        let mut display_idx = 1usize;
        for (i, e) in MODEL_TABLE.iter().enumerate() {
            let has_key = e.provider == "ollama"
                || cfg.keys.get_for(e.provider).is_some();
            if !has_key { continue; }
            let cur = if i == self.active_idx { " ◀ 현재" } else { "" };
            s.push_str(&format!(
                "  {:2}. [{}] {} — {}{}\\n",
                display_idx, e.provider, e.label, e.model_id, cur
            ));
            display_idx += 1;
        }
        s.push_str("\n→ `/model <번호>` 또는 `/model <이름/별칭>`으로 전환");
        s
    }

    /// `/models` 전체 목록 (등록 여부 무관)
    #[allow(dead_code)]
    pub fn list_display(&self) -> String {
        let mut s = String::from("📋 전체 모델 목록:\n");
        for (i, e) in MODEL_TABLE.iter().enumerate() {
            let cur = if i == self.active_idx { " ◀ 현재" } else { "" };
            s.push_str(&format!(
                "  {:2}. [{}] {} — {}{}\\n",
                i + 1, e.provider, e.label, e.model_id, cur
            ));
        }
        s.push_str("\n→ `/model <번호>` 또는 `/model <이름/별칭>`으로 전환");
        s
    }

    /// `/model <input>` → 인덱스 검색 (번호 | model_id | alias | 부분문자열)
    pub fn resolve(&self, input: &str) -> Option<usize> {
        let input = input.trim().to_lowercase();

        #[allow(clippy::collapsible_if)]
        if let Ok(n) = input.parse::<usize>() {
            if n >= 1 && n <= MODEL_TABLE.len() {
                return Some(n - 1);
            }
        }

        if let Some(idx) = MODEL_TABLE.iter().position(|e| e.model_id == input) {
            return Some(idx);
        }

        if let Some(idx) = MODEL_TABLE.iter().position(|e| e.aliases.contains(&input.as_str())) {
            return Some(idx);
        }

        MODEL_TABLE.iter().position(|e| e.model_id.contains(input.as_str()))
    }

    /// 스위칭 실행, 새 LlmConfig 반환
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


