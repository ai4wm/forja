use forja_llm::LlmConfig;
use crate::config::{ForjaConfig, llm_config_from};

// ─── 모델 엔트리 ──────────────────────────────────────────────────────────────

pub struct ModelEntry {
    pub provider: &'static str,
    pub model_id: &'static str,
    pub label: &'static str,
    pub aliases: &'static [&'static str],
}

/// 전체 등록 모델 테이블
pub static MODEL_TABLE: &[ModelEntry] = &[
    ModelEntry { provider: "openai",    model_id: "gpt-5.2",                  label: "GPT-5.2 (플래그십)",        aliases: &["smart", "gpt5"] },
    ModelEntry { provider: "openai",    model_id: "gpt-5-mini",               label: "GPT-5 Mini (경량)",          aliases: &["mini"] },
    ModelEntry { provider: "anthropic", model_id: "claude-opus-4-6",          label: "Claude Opus 4.6",            aliases: &["opus"] },
    ModelEntry { provider: "anthropic", model_id: "claude-sonnet-4-6",        label: "Claude Sonnet 4.6 (빠름)",   aliases: &["sonnet"] },
    ModelEntry { provider: "gemini",    model_id: "gemini-3.1-pro-preview",   label: "Gemini 3.1 Pro",             aliases: &["gemini"] },
    ModelEntry { provider: "gemini",    model_id: "gemini-3-flash-preview",   label: "Gemini 3 Flash (경량)",      aliases: &["flash"] },
    ModelEntry { provider: "deepseek",  model_id: "deepseek-chat",            label: "DeepSeek V3",                aliases: &["ds"] },
    ModelEntry { provider: "deepseek",  model_id: "deepseek-reasoner",        label: "DeepSeek R1 (추론)",         aliases: &["r1"] },
    ModelEntry { provider: "glm",       model_id: "glm-5",                    label: "GLM-5",                      aliases: &["glm"] },
    ModelEntry { provider: "moonshot",  model_id: "kimi-k2.5",                label: "Kimi K2.5",                  aliases: &["kimi", "fast"] },
    ModelEntry { provider: "ollama",    model_id: "qwen3.5:9b",               label: "Ollama qwen3.5:9b (로컬)",   aliases: &["local", "ollama"] },
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
            // 프로바이더만 일치해도 첫 번째
            MODEL_TABLE.iter().position(|e| e.provider == provider)
        }).unwrap_or(0);

        Self { active_idx: idx }
    }

    /// 현재 활성 엔트리
    pub fn active(&self) -> &'static ModelEntry {
        &MODEL_TABLE[self.active_idx]
    }

    /// `/models` 에 출력할 번호 목록 문자열
    pub fn list_display(&self) -> String {
        let mut s = String::from("📋 사용 가능한 모델:\n");
        for (i, e) in MODEL_TABLE.iter().enumerate() {
            let cur = if i == self.active_idx { " ◀ 현재" } else { "" };
            s.push_str(&format!(
                "  {:2}. [{}] {} — {}{}\n",
                i + 1, e.provider, e.label, e.model_id, cur
            ));
        }
        s.push_str("\n별칭: smart=GPT-5.2, mini=GPT-5-mini, opus=Claude, sonnet=Sonnet, gemini=Gemini, flash=Gemini Flash, ds=DeepSeek, r1=R1, glm=GLM-5, kimi/fast=Kimi, local/ollama=Ollama");
        s
    }

    /// `/model <input>` → 인덱스 검색
    /// input: 번호(1-based) | model_id 부분 문자열 | alias
    pub fn resolve(&self, input: &str) -> Option<usize> {
        let input = input.trim().to_lowercase();

        // 번호
        #[allow(clippy::collapsible_if)]
        if let Ok(n) = input.parse::<usize>() {
            if n >= 1 && n <= MODEL_TABLE.len() {
                return Some(n - 1);
            }
        }

        // 정확한 model_id
        if let Some(idx) = MODEL_TABLE.iter().position(|e| e.model_id == input) {
            return Some(idx);
        }

        // alias
        if let Some(idx) = MODEL_TABLE.iter().position(|e| e.aliases.contains(&input.as_str())) {
            return Some(idx);
        }

        // model_id 부분 포함
        MODEL_TABLE.iter().position(|e| e.model_id.contains(input.as_str()))
    }

    /// 스위칭 실행, 새 LlmConfig 반환
    pub fn switch_to(&mut self, idx: usize, cfg: &ForjaConfig) -> Result<LlmConfig, String> {
        let entry = &MODEL_TABLE[idx];
        // 임시 cfg로 변환
        let mut tmp = cfg.clone();
        tmp.active.provider = Some(entry.provider.to_string());
        tmp.active.model = Some(entry.model_id.to_string());
        let lc = llm_config_from(&tmp)?;
        self.active_idx = idx;
        Ok(lc)
    }
}
