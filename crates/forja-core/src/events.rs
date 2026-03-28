use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileChangeType {
    Created,
    Modified,
    Deleted,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SystemEvent {
    FileChanged { path: String, change_type: FileChangeType },
    TestFailed { test_name: String, error: String },
    TestPassed { test_name: String },
    HighMemoryUsage { percent: f32 },
    HighDiskUsage { percent: f32 },
    GitConflict { branch: String },
    CronTrigger { schedule_name: String },
    SkillFailed { skill_name: String, error: String },
    LongIdle { minutes: u64 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventSeverity {
    Info,
    Warning,
    Critical,
}

pub fn classify_severity(event: &SystemEvent) -> EventSeverity {
    match event {
        SystemEvent::FileChanged { .. } => EventSeverity::Info,
        SystemEvent::TestPassed { .. } => EventSeverity::Info,
        SystemEvent::CronTrigger { .. } => EventSeverity::Info,
        SystemEvent::LongIdle { .. } => EventSeverity::Info,
        SystemEvent::TestFailed { .. } => EventSeverity::Warning,
        SystemEvent::SkillFailed { .. } => EventSeverity::Warning,
        SystemEvent::HighMemoryUsage { percent } => {
            if *percent >= 95.0 {
                EventSeverity::Critical
            } else {
                EventSeverity::Warning
            }
        }
        SystemEvent::HighDiskUsage { percent } => {
            if *percent >= 95.0 {
                EventSeverity::Critical
            } else {
                EventSeverity::Warning
            }
        }
        SystemEvent::GitConflict { .. } => EventSeverity::Critical,
    }
}

#[derive(Clone, Default)]
pub struct EventQueue {
    queue: Arc<Mutex<VecDeque<SystemEvent>>>,
}

impl EventQueue {
    pub fn new() -> Self {
        Self {
            queue: Arc::new(Mutex::new(VecDeque::new())),
        }
    }

    pub fn push(&self, event: SystemEvent) {
        if let Ok(mut queue) = self.queue.lock() {
            queue.push_back(event);
        }
    }

    pub fn pop(&self) -> Option<SystemEvent> {
        self.queue.lock().ok().and_then(|mut queue| queue.pop_front())
    }

    pub fn drain(&self) -> Vec<SystemEvent> {
        self.queue
            .lock()
            .map(|mut queue| queue.drain(..).collect())
            .unwrap_or_default()
    }

    pub fn len(&self) -> usize {
        self.queue.lock().map(|queue| queue.len()).unwrap_or(0)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[cfg(test)]
mod tests {
    use super::{EventQueue, EventSeverity, FileChangeType, SystemEvent, classify_severity};
    use std::sync::Arc;
    use std::thread;

    #[test]
    fn classify_severity_matches_event_types() {
        assert_eq!(
            classify_severity(&SystemEvent::FileChanged {
                path: "src/main.rs".to_string(),
                change_type: FileChangeType::Modified,
            }),
            EventSeverity::Info
        );
        assert_eq!(
            classify_severity(&SystemEvent::TestFailed {
                test_name: "phase14".to_string(),
                error: "missing semicolon".to_string(),
            }),
            EventSeverity::Warning
        );
        assert_eq!(
            classify_severity(&SystemEvent::GitConflict {
                branch: "main".to_string(),
            }),
            EventSeverity::Critical
        );
    }

    #[test]
    fn event_queue_supports_push_pop_and_drain() {
        let queue = EventQueue::new();
        queue.push(SystemEvent::LongIdle { minutes: 30 });
        queue.push(SystemEvent::CronTrigger {
            schedule_name: "nightly".to_string(),
        });

        assert_eq!(queue.len(), 2);
        assert!(matches!(queue.pop(), Some(SystemEvent::LongIdle { .. })));
        assert_eq!(queue.drain().len(), 1);
        assert_eq!(queue.len(), 0);
    }

    #[test]
    fn event_queue_is_thread_safe_for_pushes() {
        let queue = Arc::new(EventQueue::new());
        let handles = (0..4)
            .map(|index| {
                let queue = queue.clone();
                thread::spawn(move || {
                    queue.push(SystemEvent::CronTrigger {
                        schedule_name: format!("job-{index}"),
                    });
                })
            })
            .collect::<Vec<_>>();

        for handle in handles {
            handle.join().unwrap();
        }

        assert_eq!(queue.len(), 4);
    }
}
