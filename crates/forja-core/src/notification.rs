use crate::error::{ForjaError, Result};
use crate::events::EventSeverity;
use chrono::{DateTime, Utc};
use std::collections::VecDeque;
use std::sync::Mutex;

const HISTORY_LIMIT: usize = 50;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum NotificationLevel {
    Info,
    Warning,
    Critical,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notification {
    pub title: String,
    pub body: String,
    pub level: NotificationLevel,
    pub timestamp: DateTime<Utc>,
}

impl Notification {
    pub fn new(title: impl Into<String>, body: impl Into<String>, level: NotificationLevel) -> Self {
        Self {
            title: title.into(),
            body: body.into(),
            level,
            timestamp: Utc::now(),
        }
    }
}

pub fn notification_level_from_severity(severity: &EventSeverity) -> NotificationLevel {
    match severity {
        EventSeverity::Info => NotificationLevel::Info,
        EventSeverity::Warning => NotificationLevel::Warning,
        EventSeverity::Critical => NotificationLevel::Critical,
    }
}

pub trait Notifier: Send + Sync {
    fn notify(&self, notification: &Notification) -> Result<()>;
    fn is_available(&self) -> bool;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NotifyCommand {
    Test,
    Off,
    On,
    Status,
    History(usize),
}

pub fn parse_notify_command(input: &str) -> Option<NotifyCommand> {
    let trimmed = input.trim();
    match trimmed {
        "/notify test" => Some(NotifyCommand::Test),
        "/notify off" => Some(NotifyCommand::Off),
        "/notify on" => Some(NotifyCommand::On),
        "/notify status" => Some(NotifyCommand::Status),
        "/notify history" => Some(NotifyCommand::History(10)),
        _ => trimmed
            .strip_prefix("/notify history ")
            .and_then(|value| value.trim().parse::<usize>().ok())
            .map(|count| NotifyCommand::History(count.max(1))),
    }
}

struct RouterEntry {
    name: String,
    notifier: Box<dyn Notifier>,
}

pub struct NotificationRouter {
    entries: Vec<RouterEntry>,
    enabled: Mutex<bool>,
    min_level: Mutex<NotificationLevel>,
    history: Mutex<VecDeque<Notification>>,
}

impl NotificationRouter {
    pub fn new(enabled: bool, min_level: NotificationLevel) -> Self {
        Self {
            entries: Vec::new(),
            enabled: Mutex::new(enabled),
            min_level: Mutex::new(min_level),
            history: Mutex::new(VecDeque::new()),
        }
    }

    pub fn add_notifier(&mut self, name: impl Into<String>, notifier: Box<dyn Notifier>) {
        self.entries.push(RouterEntry {
            name: name.into(),
            notifier,
        });
    }

    pub fn notify(&self, notification: &Notification) -> Result<()> {
        if !self.is_enabled() || notification.level < self.min_level() {
            return Ok(());
        }

        self.push_history(notification.clone());

        let mut first_error = None;
        for entry in &self.entries {
            if !entry.notifier.is_available() {
                continue;
            }
            if let Err(error) = entry.notifier.notify(notification)
                && first_error.is_none()
            {
                first_error = Some(error);
            }
        }

        if let Some(error) = first_error {
            return Err(error);
        }

        Ok(())
    }

    pub fn set_enabled(&self, enabled: bool) {
        if let Ok(mut current) = self.enabled.lock() {
            *current = enabled;
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled.lock().map(|enabled| *enabled).unwrap_or(true)
    }

    pub fn set_min_level(&self, min_level: NotificationLevel) {
        if let Ok(mut current) = self.min_level.lock() {
            *current = min_level;
        }
    }

    pub fn min_level(&self) -> NotificationLevel {
        self.min_level
            .lock()
            .map(|level| *level)
            .unwrap_or(NotificationLevel::Warning)
    }

    pub fn history(&self, count: usize) -> Vec<Notification> {
        self.history
            .lock()
            .map(|history| {
                history
                    .iter()
                    .rev()
                    .take(count)
                    .cloned()
                    .collect::<Vec<_>>()
                    .into_iter()
                    .rev()
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn status_lines(&self) -> Vec<String> {
        let enabled = self.is_enabled();
        let min_level = match self.min_level() {
            NotificationLevel::Info => "info",
            NotificationLevel::Warning => "warning",
            NotificationLevel::Critical => "critical",
        };

        let mut lines = vec![
            format!("Notifications enabled: {enabled}"),
            format!("Minimum level: {min_level}"),
        ];

        lines.extend(self.entries.iter().map(|entry| {
            let availability = if entry.notifier.is_available() {
                "available"
            } else {
                "unavailable"
            };
            format!("- {}: {availability}", entry.name)
        }));

        lines
    }

    fn push_history(&self, notification: Notification) {
        if let Ok(mut history) = self.history.lock() {
            history.push_back(notification);
            while history.len() > HISTORY_LIMIT {
                let _ = history.pop_front();
            }
        }
    }
}

pub fn parse_notification_level(value: &str) -> Result<NotificationLevel> {
    match value.trim().to_lowercase().as_str() {
        "info" => Ok(NotificationLevel::Info),
        "warning" => Ok(NotificationLevel::Warning),
        "critical" => Ok(NotificationLevel::Critical),
        other => Err(ForjaError::Internal(format!(
            "Unsupported notification level: {other}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        Notification, NotificationLevel, NotificationRouter, Notifier, NotifyCommand,
        notification_level_from_severity, parse_notification_level, parse_notify_command,
    };
    use crate::error::Result;
    use crate::events::EventSeverity;
    use std::sync::Mutex;

    struct RecordingNotifier {
        available: bool,
        seen: Mutex<Vec<String>>,
    }

    impl RecordingNotifier {
        fn new(available: bool) -> Self {
            Self {
                available,
                seen: Mutex::new(Vec::new()),
            }
        }
    }

    impl Notifier for RecordingNotifier {
        fn notify(&self, notification: &Notification) -> Result<()> {
            if let Ok(mut seen) = self.seen.lock() {
                seen.push(notification.title.clone());
            }
            Ok(())
        }

        fn is_available(&self) -> bool {
            self.available
        }
    }

    #[test]
    fn notification_level_maps_from_event_severity() {
        assert_eq!(
            notification_level_from_severity(&EventSeverity::Info),
            NotificationLevel::Info
        );
        assert_eq!(
            notification_level_from_severity(&EventSeverity::Warning),
            NotificationLevel::Warning
        );
        assert_eq!(
            notification_level_from_severity(&EventSeverity::Critical),
            NotificationLevel::Critical
        );
    }

    #[test]
    fn router_skips_unavailable_notifiers_and_keeps_history() {
        let mut router = NotificationRouter::new(true, NotificationLevel::Info);
        router.add_notifier("available", Box::new(RecordingNotifier::new(true)));
        router.add_notifier("unavailable", Box::new(RecordingNotifier::new(false)));

        router
            .notify(&Notification::new(
                "Test",
                "Body",
                NotificationLevel::Warning,
            ))
            .unwrap();

        assert_eq!(router.history(10).len(), 1);
        assert!(router
            .status_lines()
            .iter()
            .any(|line| line.contains("unavailable")));
    }

    #[test]
    fn router_ring_buffer_drops_oldest_entries_after_fifty() {
        let mut router = NotificationRouter::new(true, NotificationLevel::Info);
        router.add_notifier("available", Box::new(RecordingNotifier::new(true)));

        for index in 0..60 {
            router
                .notify(&Notification::new(
                    format!("n{index}"),
                    "Body",
                    NotificationLevel::Info,
                ))
                .unwrap();
        }

        let history = router.history(100);
        assert_eq!(history.len(), 50);
        assert_eq!(history.first().map(|item| item.title.as_str()), Some("n10"));
    }

    #[test]
    fn parse_notify_command_matches_supported_variants() {
        assert_eq!(parse_notify_command("/notify test"), Some(NotifyCommand::Test));
        assert_eq!(parse_notify_command("/notify off"), Some(NotifyCommand::Off));
        assert_eq!(parse_notify_command("/notify on"), Some(NotifyCommand::On));
        assert_eq!(parse_notify_command("/notify status"), Some(NotifyCommand::Status));
        assert_eq!(parse_notify_command("/notify history"), Some(NotifyCommand::History(10)));
        assert_eq!(
            parse_notify_command("/notify history 5"),
            Some(NotifyCommand::History(5))
        );
    }

    #[test]
    fn parse_notification_level_supports_config_values() {
        assert_eq!(
            parse_notification_level("warning").unwrap(),
            NotificationLevel::Warning
        );
        assert!(parse_notification_level("unknown").is_err());
    }
}
