use async_trait::async_trait;
use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
use forja_core::error::Result;
use forja_core::traits::Tool;
#[cfg(feature = "vision")]
use image::imageops::crop_imm;
#[cfg(feature = "vision")]
use image::{DynamicImage, ImageFormat, RgbaImage};
use serde_json::{json, Value};
#[cfg(feature = "vision")]
use std::io::Cursor;
use std::sync::{Arc, Mutex};
#[cfg(feature = "vision")]
use xcap::Monitor;
use reqwest::Client;

#[async_trait]
pub trait ScreenCaptureBackend: Send + Sync + 'static {
    async fn capture_full(&self) -> std::result::Result<Vec<u8>, String>;
    async fn capture_region(
        &self,
        x: i32,
        y: i32,
        width: u32,
        height: u32,
    ) -> std::result::Result<Vec<u8>, String>;
}

#[cfg(feature = "vision")]
pub struct XcapBackend;

#[cfg(feature = "vision")]
impl XcapBackend {
    pub fn new() -> Self {
        Self
    }

    fn capture_monitor_image(&self) -> std::result::Result<RgbaImage, String> {
        let monitor = Monitor::all()
            .map_err(|error| format!("Failed to enumerate monitors: {error}"))?
            .into_iter()
            .next()
            .ok_or_else(|| "No monitor available for capture".to_string())?;

        monitor
            .capture_image()
            .map_err(|error| format!("Failed to capture monitor image: {error}"))
    }
}

#[cfg(feature = "vision")]
impl Default for XcapBackend {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(feature = "vision")]
#[async_trait]
impl ScreenCaptureBackend for XcapBackend {
    async fn capture_full(&self) -> std::result::Result<Vec<u8>, String> {
        encode_png(self.capture_monitor_image()?)
    }

    async fn capture_region(
        &self,
        x: i32,
        y: i32,
        width: u32,
        height: u32,
    ) -> std::result::Result<Vec<u8>, String> {
        if x < 0 || y < 0 {
            return Err("Capture region coordinates must be non-negative".to_string());
        }

        let image = self.capture_monitor_image()?;
        let start_x = u32::try_from(x).map_err(|_| "Invalid x coordinate".to_string())?;
        let start_y = u32::try_from(y).map_err(|_| "Invalid y coordinate".to_string())?;
        let end_x = start_x
            .checked_add(width)
            .ok_or_else(|| "Capture region x overflow".to_string())?;
        let end_y = start_y
            .checked_add(height)
            .ok_or_else(|| "Capture region y overflow".to_string())?;

        if end_x > image.width() || end_y > image.height() {
            return Err("Capture region is out of screen bounds".to_string());
        }

        let cropped = crop_imm(&image, start_x, start_y, width, height).to_image();
        encode_png(cropped)
    }
}

pub struct MockCaptureBackend {
    calls: Mutex<Vec<String>>,
}

impl MockCaptureBackend {
    pub fn new() -> Self {
        Self {
            calls: Mutex::new(Vec::new()),
        }
    }

    pub fn calls_snapshot(&self) -> Vec<String> {
        match self.calls.lock() {
            Ok(calls) => calls.clone(),
            Err(_) => Vec::new(),
        }
    }

    fn push_call(&self, call: String) -> std::result::Result<(), String> {
        let mut calls = self
            .calls
            .lock()
            .map_err(|_| "Mock capture backend lock was poisoned".to_string())?;
        calls.push(call);
        Ok(())
    }

    fn transparent_png() -> std::result::Result<Vec<u8>, String> {
        #[cfg(feature = "vision")]
        {
            let image = RgbaImage::from_pixel(1, 1, image::Rgba([0, 0, 0, 0]));
            encode_png(image)
        }

        #[cfg(not(feature = "vision"))]
        {
            BASE64_STANDARD
                .decode("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADElEQVR4nGNgYGAAAAAEAAHIokmRAAAAAElFTkSuQmCC")
                .map_err(|error| format!("Failed to decode mock PNG: {error}"))
        }
    }
}

impl Default for MockCaptureBackend {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ScreenCaptureBackend for MockCaptureBackend {
    async fn capture_full(&self) -> std::result::Result<Vec<u8>, String> {
        self.push_call("capture_full".to_string())?;
        Self::transparent_png()
    }

    async fn capture_region(
        &self,
        x: i32,
        y: i32,
        width: u32,
        height: u32,
    ) -> std::result::Result<Vec<u8>, String> {
        self.push_call(format!("capture_region:{x},{y},{width},{height}"))?;
        Self::transparent_png()
    }
}

#[async_trait]
pub trait VisionAnalyzer: Send + Sync + 'static {
    async fn analyze_image(
        &self,
        image_base64: &str,
        prompt: &str,
    ) -> std::result::Result<String, String>;
}

pub struct MockVisionAnalyzer {
    calls: Mutex<Vec<String>>,
    response: String,
}

impl MockVisionAnalyzer {
    pub fn new() -> Self {
        Self {
            calls: Mutex::new(Vec::new()),
            response: "Mock vision response".to_string(),
        }
    }

    pub fn with_response(response: impl Into<String>) -> Self {
        Self {
            calls: Mutex::new(Vec::new()),
            response: response.into(),
        }
    }

    pub fn calls_snapshot(&self) -> Vec<String> {
        match self.calls.lock() {
            Ok(calls) => calls.clone(),
            Err(_) => Vec::new(),
        }
    }

    fn push_call(&self, call: String) -> std::result::Result<(), String> {
        let mut calls = self
            .calls
            .lock()
            .map_err(|_| "Mock vision analyzer lock was poisoned".to_string())?;
        calls.push(call);
        Ok(())
    }
}

impl Default for MockVisionAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl VisionAnalyzer for MockVisionAnalyzer {
    async fn analyze_image(
        &self,
        _image_base64: &str,
        prompt: &str,
    ) -> std::result::Result<String, String> {
        self.push_call(format!("analyze:{prompt}"))?;
        if self.response != "Mock vision response" {
            return Ok(self.response.clone());
        }
        if prompt.starts_with("Find the UI element matching: ") {
            return Ok("{\"x\": 100, \"y\": 200, \"width\": 50, \"height\": 30}".to_string());
        }

        Ok(self.response.clone())
    }
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
                "stream": true,}),
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
            // SSE stream response: collect text from response.output_text.delta events
            let mut collected_text = String::new();
            for line in body.lines() {
                if let Some(data) = line.strip_prefix("data: ")
                    && let Ok(ev) = serde_json::from_str::<Value>(data)
                    && ev["type"].as_str() == Some("response.output_text.delta")
                    && let Some(d) = ev["delta"].as_str()
                {
                    collected_text.push_str(d);
                }
            }
            if !collected_text.is_empty() {
                return Ok(collected_text);
            }
            return Err(format!("Vision SSE response contained no text. Raw: {}", &body[..body.len().min(300)]));
        }

        let json: Value = serde_json::from_str(&body)
            .map_err(|error| format!("Failed to parse vision response JSON: {error}. Raw: {body}"))?;

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

pub struct VisionTool {
    capture_backend: Arc<dyn ScreenCaptureBackend>,
    vision_analyzer: Arc<dyn VisionAnalyzer>,
}

impl VisionTool {
    pub fn with_backends(
        capture_backend: Arc<dyn ScreenCaptureBackend>,
        vision_analyzer: Arc<dyn VisionAnalyzer>,
        _unused: bool,
    ) -> Self {
        Self {
            capture_backend,
            vision_analyzer,
        }
    }
}

#[async_trait]
impl Tool for VisionTool {
    fn name(&self) -> &str {
        "vision"
    }

    fn definition(&self) -> forja_core::types::ToolDefinition {
        forja_core::types::ToolDefinition {
            name: self.name().to_string(),
            description: "Capture screenshots and analyze screen content using vision AI. Supports full/region capture, visual analysis, element finding, and OCR.".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "action": {
                        "type": "string",
                        "description": "Vision action: capture_screen, capture_region, analyze, analyze_region, find_element, or ocr."
                    }
                },
                "required": ["action"]
            }),
        }
    }

    async fn execute(&self, args: Value) -> Result<Value> {
        let action = args["action"]
            .as_str()
            .map(str::trim)
            .unwrap_or_default()
            .to_string();

        if action.is_empty() {
            return Ok(error_result("", "Missing 'action' field".to_string()));
        }

        let result = match action.as_str() {
            "capture_screen" => match self.capture_full_base64().await {
                Ok(data) => ok_result(&action, data),
                Err(data) => error_result(&action, data),
            },
            "capture_region" => {
                let (x, y, width, height) = match required_region(&args) {
                    Ok(region) => region,
                    Err(data) => return Ok(error_result(&action, data)),
                };
                match self.capture_region_base64(x, y, width, height).await {
                    Ok(data) => ok_result(&action, data),
                    Err(data) => error_result(&action, data),
                }
            }
            "analyze" => {
                let prompt = match required_string(&args, "prompt") {
                    Ok(prompt) => prompt,
                    Err(data) => return Ok(error_result(&action, data)),
                };
                match self.capture_analyze_full(prompt).await {
                    Ok(data) => ok_result(&action, data),
                    Err(data) => error_result(&action, data),
                }
            }
            "analyze_region" => {
                let (x, y, width, height) = match required_region(&args) {
                    Ok(region) => region,
                    Err(data) => return Ok(error_result(&action, data)),
                };
                let prompt = match required_string(&args, "prompt") {
                    Ok(prompt) => prompt,
                    Err(data) => return Ok(error_result(&action, data)),
                };
                match self
                    .capture_analyze_region(x, y, width, height, prompt)
                    .await
                {
                    Ok(data) => ok_result(&action, data),
                    Err(data) => error_result(&action, data),
                }
            }
            "find_element" => {
                let description = match required_string(&args, "description") {
                    Ok(description) => description,
                    Err(data) => return Ok(error_result(&action, data)),
                };
                let prompt = format!(
                    "Find the UI element matching: {description}. Return JSON: {{\"x\": number, \"y\": number, \"width\": number, \"height\": number}}. If not found, return NONE."
                );
                match self.capture_analyze_full(&prompt).await {
                    Ok(data) if data.trim().eq_ignore_ascii_case("NONE") => {
                        ok_result(&action, format!("Element not found: {description}"))
                    }
                    Ok(data) => ok_result(&action, data),
                    Err(data) => error_result(&action, data),
                }
            }
            "ocr" => {
                let (x, y, width, height) = match required_region(&args) {
                    Ok(region) => region,
                    Err(data) => return Ok(error_result(&action, data)),
                };
                let prompt = "Read all text visible in this image. Return the text exactly as shown.";
                match self
                    .capture_analyze_region(x, y, width, height, prompt)
                    .await
                {
                    Ok(data) => ok_result(&action, data),
                    Err(data) => error_result(&action, data),
                }
            }
            _ => error_result(&action, format!("Unsupported vision action: {action}")),
        };

        Ok(result)
    }
}

impl VisionTool {
    async fn capture_full_base64(&self) -> std::result::Result<String, String> {
        eprintln!("[Vision] Screen capture performed - may contain sensitive information");
        self.capture_backend
            .capture_full()
            .await
            .map(|bytes| BASE64_STANDARD.encode(bytes))
    }

    async fn capture_region_base64(
        &self,
        x: i32,
        y: i32,
        width: u32,
        height: u32,
    ) -> std::result::Result<String, String> {
        eprintln!("[Vision] Screen capture performed - may contain sensitive information");
        self.capture_backend
            .capture_region(x, y, width, height)
            .await
            .map(|bytes| BASE64_STANDARD.encode(bytes))
    }

    async fn capture_analyze_full(&self, prompt: &str) -> std::result::Result<String, String> {
        let image = self.capture_full_base64().await?;

        self.vision_analyzer.analyze_image(&image, prompt).await
    }

    async fn capture_analyze_region(
        &self,
        x: i32,
        y: i32,
        width: u32,
        height: u32,
        prompt: &str,
    ) -> std::result::Result<String, String> {
        let image = self.capture_region_base64(x, y, width, height).await?;

        self.vision_analyzer.analyze_image(&image, prompt).await
    }
}

#[cfg(feature = "vision")]
fn encode_png(image: RgbaImage) -> std::result::Result<Vec<u8>, String> {
    let mut buffer = Cursor::new(Vec::new());
    DynamicImage::ImageRgba8(image)
        .write_to(&mut buffer, ImageFormat::Png)
        .map_err(|error| format!("Failed to encode PNG: {error}"))?;

    Ok(buffer.into_inner())
}

fn required_string<'a>(args: &'a Value, field: &str) -> std::result::Result<&'a str, String> {
    args[field]
        .as_str()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("Missing required string field '{field}'"))
}

fn required_i32(args: &Value, field: &str) -> std::result::Result<i32, String> {
    let value = args[field]
        .as_i64()
        .ok_or_else(|| format!("Missing required integer field '{field}'"))?;

    i32::try_from(value).map_err(|_| format!("Field '{field}' is out of range"))
}

fn required_u32(args: &Value, field: &str) -> std::result::Result<u32, String> {
    let value = args[field]
        .as_u64()
        .ok_or_else(|| format!("Missing required integer field '{field}'"))?;

    u32::try_from(value).map_err(|_| format!("Field '{field}' is out of range"))
}

fn required_region(args: &Value) -> std::result::Result<(i32, i32, u32, u32), String> {
    Ok((
        required_i32(args, "x")?,
        required_i32(args, "y")?,
        required_u32(args, "width")?,
        required_u32(args, "height")?,
    ))
}

fn ok_result(action: &str, data: String) -> Value {
    json!({
        "status": "ok",
        "action": action,
        "data": data,
    })
}

fn error_result(action: &str, data: String) -> Value {
    json!({
        "status": "error",
        "action": action,
        "data": data,
    })
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










