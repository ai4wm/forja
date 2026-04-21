use std::collections::HashMap;
use std::path::PathBuf;

/// External configuration injected into the LLM client.
/// Includes base URL, auth key, target model, and extra header values.
#[derive(Debug, Clone)]
pub struct LlmConfig {
    pub base_url: String,
    pub api_key: String,
    pub model: String,
    pub max_tokens: u32,
    pub extra_headers: HashMap<String, String>,
    pub use_responses_api: bool,
    pub use_gemini_native_api: bool,
    pub local_model_path: Option<PathBuf>,
    pub manage_local_server: bool,
}

impl LlmConfig {
    /// Basic config constructor.
    /// Requires base URL, model name, and API key.
    /// max_tokens defaults to 4096.
    pub fn new(base_url: &str, model: &str, api_key: &str) -> Self {
        Self {
            base_url: base_url.to_string(),
            api_key: api_key.to_string(),
            model: model.to_string(),
            max_tokens: 4096,
            extra_headers: HashMap::new(),
            use_responses_api: false,
            use_gemini_native_api: false,
            local_model_path: None,
            manage_local_server: false,
        }
    }

    /// Creates a config by reading the API key from the given environment variable.
    /// Returns None when the environment variable is missing.
    pub fn from_env(base_url: &str, model: &str, env_var: &str) -> Option<Self> {
        std::env::var(env_var)
            .ok()
            .map(|key| Self::new(base_url, model, &key))
    }

    /// Builder method for changing the maximum output tokens.
    pub fn with_max_tokens(mut self, max_tokens: u32) -> Self {
        self.max_tokens = max_tokens;
        self
    }

    /// Builder method for inserting extra HTTP headers such as x-api-key or version.
    pub fn with_header(mut self, key: &str, value: &str) -> Self {
        self.extra_headers
            .insert(key.to_string(), value.to_string());
        self
    }

    /// Builder method for enabling Responses API (/v1/responses).
    pub fn with_responses_api(mut self) -> Self {
        self.use_responses_api = true;
        self
    }

    /// Builder method for enabling Gemini Native API mode.
    pub fn with_gemini_native_api(mut self) -> Self {
        self.use_gemini_native_api = true;
        self
    }

    pub fn with_local_model_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.local_model_path = Some(path.into());
        self
    }

    pub fn with_managed_local_server(mut self) -> Self {
        self.manage_local_server = true;
        self
    }
}
