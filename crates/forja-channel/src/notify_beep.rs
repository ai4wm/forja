use forja_core::error::{ForjaError, Result};
use forja_core::notification::{Notification, NotificationLevel, Notifier};
use std::io::{self, Write};

pub struct BeepNotifier;

impl BeepNotifier {
    pub fn new() -> Self {
        Self
    }
}

impl Default for BeepNotifier {
    fn default() -> Self {
        Self::new()
    }
}

impl Notifier for BeepNotifier {
    fn notify(&self, notification: &Notification) -> Result<()> {
        if matches!(
            notification.level,
            NotificationLevel::Warning | NotificationLevel::Critical
        ) {
            let mut stderr = io::stderr();
            stderr
                .write_all(b"\x07")
                .map_err(|error| ForjaError::ChannelError(error.to_string()))?;
            stderr
                .flush()
                .map_err(|error| ForjaError::ChannelError(error.to_string()))?;
        }

        Ok(())
    }

    fn is_available(&self) -> bool {
        true
    }
}
