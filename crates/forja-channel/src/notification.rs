use forja_core::error::{ForjaError, Result};
use forja_core::traits::{NotificationLevel, NotificationState, NotificationTopic};
use std::process::Command;
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub struct NotificationManager {
    state: Arc<Mutex<NotificationState>>,
}

impl NotificationManager {
    pub fn new(state: NotificationState) -> Self {
        Self {
            state: Arc::new(Mutex::new(state)),
        }
    }

    pub fn state(&self) -> NotificationState {
        self.state.lock().map(|state| *state).unwrap_or_default()
    }

    pub fn set_enabled(&self, enabled: bool) -> NotificationState {
        let mut state = self.state.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        state.enabled = enabled;
        *state
    }

    pub fn send(
        &self,
        title: &str,
        body: &str,
        topic: NotificationTopic,
        level: NotificationLevel,
    ) -> Result<bool> {
        let state = self.state();
        if !state.enabled || level < state.min_level || !topic_allowed(state, topic) {
            return Ok(false);
        }

        #[cfg(target_os = "windows")]
        {
            if send_windows_toast(title, body).is_ok() {
                return Ok(true);
            }
        }

        send_desktop_notification(title, body)
    }
}

fn topic_allowed(state: NotificationState, topic: NotificationTopic) -> bool {
    match topic {
        NotificationTopic::Task => state.notify_tasks,
        NotificationTopic::Autonomy => state.notify_autonomy,
        NotificationTopic::Skill => state.notify_skills,
        NotificationTopic::Error => state.notify_errors,
    }
}

#[cfg(target_os = "windows")]
fn send_windows_toast(title: &str, body: &str) -> Result<()> {
    let escaped_title = escape_for_powershell(title);
    let escaped_body = escape_for_powershell(body);
    let script = format!(
        "$module = Get-Module -ListAvailable BurntToast; \
         if (-not $module) {{ exit 2 }}; \
         Import-Module BurntToast; \
         New-BurntToastNotification -Text '{escaped_title}','{escaped_body}'"
    );

    let status = Command::new("powershell")
        .args(["-NoProfile", "-Command", &script])
        .status()
        .map_err(|error| ForjaError::ChannelError(format!("PowerShell toast failed: {error}")))?;

    if status.success() {
        Ok(())
    } else {
        Err(ForjaError::ChannelError(format!(
            "BurntToast exited with status {}",
            status
        )))
    }
}

#[cfg(not(target_os = "windows"))]
fn send_windows_toast(_title: &str, _body: &str) -> Result<()> {
    Err(ForjaError::ChannelError(
        "Windows toast is unavailable on this platform".to_string(),
    ))
}

fn send_desktop_notification(title: &str, body: &str) -> Result<bool> {
    notify_rust::Notification::new()
        .summary(title)
        .body(body)
        .show()
        .map(|_| true)
        .map_err(|error| ForjaError::ChannelError(format!("Desktop notification failed: {error}")))
}

#[cfg(target_os = "windows")]
fn escape_for_powershell(value: &str) -> String {
    value.replace('\'', "''")
}

#[cfg(not(target_os = "windows"))]
fn escape_for_powershell(value: &str) -> String {
    value.to_string()
}
