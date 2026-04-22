use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::{Duration, Instant};

pub(crate) const SHUTDOWN_DOUBLE_TAP_WINDOW: Duration = Duration::from_secs(2);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ShutdownTriggerState {
    Armed,
    Triggered,
}

#[derive(Clone, Copy)]
struct ArmedState {
    deadline: Instant,
    generation: u64,
}

#[derive(Clone, Default)]
pub(crate) struct ShutdownSignal {
    triggered: Arc<AtomicBool>,
    armed_state: Arc<StdMutex<Option<ArmedState>>>,
    generation: Arc<std::sync::atomic::AtomicU64>,
    notify: Arc<tokio::sync::Notify>,
}

impl ShutdownSignal {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn trigger(&self) -> ShutdownTriggerState {
        if self.is_triggered() {
            return ShutdownTriggerState::Triggered;
        }

        let now = Instant::now();
        let mut armed_state = self
            .armed_state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());

        if armed_state.is_some_and(|state| now <= state.deadline) {
            self.triggered.store(true, Ordering::SeqCst);
            *armed_state = None;
            self.notify.notify_waiters();
            return ShutdownTriggerState::Triggered;
        }

        let generation = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        *armed_state = Some(ArmedState {
            deadline: now + SHUTDOWN_DOUBLE_TAP_WINDOW,
            generation,
        });
        drop(armed_state);
        self.spawn_disarm_timer(generation);
        ShutdownTriggerState::Armed
    }

    #[cfg(test)]
    pub(crate) fn is_armed(&self) -> bool {
        self.clear_expired_arm();
        self.armed_state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .is_some()
    }

    pub(crate) fn is_triggered(&self) -> bool {
        self.triggered.load(Ordering::SeqCst)
    }

    pub(crate) async fn wait(&self) {
        if self.is_triggered() {
            return;
        }

        self.notify.notified().await;
    }

    fn spawn_disarm_timer(&self, generation: u64) {
        let signal = self.clone();
        std::thread::spawn(move || {
            std::thread::sleep(SHUTDOWN_DOUBLE_TAP_WINDOW);
            signal.clear_armed_generation(generation);
        });
    }

    #[cfg(test)]
    fn clear_expired_arm(&self) {
        let now = Instant::now();
        let mut armed_state = self
            .armed_state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if armed_state.is_some_and(|state| now > state.deadline) {
            *armed_state = None;
        }
    }

    fn clear_armed_generation(&self, generation: u64) {
        let now = Instant::now();
        let mut armed_state = self
            .armed_state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if armed_state.is_some_and(|state| {
            state.generation == generation && now > state.deadline && !self.is_triggered()
        }) {
            *armed_state = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{SHUTDOWN_DOUBLE_TAP_WINDOW, ShutdownSignal, ShutdownTriggerState};

    #[tokio::test]
    async fn shutdown_signal_wait_returns_after_second_trigger() {
        let signal = ShutdownSignal::new();
        let wait_future = signal.wait();

        assert_eq!(signal.trigger(), ShutdownTriggerState::Armed);
        assert!(signal.is_armed());
        assert!(!signal.is_triggered());
        assert_eq!(signal.trigger(), ShutdownTriggerState::Triggered);

        tokio::time::timeout(std::time::Duration::from_millis(50), wait_future)
            .await
            .unwrap();
        assert!(signal.is_triggered());
    }

    #[tokio::test]
    async fn shutdown_signal_wait_does_not_return_after_first_trigger() {
        let signal = ShutdownSignal::new();
        assert_eq!(signal.trigger(), ShutdownTriggerState::Armed);

        assert!(signal.is_armed());
        assert!(!signal.is_triggered());
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(50), signal.wait())
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn shutdown_signal_wait_returns_immediately_after_second_trigger() {
        let signal = ShutdownSignal::new();
        assert_eq!(signal.trigger(), ShutdownTriggerState::Armed);
        assert_eq!(signal.trigger(), ShutdownTriggerState::Triggered);

        tokio::time::timeout(std::time::Duration::from_millis(50), signal.wait())
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn shutdown_signal_requires_new_double_tap_after_window_expires() {
        let signal = ShutdownSignal::new();
        let wait_future = signal.wait();

        assert_eq!(signal.trigger(), ShutdownTriggerState::Armed);
        tokio::time::sleep(SHUTDOWN_DOUBLE_TAP_WINDOW + std::time::Duration::from_millis(100))
            .await;

        assert!(!signal.is_armed());
        assert_eq!(signal.trigger(), ShutdownTriggerState::Armed);
        assert!(signal.is_armed());
        assert!(!signal.is_triggered());
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(50), wait_future)
                .await
                .is_err()
        );
    }
}
