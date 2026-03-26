use async_trait::async_trait;
use forja_core::traits::Tool;
use forja_tools::confirm::ConfirmationHandler;
use forja_tools::ShellTool;
use serde_json::json;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

struct AllowConfirmation;

#[async_trait]
impl ConfirmationHandler for AllowConfirmation {
    async fn confirm(&self, _cmd: &str, _dangerous: bool) -> bool {
        true
    }
}

struct DenyConfirmation;

#[async_trait]
impl ConfirmationHandler for DenyConfirmation {
    async fn confirm(&self, _cmd: &str, _dangerous: bool) -> bool {
        false
    }
}

fn unique_temp_dir(name: &str) -> std::path::PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    std::env::temp_dir().join(format!("forja_{name}_{nanos}"))
}

#[cfg(target_os = "windows")]
fn safe_echo_command() -> &'static str {
    "Write-Output hello"
}

#[cfg(not(target_os = "windows"))]
fn safe_echo_command() -> &'static str {
    "echo hello"
}

#[cfg(target_os = "windows")]
fn timeout_command() -> &'static str {
    "Start-Sleep -Seconds 2"
}

#[cfg(not(target_os = "windows"))]
fn timeout_command() -> &'static str {
    "sleep 2"
}

#[cfg(target_os = "windows")]
fn mixed_output_command() -> &'static str {
    "Write-Output out; [Console]::Error.WriteLine('err')"
}

#[cfg(not(target_os = "windows"))]
fn mixed_output_command() -> &'static str {
    "printf 'out\\n'; printf 'err\\n' 1>&2"
}

#[cfg(target_os = "windows")]
fn dangerous_delete_command(path: &str) -> String {
    format!("Remove-Item -Recurse -Force '{path}'")
}

#[cfg(not(target_os = "windows"))]
fn dangerous_delete_command(path: &str) -> String {
    format!("rm -rf '{path}'")
}

#[tokio::test]
async fn safe_command_execution_returns_output() {
    let tool = ShellTool::with_settings(
        Arc::new(AllowConfirmation),
        Duration::from_secs(30),
        false,
    );

    let result = tool
        .execute(json!({ "command": safe_echo_command() }))
        .await
        .unwrap();

    assert_eq!(result["status"], json!("success"));
    assert!(result["output"].as_str().unwrap().contains("hello"));
}

#[tokio::test]
async fn dangerous_command_returns_warning_when_not_confirmed() {
    let tool = ShellTool::with_settings(
        Arc::new(DenyConfirmation),
        Duration::from_secs(30),
        false,
    );

    let result = tool.execute(json!({ "command": "rm -rf /" })).await.unwrap();

    assert_eq!(result["status"], json!("warning"));
    assert!(result["output"].as_str().unwrap().contains("[WARNING]"));
    assert!(result["output"].as_str().unwrap().contains("rm -rf /"));
}

#[test]
fn dangerous_command_patterns_are_all_detected() {
    let commands = [
        "rm -rf /",
        "del /f test.txt",
        "format c:",
        "fdisk /dev/sda",
        "shutdown /s /t 0",
        "reboot",
        "Remove-Item -Recurse -Force foo",
        "Stop-Process -Name notepad",
        "reg delete HKCU\\Software\\Test /f",
        "mkfs.ext4 /dev/sda1",
        "dd if=/dev/zero of=/dev/sda",
    ];

    for command in commands {
        assert!(ShellTool::is_dangerous_command(command), "{command}");
    }
}

#[tokio::test]
async fn command_timeout_returns_error() {
    let tool = ShellTool::with_settings(
        Arc::new(AllowConfirmation),
        Duration::from_secs(1),
        false,
    );

    let error = tool
        .execute(json!({ "command": timeout_command() }))
        .await
        .unwrap_err();

    assert!(error.to_string().to_lowercase().contains("timeout"));
}

#[tokio::test]
async fn stdout_and_stderr_are_combined() {
    let tool = ShellTool::with_settings(
        Arc::new(AllowConfirmation),
        Duration::from_secs(30),
        false,
    );

    let result = tool
        .execute(json!({ "command": mixed_output_command() }))
        .await
        .unwrap();
    let output = result["output"].as_str().unwrap();

    assert!(output.contains("out"));
    assert!(output.contains("err"));
}

#[tokio::test]
async fn unsafe_mode_executes_dangerous_command() {
    let temp_dir = unique_temp_dir("phase17a_shell_unsafe");
    std::fs::create_dir_all(&temp_dir).unwrap();
    std::fs::write(temp_dir.join("file.txt"), "hello").unwrap();
    let tool = ShellTool::with_settings(
        Arc::new(DenyConfirmation),
        Duration::from_secs(30),
        true,
    );
    let command = dangerous_delete_command(temp_dir.to_str().unwrap());

    let result = tool.execute(json!({ "command": command })).await.unwrap();

    assert_eq!(result["status"], json!("success"));
    assert!(!temp_dir.exists());
}

#[tokio::test]
async fn empty_command_returns_error() {
    let tool = ShellTool::with_settings(
        Arc::new(AllowConfirmation),
        Duration::from_secs(30),
        false,
    );

    let error = tool.execute(json!({ "command": "   " })).await.unwrap_err();

    assert!(error.to_string().to_lowercase().contains("empty"));
}

#[cfg(target_os = "windows")]
#[test]
fn windows_shell_invocation_uses_powershell() {
    let (program, args) = ShellTool::shell_invocation("Get-Date");

    assert_eq!(program, "powershell");
    assert_eq!(args, vec!["-NoProfile", "-Command", "Get-Date"]);
}

#[cfg(not(target_os = "windows"))]
#[test]
fn unix_shell_invocation_uses_sh() {
    let (program, args) = ShellTool::shell_invocation("date");

    assert_eq!(program, "sh");
    assert_eq!(args, vec!["-c", "date"]);
}
