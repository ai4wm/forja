use forja_core::error::Result;
use forja_core::notification::{Notification, Notifier};
use std::process::Command;
use std::sync::OnceLock;

static BURNT_TOAST_AVAILABLE: OnceLock<bool> = OnceLock::new();

pub struct ToastNotifier;

impl ToastNotifier {
    pub fn new() -> Self {
        Self
    }
}

impl Default for ToastNotifier {
    fn default() -> Self {
        Self::new()
    }
}

impl Notifier for ToastNotifier {
    fn notify(&self, notification: &Notification) -> Result<()> {
        if !self.is_available() {
            return Ok(());
        }

        let body = notification.body.replace('\'', "''");
        let title = notification.title.replace('\'', "''");
        std::thread::spawn(move || {
            let command = format!(
                "New-BurntToastNotification -Text 'Forja Agent', '{title}', '{body}'"
            );
            let _ = Command::new("powershell")
                .args(["-NoProfile", "-Command", &command])
                .spawn();
        });

        Ok(())
    }

    fn is_available(&self) -> bool {
        if !cfg!(target_os = "windows") {
            return false;
        }

        *BURNT_TOAST_AVAILABLE.get_or_init(|| {
            Command::new("powershell")
                .args([
                    "-NoProfile",
                    "-Command",
                    "Get-Module -ListAvailable BurntToast | Select-Object -First 1 | ForEach-Object { 'yes' }",
                ])
                .output()
                .ok()
                .map(|output| String::from_utf8_lossy(&output.stdout).contains("yes"))
                .unwrap_or(false)
        })
    }
}
