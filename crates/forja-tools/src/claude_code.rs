use async_trait::async_trait;
use forja_core::{
    error::{ForjaError, Result},
    traits::Tool,
    types::ToolDefinition,
};
use serde_json::Value;
use std::process::Stdio;
use tokio::process::Command;
use tokio::time::{timeout, Duration};

const TIMEOUT_SECS: u64 = 600; // 10 minutes

pub struct ClaudeCodeTool;

impl ClaudeCodeTool {
    pub fn new() -> Self {
        Self
    }

    /// Check if 'claude' command is available in the system path
    pub async fn is_installed() -> bool {
        Command::new("claude")
            .arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .await
            .is_ok()
    }
}

#[async_trait]
impl Tool for ClaudeCodeTool {
    fn name(&self) -> &str {
        "claude_code"
    }

    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: self.name().to_string(),
            description: "Executes Claude Code CLI for complex coding tasks. Highly capable for deep repository modifications. Takes up to 10 minutes.".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "prompt": {
                        "type": "string",
                        "description": "Instruction for Claude Code. e.g. 'Refactor the auth module' or 'Find and fix memory leaks in parser.rs'"
                    }
                },
                "required": ["prompt"]
            }),
        }
    }

    async fn execute(&self, arguments: Value) -> Result<Value> {
        let prompt = arguments
            .get("prompt")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ForjaError::ToolError("Missing 'prompt' argument".to_string()))?;

        if !Self::is_installed().await {
            return Ok(serde_json::json!({
                "error": "Claude Code CLI is not installed or not available in the system PATH."
            }));
        }

        let mut child = Command::new("claude")
            .arg("--print")
            .arg("--json")
            .arg(prompt)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| ForjaError::ToolError(format!("Failed to execute claude: {}", e)))?;

        let output_future = child.wait_with_output();
        
        match timeout(Duration::from_secs(TIMEOUT_SECS), output_future).await {
            Ok(Ok(output)) => {
                let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                let stderr = String::from_utf8_lossy(&output.stderr).to_string();

                if output.status.success() {
                    Ok(serde_json::json!({
                        "result": stdout.trim(),
                        "stderr": if stderr.is_empty() { None } else { Some(stderr.trim()) }
                    }))
                } else {
                    Ok(serde_json::json!({
                        "error": format!("Claude Code execution failed (exit code {}).\nstdout: {}\nstderr: {}", 
                            output.status.code().unwrap_or(-1),
                            stdout.trim(),
                            stderr.trim()
                        )
                    }))
                }
            }
            Ok(Err(e)) => Err(ForjaError::ToolError(format!("Failed to read claude output: {}", e))),
            Err(_) => {
                Ok(serde_json::json!({
                    "error": format!("Claude Code execution timed out after {} seconds.", TIMEOUT_SECS)
                }))
            }
        }
    }
}
