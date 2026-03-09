use std::collections::HashMap;

/// LLM 클라이언트 연동에 필요한 외부 주입 설정 정보.
/// API의 기본 주소, 인증 키, 대상 모델 및 추가 헤더 값들을 포함합니다.
#[derive(Debug, Clone)]
pub struct LlmConfig {
    pub base_url: String,
    pub api_key: String,
    pub model: String,
    pub max_tokens: u32,
    pub extra_headers: HashMap<String, String>,
    pub use_responses_api: bool,
    pub use_gemini_native_api: bool,
}

impl LlmConfig {
    /// 기본 Config 생성자
    /// 필수 파라미터(기본 URL, 모델명, API Key)를 요구하며
    /// max_tokens는 기본적으로 4096으로 설정합니다.
    pub fn new(base_url: &str, model: &str, api_key: &str) -> Self {
        Self {
            base_url: base_url.to_string(),
            api_key: api_key.to_string(),
            model: model.to_string(),
            max_tokens: 4096,
            extra_headers: HashMap::new(),
            use_responses_api: false,
            use_gemini_native_api: false,
        }
    }

    /// 주어진 환경변수 이름(`env_var`)에서 API Key를 읽어들여 Config를 생성합니다.
    /// 환경 변수가 존재하지 않으면 None을 반환합니다.
    pub fn from_env(base_url: &str, model: &str, env_var: &str) -> Option<Self> {
        std::env::var(env_var)
            .ok()
            .map(|key| Self::new(base_url, model, &key))
    }

    /// 최대 출력 토큰수(max_tokens)를 변경할 수 있는 빌더 메서드
    pub fn with_max_tokens(mut self, max_tokens: u32) -> Self {
        self.max_tokens = max_tokens;
        self
    }

    /// 인증 외에 x-api-key, version 등 추가적인 HTTP 헤더를 삽입할 수 있는 빌더 메서드
    pub fn with_header(mut self, key: &str, value: &str) -> Self {
        self.extra_headers
            .insert(key.to_string(), value.to_string());
        self
    }

    /// Responses API (/v1/responses) 사용 여부를 설정하는 빌더 메서드
    pub fn with_responses_api(mut self) -> Self {
        self.use_responses_api = true;
        self
    }

    /// Gemini Native API 사용 여부를 설정하는 빌더 메서드
    pub fn with_gemini_native_api(mut self) -> Self {
        self.use_gemini_native_api = true;
        self
    }
}
