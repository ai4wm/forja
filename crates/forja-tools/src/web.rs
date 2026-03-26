use async_trait::async_trait;
use forja_core::error::{ForjaError, Result};
use forja_core::traits::Tool;
use serde_json::{json, Value};

/// Tool for fetching web page text with a simple GET request.
pub struct WebTool;

impl WebTool {
    pub fn new() -> Self {
        Self
    }
}

impl Default for WebTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for WebTool {
    fn name(&self) -> &str {
        "web_tool"
    }

    fn definition(&self) -> forja_core::types::ToolDefinition {
        forja_core::types::ToolDefinition {
            name: self.name().to_string(),
            description: "Fetch the content of a web page using a GET request.".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "url": {
                        "type": "string",
                        "description": "The URL of the web page to fetch."
                    }
                },
                "required": ["url"]
            }),
        }
    }

    async fn execute(&self, args: Value) -> Result<Value> {
        let url = args["url"].as_str().ok_or_else(|| {
            ForjaError::ToolError("Missing 'url' parameter for web_tool".into())
        })?;

        // Build a reqwest client with a 10-second timeout and issue a GET request.
        let client = reqwest::Client::builder()
            .connect_timeout(std::time::Duration::from_secs(10))
            .build()
            .map_err(|e| ForjaError::ToolError(format!("HTTP client error: {}", e)))?;

        let response = client.get(url).send().await.map_err(|e| {
            ForjaError::ToolError(format!("Failed to execute GET request to {}: {}", url, e))
        })?;

        let status = response.status();
        if !status.is_success() {
            return Err(ForjaError::ToolError(format!(
                "HTTP Error {} when accessing {}",
                status, url
            )));
        }

        let body = response.text().await.map_err(|e| {
            ForjaError::ToolError(format!("Failed to extract body text from {}: {}", url, e))
        })?;

        // Return raw body text without HTML cleanup and cap it at 50,000 chars
        // to avoid excessive context growth.
        let max_chars = 50_000;
        let truncated = if body.chars().count() > max_chars {
            let cut: String = body.chars().take(max_chars).collect();
            format!("{}...\n[Truncated: original {} chars]", cut, body.chars().count())
        } else {
            body
        };

        Ok(json!({ "content": truncated }))
    }
}
