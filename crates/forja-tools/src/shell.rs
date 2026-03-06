use async_trait::async_trait;
use forja_core::error::{ForjaError, Result};
use forja_core::traits::Tool;
use serde_json::{json, Value};
use std::sync::Arc;
use tokio::process::Command;

use crate::confirm::ConfirmationHandler;

const AUTO_APPROVE_PREFIXES: &[&str] = &[
    "ls", "dir", "pwd", "cat", "echo", "whoami", 
    "cargo build", "cargo test", "cargo check", 
    "git status", "git log", "git diff",
    "curl", "date",
];

/// 시스템 명령어를 안전하게 실행하기 위한 도구.
/// 임의 명령어 제어를 위한 화이트리스트 정책이나
/// `ConfirmationHandler` 인터페이스를 통한 사용자 승인 확인 절차를 갖춥니다.
pub struct ShellTool {
    confirmation_handler: Arc<dyn ConfirmationHandler>,
}

impl ShellTool {
    pub fn new(handler: Arc<dyn ConfirmationHandler>) -> Self {
        Self {
            confirmation_handler: handler,
        }
    }

    /// Windows 대응을 위한 간단한 셸 커맨드 파싱 헬퍼
    fn build_command(&self, cmd_line: &str) -> Command {
        // Windows/Unix 분기.
        // 현재는 pwsh (PowerShell Core) 혹은 cmd.exe 를 래핑해 실행
        if cfg!(target_os = "windows") {
            let mut cmd = Command::new("cmd");
            cmd.args(["/C", cmd_line]);
            cmd
        } else {
            let mut cmd = Command::new("sh");
            cmd.args(["-c", cmd_line]);
            cmd
        }
    }
}

#[async_trait]
impl Tool for ShellTool {
    fn name(&self) -> &str {
        "shell_tool"
    }

    fn definition(&self) -> forja_core::types::ToolDefinition {
        forja_core::types::ToolDefinition {
            name: self.name().to_string(),
            description: "Execute a shell command on the local system. Requires user approval for each execution.".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "command": {
                        "type": "string",
                        "description": "The shell command to execute."
                    }
                },
                "required": ["command"]
            }),
        }
    }

    async fn execute(&self, args: Value) -> Result<Value> {
        let cmd_str = args["command"].as_str().ok_or_else(|| {
            ForjaError::ToolError("Missing 'command' parameter for shell_tool".into())
        })?;

        let is_auto_approved = AUTO_APPROVE_PREFIXES.iter().any(|&prefix| {
            cmd_str == prefix || cmd_str.starts_with(&format!("{} ", prefix))
        });

        // 1. 실행 전, 자동 승인 대상인지 확인하거나 ConfirmationHandler를 통한 사용자 승인
        let is_approved = if is_auto_approved {
            println!("✅ Auto-approved: {}", cmd_str);
            true
        } else {
            self.confirmation_handler.confirm(cmd_str).await
        };
        
        if !is_approved {
            return Err(ForjaError::ToolError(format!(
                "[DENIED] User rejected the shell command: {}",
                cmd_str
            )));
        }

        // 2. 인가되었다면 명령어 실행
        let mut child = self.build_command(cmd_str);
        
        let output = child.output().await.map_err(|e| {
            ForjaError::ToolError(format!("Failed to execute '{}': {}", cmd_str, e))
        })?;

        // 3. 결과 수집 (UTF-8 변환)
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        
        // 0이면 성공, 그 외 에러
        if !output.status.success() {
            return Ok(json!({
                "status": "error",
                "exit_code": output.status.code(),
                "stdout": stdout,
                "stderr": stderr,
            }));
        }

        Ok(json!({
            "status": "success",
            "stdout": stdout,
            "stderr": stderr,
        }))
    }
}
