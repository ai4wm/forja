use forja_core::error::Result;
use forja_core::notification::{Notification, NotificationLevel, Notifier};
use std::io::{self, Write};

pub struct TerminalNotifier;

impl TerminalNotifier {
    pub fn new() -> Self {
        Self
    }
}

impl Default for TerminalNotifier {
    fn default() -> Self {
        Self::new()
    }
}

impl Notifier for TerminalNotifier {
    fn notify(&self, notification: &Notification) -> Result<()> {
        let mut stderr = io::stderr();
        stderr
            .write_all(format_notification_banner(notification).as_bytes())
            .map_err(|error| forja_core::error::ForjaError::ChannelError(error.to_string()))?;
        stderr
            .flush()
            .map_err(|error| forja_core::error::ForjaError::ChannelError(error.to_string()))
    }

    fn is_available(&self) -> bool {
        true
    }
}

pub fn format_notification_banner(notification: &Notification) -> String {
    let color = match notification.level {
        NotificationLevel::Info => "\u{001b}[0m",
        NotificationLevel::Warning => "\u{001b}[33m",
        NotificationLevel::Critical => "\u{001b}[31m",
    };
    let reset = "\u{001b}[0m";
    format!(
        "\n{color}[Agent]  {}\n{}\n{reset}\n",
        notification.title, notification.body
    )
}

#[cfg(test)]
mod tests {
    use super::format_notification_banner;
    use forja_core::notification::{Notification, NotificationLevel};

    #[test]
    fn formatter_uses_warning_and_critical_colors() {
        let warning = format_notification_banner(&Notification::new(
            "Warning",
            "Body",
            NotificationLevel::Warning,
        ));
        let critical = format_notification_banner(&Notification::new(
            "Critical",
            "Body",
            NotificationLevel::Critical,
        ));
        let info = format_notification_banner(&Notification::new(
            "Info",
            "Body",
            NotificationLevel::Info,
        ));

        assert!(warning.contains("\u{001b}[33m"));
        assert!(critical.contains("\u{001b}[31m"));
        assert!(info.contains("\u{001b}[0m"));
    }
}
