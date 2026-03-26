use async_trait::async_trait;
use forja_core::error::{ForjaError, Result};
use forja_core::traits::Tool;
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::Duration;
use tokio::process::Command;

use crate::confirm::ConfirmationHandler;

const DEFAULT_TIMEOUT_SECS: u64 = 30;
const DANGEROUS_PATTERNS: &[&str] = &[
    "rm -rf",
    "del /f",
    "format c:", "format d:", "format e:",
    "fdisk",
    "shutdown",
    "reboot",
    "remove-item -recurse -force",
    "stop-process",
    "reg delete",
    "mkfs",
    "dd if=",
];

/// 시스템 명령어를 로컬 셸에서 실행하는 도구.
pub struct ShellTool {
    confirmation_handler: Arc<dyn ConfirmationHandler>,
    timeout: Duration,
    unsafe_mode: bool,
}

impl ShellTool {
    pub fn new(handler: Arc<dyn ConfirmationHandler>) -> Self {
        let timeout = std::env::var("FORJA_SHELL_TIMEOUT")
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .filter(|value| *value > 0)
            .map(Duration::from_secs)
            .unwrap_or_else(|| Duration::from_secs(DEFAULT_TIMEOUT_SECS));

        Self::with_settings(handler, timeout, false)
    }

    pub fn with_settings(
        handler: Arc<dyn ConfirmationHandler>,
        timeout: Duration,
        unsafe_mode: bool,
    ) -> Self {
        Self {
            confirmation_handler: handler,
            timeout,
            unsafe_mode,
        }
    }

    pub fn is_dangerous_command(command: &str) -> bool {
        let normalized = command.trim().to_lowercase();

        DANGEROUS_PATTERNS
            .iter()
            .any(|pattern| normalized.contains(pattern))
    }

    pub fn shell_invocation(command: &str) -> (String, Vec<String>) {
        #[cfg(target_os = "windows")]
        {
            (
                "powershell".to_string(),
                vec![
                    "-NoProfile".to_string(),
                    "-Command".to_string(),
                    command.to_string(),
                ],
            )
        }

        #[cfg(not(target_os = "windows"))]
        {
            ("sh".to_string(), vec!["-c".to_string(), command.to_string()])
        }
    }

    async fn run_command(&self, command: &str) -> Result<Value> {
        let current_dir = std::env::current_dir().map_err(|error| {
            ForjaError::ToolError(format!("Failed to resolve current directory: {error}"))
        })?;
        let (program, args) = Self::shell_invocation(command);
        let mut process = Command::new(&program);
        process
            .args(&args)
            .current_dir(current_dir)
            .kill_on_drop(true);

        let output = match tokio::time::timeout(self.timeout, process.output()).await {
            Ok(output) => output.map_err(|error| {
                ForjaError::ToolError(format!("Failed to execute '{command}': {error}"))
            })?,
            Err(_) => {
                return Err(ForjaError::ToolError(format!(
                    "Shell command timeout after {}s: {command}",
                    self.timeout.as_secs()
                )));
            }
        };

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let combined_output = format!("{stdout}{stderr}");

        if !output.status.success() {
            return Ok(json!({
                "status": "error",
                "exit_code": output.status.code(),
                "output": combined_output,
            }));
        }

        Ok(json!({
            "status": "success",
            "exit_code": output.status.code(),
            "output": combined_output,
        }))
    }
}

#[async_trait]
impl Tool for ShellTool {
    fn name(&self) -> &str {
        "shell"
    }

    fn definition(&self) -> forja_core::types::ToolDefinition {
        forja_core::types::ToolDefinition {
            name: self.name().to_string(),
            description: "Execute a local OS shell command. Dangerous commands require explicit user confirmation unless unsafe mode is enabled.".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "command": {
                        "type": "string",
                        "description": "The OS command to execute."
                    }
                },
                "required": ["command"]
            }),
        }
    }

    async fn execute(&self, args: Value) -> Result<Value> {
        let command = args["command"].as_str().ok_or_else(|| {
            ForjaError::ToolError("Missing 'command' parameter for shell".to_string())
        })?;
        let command = command.trim();

        if command.is_empty() {
            return Err(ForjaError::ToolError("Shell command is empty".to_string()));
        }

        if !self.unsafe_mode && Self::is_dangerous_command(command) {
            let warning = format!(
                "[경고] 이 명령어는 시스템에 영향을 줄 수 있습니다: {command}\n실행하시겠습니까? (y/n)"
            );
            if !self.confirmation_handler.confirm(command, true).await {
                return Ok(json!({
                    "status": "warning",
                    "output": warning,
                }));
            }
        } else if !self.unsafe_mode && !self.confirmation_handler.confirm(command, false).await {
            return Ok(json!({
                "status": "warning",
                "output": format!("[경고] 실행 전 확인이 필요합니다: {command}\n실행하시겠습니까? (y/n)"),
            }));
        }

        self.run_command(command).await
    }
}
