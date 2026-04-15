use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::{Duration, Instant};

pub(crate) const SHUTDOWN_DOUBLE_TAP_WINDOW: Duration = Duration::from_millis(1_500);

#[derive(Clone, Default)]
pub(crate) struct ShutdownSignal {
    triggered: Arc<AtomicBool>,
    armed_until: Arc<StdMutex<Option<Instant>>>,
    notify: Arc<tokio::sync::Notify>,
}

impl ShutdownSignal {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn trigger(&self) -> bool {
        let now = Instant::now();
        let mut armed_until = self
            .armed_until
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());

        if armed_until.is_some_and(|deadline| now <= deadline) {
            self.triggered.store(true, Ordering::SeqCst);
            *armed_until = None;
            self.notify.notify_one();
            return true;
        }

        *armed_until = Some(now + SHUTDOWN_DOUBLE_TAP_WINDOW);
        false
    }

    #[cfg(test)]
    pub(crate) fn is_armed(&self) -> bool {
        self.armed_until
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .is_some_and(|deadline| Instant::now() <= deadline)
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
}

#[cfg(test)]
mod tests {
    use super::{ShutdownSignal, SHUTDOWN_DOUBLE_TAP_WINDOW};

    #[tokio::test]
    async fn shutdown_signal_wait_returns_after_second_trigger() {
        let signal = ShutdownSignal::new();
        let wait_future = signal.wait();

        assert!(!signal.trigger());
        assert!(signal.is_armed());
        assert!(!signal.is_triggered());
        assert!(signal.trigger());

        tokio::time::timeout(std::time::Duration::from_millis(50), wait_future)
            .await
            .unwrap();
        assert!(signal.is_triggered());
    }

    #[tokio::test]
    async fn shutdown_signal_wait_does_not_return_after_first_trigger() {
        let signal = ShutdownSignal::new();
        assert!(!signal.trigger());

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
        assert!(!signal.trigger());
        assert!(signal.trigger());

        tokio::time::timeout(std::time::Duration::from_millis(50), signal.wait())
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn shutdown_signal_requires_new_double_tap_after_window_expires() {
        let signal = ShutdownSignal::new();
        let wait_future = signal.wait();

        assert!(!signal.trigger());
        tokio::time::sleep(SHUTDOWN_DOUBLE_TAP_WINDOW + std::time::Duration::from_millis(100))
            .await;

        assert!(!signal.is_armed());
        assert!(!signal.trigger());
        assert!(signal.is_armed());
        assert!(!signal.is_triggered());
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(50), wait_future)
                .await
                .is_err()
        );
    }
}
