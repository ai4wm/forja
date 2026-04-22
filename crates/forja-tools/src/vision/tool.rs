use super::analyzer::VisionAnalyzer;
use super::capture::ScreenCaptureBackend;
use async_trait::async_trait;
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
use forja_core::error::Result;
use forja_core::traits::Tool;
use serde_json::{Value, json};
use std::sync::Arc;

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
            "capture_screen" => simple_action_result(&action, self.capture_full_base64().await),
            "capture_region" => execute_capture_region(&action, &args, self).await,
            "analyze" => execute_analyze(&action, &args, self).await,
            "analyze_region" => execute_analyze_region(&action, &args, self).await,
            "find_element" => execute_find_element(&action, &args, self).await,
            "ocr" => execute_ocr(&action, &args, self).await,
            _ => error_result(&action, format!("Unsupported vision action: {action}")),
        };

        Ok(result)
    }
}

async fn execute_capture_region(action: &str, args: &Value, tool: &VisionTool) -> Value {
    let (x, y, width, height) = match required_region(args) {
        Ok(region) => region,
        Err(detail) => return error_result(action, detail),
    };
    simple_action_result(
        action,
        tool.capture_region_base64(x, y, width, height).await,
    )
}

async fn execute_analyze(action: &str, args: &Value, tool: &VisionTool) -> Value {
    let prompt = match required_string(args, "prompt") {
        Ok(prompt) => prompt,
        Err(detail) => return error_result(action, detail),
    };
    simple_action_result(action, tool.capture_analyze_full(prompt).await)
}

async fn execute_analyze_region(action: &str, args: &Value, tool: &VisionTool) -> Value {
    let (x, y, width, height) = match required_region(args) {
        Ok(region) => region,
        Err(detail) => return error_result(action, detail),
    };
    let prompt = match required_string(args, "prompt") {
        Ok(prompt) => prompt,
        Err(detail) => return error_result(action, detail),
    };
    simple_action_result(
        action,
        tool.capture_analyze_region(x, y, width, height, prompt)
            .await,
    )
}

async fn execute_find_element(action: &str, args: &Value, tool: &VisionTool) -> Value {
    let description = match required_string(args, "description") {
        Ok(description) => description,
        Err(detail) => return error_result(action, detail),
    };
    let prompt = format!(
        "Find the UI element matching: {description}. Return JSON: {{\"x\": number, \"y\": number, \"width\": number, \"height\": number}}. If not found, return NONE."
    );
    match tool.capture_analyze_full(&prompt).await {
        Ok(data) if data.trim().eq_ignore_ascii_case("NONE") => {
            ok_result(action, format!("Element not found: {description}"))
        }
        Ok(data) => ok_result(action, data),
        Err(detail) => error_result(action, detail),
    }
}

async fn execute_ocr(action: &str, args: &Value, tool: &VisionTool) -> Value {
    let (x, y, width, height) = match required_region(args) {
        Ok(region) => region,
        Err(detail) => return error_result(action, detail),
    };
    simple_action_result(
        action,
        tool.capture_analyze_region(
            x,
            y,
            width,
            height,
            "Read all text visible in this image. Return the text exactly as shown.",
        )
        .await,
    )
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

fn simple_action_result(action: &str, result: std::result::Result<String, String>) -> Value {
    match result {
        Ok(data) => ok_result(action, data),
        Err(detail) => error_result(action, detail),
    }
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
