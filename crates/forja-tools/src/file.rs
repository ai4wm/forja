use async_trait::async_trait;
use forja_core::error::{ForjaError, Result};
use forja_core::traits::Tool;
use serde_json::{json, Value};
use tokio::fs;

/// 파일시스템 읽기 및 덮어쓰기 기능을 제공하는 기본 FileTool
pub struct FileTool;

impl FileTool {
    pub fn new() -> Self {
        Self
    }
}

impl Default for FileTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for FileTool {
    fn name(&self) -> &str {
        "file_tool"
    }

    fn definition(&self) -> forja_core::types::ToolDefinition {
        forja_core::types::ToolDefinition {
            name: self.name().to_string(),
            description: "Read from or write to a file on the local system.".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "action": {
                        "type": "string",
                        "enum": ["read", "write"],
                        "description": "The action to perform (read or write)."
                    },
                    "path": {
                        "type": "string",
                        "description": "The absolute or relative path to the file."
                    },
                    "content": {
                        "type": "string",
                        "description": "The content to write (only required for action='write')."
                    }
                },
                "required": ["action", "path"]
            }),
        }
    }

    async fn execute(&self, args: Value) -> Result<Value> {
        let action = args["action"].as_str().ok_or_else(|| {
            ForjaError::ToolError("Missing 'action' parameter for file_tool".into())
        })?;

        match action {
            "read" => {
                let path = args["path"].as_str().ok_or_else(|| {
                    ForjaError::ToolError("Missing 'path' parameter for file_tool(read)".into())
                })?;

                let content = fs::read_to_string(path).await.map_err(|e| {
                    ForjaError::ToolError(format!("Failed to read file '{}': {}", path, e))
                })?;

                Ok(json!({ "content": content }))
            }
            "write" => {
                let path = args["path"].as_str().ok_or_else(|| {
                    ForjaError::ToolError("Missing 'path' parameter for file_tool(write)".into())
                })?;

                let content = args["content"].as_str().ok_or_else(|| {
                    ForjaError::ToolError("Missing 'content' parameter for file_tool(write)".into())
                })?;

                fs::write(path, content).await.map_err(|e| {
                    ForjaError::ToolError(format!("Failed to write to file '{}': {}", path, e))
                })?;

                Ok(json!({ "status": "success", "message": format!("File {} written successfully.", path) }))
            }
            _ => Err(ForjaError::ToolError(format!("Unsupported action '{}' for file_tool. Allowed: read, write", action))),
        }
    }
}
