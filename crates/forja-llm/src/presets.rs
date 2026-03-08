use crate::config::LlmConfig;

/// OpenAI GPT-5.2 (최신 플래그십, 2025.12~)
pub fn openai(api_key: &str) -> LlmConfig {
    LlmConfig::new("https://api.openai.com/v1", "gpt-5.4", api_key)
}

/// OpenAI GPT-5 mini (경량/저비용)
pub fn openai_mini(api_key: &str) -> LlmConfig {
    LlmConfig::new("https://api.openai.com/v1", "gpt-5.4-mini", api_key)
}

/// Anthropic Claude Opus 4.6 (최신, 2026.02.05~)
pub fn anthropic(api_key: &str) -> LlmConfig {
    LlmConfig::new(
        "https://api.anthropic.com/v1",
        "claude-opus-4-6",
        api_key,
    )
    .with_header("x-api-key", api_key)
    .with_header("anthropic-version", "2023-06-01")
}

/// Anthropic Claude Sonnet 4.6 (빠른 응답용, 2026.02.17~)
pub fn anthropic_sonnet(api_key: &str) -> LlmConfig {
    LlmConfig::new(
        "https://api.anthropic.com/v1",
        "claude-sonnet-4-6",
        api_key,
    )
    .with_header("x-api-key", api_key)
    .with_header("anthropic-version", "2023-06-01")
}

/// Google Gemini 3.1 Pro (최신, 2026.02.19~)
pub fn gemini(api_key: &str) -> LlmConfig {
    LlmConfig::new(
        "https://generativelanguage.googleapis.com/v1beta",
        "gemini-3.1-pro-preview",
        api_key,
    )
}

/// Google Gemini 3 Flash (경량/저비용)
pub fn gemini_flash(api_key: &str) -> LlmConfig {
    LlmConfig::new(
        "https://generativelanguage.googleapis.com/v1beta",
        "gemini-3-flash-preview",
        api_key,
    )
}

/// DeepSeek V3.2 (현재 API 기본 모델, V4 출시 임박)
pub fn deepseek(api_key: &str) -> LlmConfig {
    LlmConfig::new("https://api.deepseek.com/v1", "deepseek-chat", api_key)
}

/// DeepSeek R1 (추론 특화)
pub fn deepseek_reasoner(api_key: &str) -> LlmConfig {
    LlmConfig::new("https://api.deepseek.com/v1", "deepseek-reasoner", api_key)
}

/// GLM-5 (Zhipu 최신 플래그십, 2026.02.11~)
pub fn glm(api_key: &str) -> LlmConfig {
    LlmConfig::new("https://open.bigmodel.cn/api/paas/v4", "glm-5", api_key)
}

/// GLM-4.5V (경량/비전 지원, 저비용)
pub fn glm_lite(api_key: &str) -> LlmConfig {
    LlmConfig::new("https://open.bigmodel.cn/api/paas/v4", "glm-4.5v", api_key)
}

/// Moonshot Kimi K2.5 (최신, 2026.01.27~)
pub fn moonshot(api_key: &str) -> LlmConfig {
    LlmConfig::new("https://api.moonshot.cn/v1", "kimi-k2.5", api_key)
}

/// xAI Grok-3 (플래그십)
pub fn xai(api_key: &str) -> LlmConfig {
    LlmConfig::new("https://api.x.ai/v1", "grok-3", api_key)
}

/// xAI Grok-3 Mini (경량)
pub fn xai_mini(api_key: &str) -> LlmConfig {
    LlmConfig::new("https://api.x.ai/v1", "grok-3-mini", api_key)
}

/// 로컬 Ollama
pub fn ollama(model: &str) -> LlmConfig {
    LlmConfig::new("http://localhost:11434/v1", model, "ollama")
}
