use crate::config::LlmConfig;

/// OpenAI GPT-5.2 (latest flagship, 2025.12+)
pub fn openai(api_key: &str) -> LlmConfig {
    LlmConfig::new("https://api.openai.com/v1", "gpt-5.4", api_key)
}

/// OpenAI GPT-5 mini (lightweight / lower cost)
pub fn openai_mini(api_key: &str) -> LlmConfig {
    LlmConfig::new("https://api.openai.com/v1", "gpt-5.4-mini", api_key)
}

/// Anthropic Claude Opus 4.6 (latest, 2026.02.05+)
pub fn anthropic(api_key: &str) -> LlmConfig {
    LlmConfig::new(
        "https://api.anthropic.com/v1",
        "claude-opus-4-6",
        api_key,
    )
    .with_header("x-api-key", api_key)
    .with_header("anthropic-version", "2023-06-01")
}

/// Anthropic Claude Sonnet 4.6 (optimized for faster responses, 2026.02.17+)
pub fn anthropic_sonnet(api_key: &str) -> LlmConfig {
    LlmConfig::new(
        "https://api.anthropic.com/v1",
        "claude-sonnet-4-6",
        api_key,
    )
    .with_header("x-api-key", api_key)
    .with_header("anthropic-version", "2023-06-01")
}

/// Google Gemini 3.1 Pro (latest, 2026.02.19+)
pub fn gemini(api_key: &str) -> LlmConfig {
    LlmConfig::new(
        "https://generativelanguage.googleapis.com/v1beta",
        "gemini-3.1-pro-preview",
        api_key,
    )
}

/// Google Gemini 3 Flash (lightweight / lower cost)
pub fn gemini_flash(api_key: &str) -> LlmConfig {
    LlmConfig::new(
        "https://generativelanguage.googleapis.com/v1beta",
        "gemini-3-flash-preview",
        api_key,
    )
}

/// Groq OpenAI-compatible endpoint
pub fn groq(model: &str, api_key: &str) -> LlmConfig {
    LlmConfig::new("https://api.groq.com/openai/v1", model, api_key)
}

/// OpenRouter OpenAI-compatible endpoint
pub fn openrouter(model: &str, api_key: &str) -> LlmConfig {
    LlmConfig::new("https://openrouter.ai/api/v1", model, api_key)
        .with_header("HTTP-Referer", "https://github.com/ai4wm/forja")
        .with_header("X-Title", "Forja")
}

/// DeepSeek V3.2 (current default API model, V4 expected soon)
pub fn deepseek(api_key: &str) -> LlmConfig {
    LlmConfig::new("https://api.deepseek.com/v1", "deepseek-chat", api_key)
}

/// DeepSeek R1 (reasoning-focused)
pub fn deepseek_reasoner(api_key: &str) -> LlmConfig {
    LlmConfig::new("https://api.deepseek.com/v1", "deepseek-reasoner", api_key)
}

/// GLM-5 (Zhipu latest flagship, 2026.02.11+)
pub fn glm(api_key: &str) -> LlmConfig {
    LlmConfig::new("https://open.bigmodel.cn/api/paas/v4", "glm-5", api_key)
}

/// GLM-4.5V (lightweight, vision-enabled, lower cost)
pub fn glm_lite(api_key: &str) -> LlmConfig {
    LlmConfig::new("https://open.bigmodel.cn/api/paas/v4", "glm-4.5v", api_key)
}

/// Moonshot Kimi K2.5 (latest, 2026.01.27+)
pub fn moonshot(api_key: &str) -> LlmConfig {
    LlmConfig::new("https://api.moonshot.cn/v1", "kimi-k2.5", api_key)
}

/// xAI Grok-3 (flagship)
pub fn xai(api_key: &str) -> LlmConfig {
    LlmConfig::new("https://api.x.ai/v1", "grok-3", api_key)
}

/// xAI Grok-3 Mini (lightweight)
pub fn xai_mini(api_key: &str) -> LlmConfig {
    LlmConfig::new("https://api.x.ai/v1", "grok-3-mini", api_key)
}

/// OAuth only: OpenAI (Codex subscription)
pub fn openai_oauth(token: &str) -> LlmConfig {
    LlmConfig::new("https://chatgpt.com/backend-api", "gpt-5.4", token)
        .with_responses_api()
}

/// OAuth only: Gemini (CLI subscription, internal endpoint)
pub fn gemini_oauth(token: &str) -> LlmConfig {
    LlmConfig::new("https://cloudcode-pa.googleapis.com", "gemini-3-flash-preview", token)
        .with_gemini_native_api()
        .with_header("user-agent", "GeminiCLI/v22.12.0 (windows; x86_64)")
}

/// Local Ollama
pub fn ollama(model: &str) -> LlmConfig {
    LlmConfig::new("http://localhost:11434/v1", model, "ollama")
}
