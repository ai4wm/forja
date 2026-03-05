# forja-llm Implementation Plan

## Overview
이 문서는 `forja-llm` 크레이트의 설계를 정의합니다.
기존 특정 프로바이더(Anthropic 등) 전용 클라이언트를 만드는 방식의 한계를 극복하고, **OpenAI Chat Completions API 호환 포맷 하나로 범용 LLM 클라이언트를 구축**합니다. 
최근 거의 모든 상위 모델(DeepSeek, GLM, Ollama) 및 파운데이션 모델(Anthropic)이 이 호환 엔드포인트를 제공하므로, 클라이언트 1개만으로 확장성을 극대화합니다.

### 🌟 핵심 설계 (Universal Client + Presets)
- **단일 클라이언트**: OpenAI 호환 포맷으로 통신하는 `LlmClient` 한 개만 존재.
- **의존성 주입**: API URL, 헤더 매핑, 모델명은 하드코딩 없이 `LlmConfig`로 외부 주입.
- **프리셋 패턴 (3줄 확장)**: 새로운 LLM 추가는 `presets.rs`에 팩토리 함수(URL, 모델명만 지정)를 3줄 추가하는 것으로 완료됩니다.

---

## 📁 File Structure
```text
crates/forja-llm/
├── Cargo.toml
├── plan.md            <- 현재 파일
└── src/
    ├── lib.rs         <- 모듈 선언 + re-export
    ├── config.rs      <- LlmConfig 구조체 (URL, 헤더, 모델명 동적 설정)
    ├── models.rs      <- Request/Response 구조체 (OpenAI Chat Completions 형식)
    ├── client.rs      <- LlmClient (LlmProvider Trait 구현체)
    └── presets.rs     <- anthropic(), openai(), glm(), deepseek(), ollama()
```

---

## 📦 Dependencies (`Cargo.toml`)

```toml
[package]
name = "forja-llm"
version = "0.1.0"
edition = "2024"

[dependencies]
forja-core = { path = "../forja-core" }
tokio-stream = "0.1"
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
async-trait = "0.1"

# HTTP 및 스트리밍 처리 (호환성이 보장된 0.12/0.6 매칭)
reqwest = { version = "0.12", features = ["json", "stream"] }
reqwest-eventsource = "0.6"
async-stream = "0.3"
```

---

## 📝 Code Snippets

### 1. `src/config.rs` — 설정 외부 주입
하드코딩을 원천 차단하고 구조체로 API 키, URL, 모델, 추가 헤더를 캡슐화합니다.

```rust
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct LlmConfig {
    pub base_url: String,
    pub api_key: String,
    pub model: String,
    pub max_tokens: u32,
    pub extra_headers: HashMap<String, String>,
}

impl LlmConfig {
    /// 기본 Config 생성자 (필수 파라미터만 강제)
    pub fn new(base_url: &str, model: &str, api_key: &str) -> Self {
        Self {
            base_url: base_url.to_string(),
            api_key: api_key.to_string(),
            model: model.to_string(),
            max_tokens: 4096,
            extra_headers: HashMap::new(),
        }
    }

    /// 환경변수에서 키를 읽어오는 헬퍼
    pub fn from_env(base_url: &str, model: &str, env_var: &str) -> Option<Self> {
        std::env::var(env_var).ok().map(|key| Self::new(base_url, model, &key))
    }

    pub fn with_max_tokens(mut self, max_tokens: u32) -> Self {
        self.max_tokens = max_tokens;
        self
    }

    pub fn with_header(mut self, key: &str, value: &str) -> Self {
        self.extra_headers.insert(key.to_string(), value.to_string());
        self
    }
}
```

---

### 2. `src/presets.rs` — 팩토리 패턴
새로운 모델이 출시되면 파일 1개만 건드려 3줄 안에 추가할 수 있는 프리셋 모음.

```rust
use crate::config::LlmConfig;

/// OpenAI 기본 (GPT-4)
pub fn openai(api_key: &str) -> LlmConfig {
    LlmConfig::new("https://api.openai.com/v1", "gpt-4-turbo", api_key)
}

/// Anthropic 호환 서버 (OpenAI API 포맷을 지원하는 래퍼나 클라우드)
pub fn anthropic(api_key: &str) -> LlmConfig {
    LlmConfig::new("https://api.anthropic.com/v1", "claude-sonnet-4-20250514", api_key)
        .with_header("x-api-key", api_key)
        .with_header("anthropic-version", "2023-06-01")
}

/// DeepSeek
pub fn deepseek(api_key: &str) -> LlmConfig {
    LlmConfig::new("https://api.deepseek.com/v1", "deepseek-chat", api_key)
}

/// GLM (Zhipuai)
pub fn glm(api_key: &str) -> LlmConfig {
    LlmConfig::new("https://open.bigmodel.cn/api/paas/v4", "glm-4", api_key)
}

/// 로컬 Ollama (키 불필요)
pub fn ollama(model: &str) -> LlmConfig {
    LlmConfig::new("http://localhost:11434/v1", model, "local-ollama-key")
}
```

---

### 3. `src/models.rs` — 범용 OpenAI 호환 스펙
대부분의 엔드포인트가 채택한 표준 OpenAI 포맷으로 직렬화/역직렬화합니다.

```rust
use serde::{Deserialize, Serialize};

#[derive(Serialize, Debug)]
pub struct ChatCompletionRequest<'a> {
    pub model: &'a str,
    pub messages: Vec<ChatCompletionMessage>,
    pub max_tokens: u32,
    pub stream: bool,
}

#[derive(Serialize, Debug)]
pub struct ChatCompletionMessage {
    pub role: String,
    pub content: String,
}

#[derive(Deserialize, Debug)]
pub struct ChatCompletionResponse {
    pub id: Option<String>,
    pub choices: Vec<Choice>,
}

#[derive(Deserialize, Debug)]
pub struct Choice {
    pub message: Option<ChatMessage>,
    pub delta: Option<ChatDelta>, // 스트리밍 용
}

#[derive(Deserialize, Debug)]
pub struct ChatMessage {
    pub role: Option<String>,
    pub content: Option<String>,
}

#[derive(Deserialize, Debug)]
pub struct ChatDelta {
    pub content: Option<String>,
}
```

---

### 4. `src/client.rs` — 유니버설 LlmProvider 구현체

```rust
use crate::config::LlmConfig;
use crate::models::{ChatCompletionMessage, ChatCompletionRequest, ChatCompletionResponse};
use async_trait::async_trait;
use forja_core::error::{ForjaError, Result};
use forja_core::traits::LlmProvider;
use forja_core::types::{Content, Message, Role};
use reqwest::header::{HeaderMap, HeaderValue};
use reqwest_eventsource::{Event, EventSource};
use std::pin::Pin;
use tokio_stream::Stream;

pub struct LlmClient {
    client: reqwest::Client,
    config: LlmConfig,
}

impl LlmClient {
    pub fn new(config: LlmConfig) -> Result<Self> {
        let mut headers = HeaderMap::new();
        
        // Authorization Bearer는 기본 적용 (OpenAI API 호환의 암묵적 룰)
        let auth_val = format!("Bearer {}", config.api_key);
        if let Ok(val) = HeaderValue::from_str(&auth_val) {
            headers.insert("Authorization", val);
        }

        // Custom extra headers (Anthropic의 x-api-key 등 대응)
        for (k, v) in &config.extra_headers {
            if let Ok(hdr_name) = reqwest::header::HeaderName::from_bytes(k.as_bytes()) {
                if let Ok(hdr_val) = HeaderValue::from_str(v) {
                    headers.insert(hdr_name, hdr_val);
                }
            }
        }

        let client = reqwest::Client::builder()
            .default_headers(headers)
            .build()
            .map_err(|e| ForjaError::Internal(e.to_string()))?;

        Ok(Self { client, config })
    }

    fn prepare_payload(&self, messages: &[Message], stream: bool) -> ChatCompletionRequest {
        let chat_msgs: Vec<ChatCompletionMessage> = messages
            .iter()
            .map(|m| {
                let role = match m.role {
                    Role::System => "system",
                    Role::User => "user",
                    Role::Assistant => "assistant",
                    Role::Tool => "tool",
                };

                let content_str = match &m.content {
                    Content::Text { text } => text.clone(),
                    // ToolCall 시뮬레이션을 구현할 Phase 2에서 JSON 직렬화 로직 분기 추가
                    _ => "(Unsupported content type)".to_string(),
                };

                ChatCompletionMessage {
                    role: role.to_string(),
                    content: content_str,
                }
            })
            .collect();

        ChatCompletionRequest {
            model: &self.config.model,
            messages: chat_msgs,
            max_tokens: self.config.max_tokens,
            stream,
        }
    }
}

#[async_trait]
impl LlmProvider for LlmClient {
    async fn chat(&self, messages: &[Message]) -> Result<String> {
        let payload = self.prepare_payload(messages, false);
        let endpoint = format!("{}/chat/completions", self.config.base_url);

        let response = self
            .client
            .post(&endpoint)
            .json(&payload)
            .send()
            .await
            .map_err(|e| ForjaError::LlmError(e.to_string()))?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(ForjaError::LlmError(format!("Http {}: {}", status, text)));
        }

        let parsed: ChatCompletionResponse = response
            .json()
            .await
            .map_err(|e| ForjaError::LlmError(format!("JSON parsing error: {}", e)))?;

        let content = parsed
            .choices
            .into_iter()
            .next()
            .and_then(|c| c.message)
            .and_then(|m| m.content)
            .unwrap_or_default();

        Ok(content)
    }

    async fn stream(
        &self,
        messages: &[Message],
    ) -> Result<Pin<Box<dyn Stream<Item = Result<String>> + Send>>> {
        let payload = self.prepare_payload(messages, true);
        let endpoint = format!("{}/chat/completions", self.config.base_url);

        let request_builder = self.client.post(&endpoint).json(&payload);

        let mut event_source = reqwest_eventsource::EventSource::new(request_builder)
            .map_err(|e| ForjaError::LlmError(e.to_string()))?;

        let stream = async_stream::stream! {
            while let Some(event_res) = tokio_stream::StreamExt::next(&mut event_source).await {
                match event_res {
                    Ok(Event::Message(msg)) => {
                        if msg.data == "[DONE]" {
                            break;
                        }
                        if let Ok(parsed) = serde_json::from_str::<ChatCompletionResponse>(&msg.data) {
                            if let Some(choice) = parsed.choices.into_iter().next() {
                                if let Some(delta) = choice.delta {
                                    if let Some(text) = delta.content {
                                        yield Ok(text);
                                    }
                                }
                            }
                        }
                    }
                    Ok(Event::Open) => continue,
                    Err(e) => {
                        yield Err(ForjaError::LlmError(format!("Stream error: {}", e)));
                        break;
                    }
                }
            }
        };

        Ok(Box::pin(stream))
    }
}
```

### 5. `src/lib.rs` — 모듈 묶기

```rust
pub mod config;
pub mod models;
pub mod client;
pub mod presets;

pub use config::LlmConfig;
pub use client::LlmClient;
```
