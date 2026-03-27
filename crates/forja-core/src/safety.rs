use crate::mode::ExecMode;
use serde_json::Value;

pub const DANGEROUS_COMMAND_PATTERNS: &[&str] = &[
    "rm",
    "del",
    "rmdir",
    "format",
    "mkfs",
    "dd",
    "chmod",
    "chown",
    "shutdown",
    "reboot",
    "kill",
    "pkill",
    "pip install",
    "npm install",
    "cargo install",
    "brew install",
    "apt install",
    "apt remove",
    "apt purge",
    "pacman -s",
    "pacman -r",
    "git push",
    "git reset --hard",
    "git clean",
    "drop table",
    "delete from",
    "truncate",
    "remove-item -recurse -force",
    "stop-process",
    "reg delete",
    "fdisk",
];

pub fn is_dangerous_command(command: &str) -> bool {
    let normalized = command.trim().to_lowercase();
    DANGEROUS_COMMAND_PATTERNS
        .iter()
        .any(|pattern| normalized.contains(pattern))
}

pub fn should_confirm_command(exec_mode: ExecMode, command: &str) -> bool {
    match exec_mode {
        ExecMode::Safe => true,
        ExecMode::Auto => is_dangerous_command(command),
        ExecMode::Trust => false,
    }
}

pub(crate) fn shell_command_from_args(args: &Value) -> Option<&str> {
    args.get("command")?.as_str().map(str::trim).filter(|cmd| !cmd.is_empty())
}

pub(crate) fn shell_confirmation_message(command: &str) -> String {
    format!("Confirm shell execution:\n> {command}")
}

pub(crate) fn shell_cancellation_result(command: &str) -> Value {
    serde_json::json!({
        "status": "warning",
        "output": format!("[WARNING] Execution cancelled by user: {command}"),
    })
}
