use async_trait::async_trait;
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
use reqwest::Client;
use serde_json::{Value, json};

#[async_trait]
pub trait VisionAnalyzer: Send + Sync + 'static {
    async fn analyze_image(
        &self,
        image_base64: &str,
        prompt: &str,
    ) -> std::result::Result<String, String>;
}

pub struct GptVisionAnalyzer {
    api_base: String,
    auth_token: String,
    model: String,
    client: Client,
}

impl GptVisionAnalyzer {
    pub fn new(
        api_base: impl Into<String>,
        auth_token: impl Into<String>,
        model: impl Into<String>,
    ) -> Self {
        Self {
            api_base: api_base.into().trim_end_matches('/').to_string(),
            auth_token: auth_token.into(),
            model: model.into(),
            client: Client::new(),
        }
    }

    fn is_responses_api(&self) -> bool {
        let normalized = self.api_base.to_lowercase();
        normalized.contains("chatgpt.com") || normalized.contains("backend-api")
    }
}

impl Default for GptVisionAnalyzer {
    fn default() -> Self {
        Self::new("https://api.openai.com/v1", "", "gpt-5.4")
    }
}

#[async_trait]
impl VisionAnalyzer for GptVisionAnalyzer {
    async fn analyze_image(
        &self,
        image_base64: &str,
        prompt: &str,
    ) -> std::result::Result<String, String> {
        if self.auth_token.trim().is_empty() {
            return Err("Vision analyzer auth token is empty".to_string());
        }

        let media_type = detect_media_type(image_base64);
        let (endpoint, payload) = if self.is_responses_api() {
            (
                format!("{}/codex/responses", self.api_base),
                json!({
                    "model": self.model,
                    "input": [{
                        "role": "user",
                        "content": [
                            {"type": "input_text", "text": prompt},
                            {"type": "input_image", "image_url": format!("data:{media_type};base64,{image_base64}")}
                        ]
                    }],
                    "instructions": "You are a helpful vision assistant. Analyze the image and respond in the same language as the user prompt.",
                    "store": false,
                    "stream": true,
                }),
            )
        } else {
            (
                format!("{}/chat/completions", self.api_base),
                json!({
                    "model": self.model,
                    "messages": [{
                        "role": "user",
                        "content": [
                            {"type": "text", "text": prompt},
                            {"type": "image_url", "image_url": {
                                "url": format!("data:{media_type};base64,{image_base64}")
                            }}
                        ]
                    }],
                    "max_tokens": 4096
                }),
            )
        };

        let response = self
            .client
            .post(&endpoint)
            .bearer_auth(&self.auth_token)
            .header("Accept", "text/event-stream")
            .json(&payload)
            .send()
            .await
            .map_err(|error| format!("Vision request failed: {error}"))?;

        let status = response.status();
        let body = response
            .text()
            .await
            .map_err(|error| format!("Failed to read vision response body: {error}"))?;

        if !status.is_success() {
            return Err(format!("Vision HTTP {}: {}", status, body));
        }

        if self.is_responses_api() {
            let mut collected_text = String::new();
            for line in body.lines() {
                if let Some(data) = line.strip_prefix("data: ")
                    && let Ok(event) = serde_json::from_str::<Value>(data)
                    && event["type"].as_str() == Some("response.output_text.delta")
                    && let Some(delta) = event["delta"].as_str()
                {
                    collected_text.push_str(delta);
                }
            }
            if !collected_text.is_empty() {
                return Ok(collected_text);
            }
            return Err(format!(
                "Vision SSE response contained no text. Raw: {}",
                &body[..body.len().min(300)]
            ));
        }

        let json: Value = serde_json::from_str(&body).map_err(|error| {
            format!("Failed to parse vision response JSON: {error}. Raw: {body}")
        })?;

        if let Some(text) = json["choices"][0]["message"]["content"].as_str() {
            return Ok(text.to_string());
        }

        if let Some(parts) = json["choices"][0]["message"]["content"].as_array() {
            let text = parts
                .iter()
                .filter_map(|part| part.get("text").and_then(Value::as_str))
                .collect::<Vec<_>>()
                .join("\n");
            if !text.is_empty() {
                return Ok(text);
            }
        }

        Err("Vision response missing choices[0].message.content".to_string())
    }
}

fn detect_media_type(image_base64: &str) -> &'static str {
    let bytes = match BASE64_STANDARD.decode(image_base64) {
        Ok(bytes) => bytes,
        Err(_) => return "image/png",
    };

    if bytes.starts_with(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]) {
        return "image/png";
    }

    if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return "image/jpeg";
    }

    "image/png"
}
