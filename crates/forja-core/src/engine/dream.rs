use super::Engine;
use crate::traits::{DreamRunStatus, DreamTrigger, NotificationLevel, NotificationTopic};
use chrono::Utc;
use std::future::pending;
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU64, Ordering},
};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::time::{Interval, MissedTickBehavior};

#[derive(Debug, Clone, Copy)]
pub struct DreamRuntimeConfig {
    pub enabled: bool,
    pub idle_after: Duration,
    pub shutdown_after: Duration,
}

impl DreamRuntimeConfig {
    fn sanitized(self) -> Self {
        Self {
            enabled: self.enabled,
            idle_after: self.idle_after.max(Duration::from_millis(10)),
            shutdown_after: self.shutdown_after.max(Duration::from_secs(1)),
        }
    }
}

#[derive(Debug)]
pub(super) struct DreamRuntimeState {
    running: AtomicBool,
    last_activity_millis: AtomicU64,
    last_completed_millis: AtomicU64,
}

impl DreamRuntimeState {
    fn new() -> Self {
        let now = now_millis();
        Self {
            running: AtomicBool::new(false),
            last_activity_millis: AtomicU64::new(now),
            last_completed_millis: AtomicU64::new(0),
        }
    }

    fn mark_started(&self) -> bool {
        self.running
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
    }

    fn finish(&self, completed_at_secs: Option<u64>) {
        if let Some(value) = completed_at_secs {
            self.last_completed_millis
                .store(value.saturating_mul(1_000), Ordering::SeqCst);
        }
        self.running.store(false, Ordering::SeqCst);
    }

    fn note_activity(&self) {
        self.last_activity_millis
            .store(now_millis(), Ordering::SeqCst);
    }

    fn idle_due(&self, idle_after: Duration) -> bool {
        let now = now_millis();
        let idle_after_millis = idle_after.as_millis() as u64;
        let last_activity = self.last_activity_millis.load(Ordering::SeqCst);
        let last_completed = self.last_completed_millis.load(Ordering::SeqCst);
        now.saturating_sub(last_activity) >= idle_after_millis
            && now.saturating_sub(last_completed) >= idle_after_millis
    }

    fn reset(&self) {
        let now = now_millis();
        self.running.store(false, Ordering::SeqCst);
        self.last_activity_millis.store(now, Ordering::SeqCst);
        self.last_completed_millis.store(0, Ordering::SeqCst);
    }
}

impl Engine {
    pub fn with_dream_runtime(mut self, config: DreamRuntimeConfig) -> Self {
        self.dream_runtime = Some(config.sanitized());
        self.dream_state = Some(Arc::new(DreamRuntimeState::new()));
        self
    }

    #[cfg(feature = "runtime")]
    pub(super) fn dream_interval(&self) -> Option<Interval> {
        let config = self.dream_runtime?;
        if !config.enabled {
            return None;
        }
        let mut interval = tokio::time::interval(config.idle_after);
        interval.set_missed_tick_behavior(MissedTickBehavior::Delay);
        Some(interval)
    }

    pub(super) fn note_user_activity(&self) {
        if let Some(state) = &self.dream_state {
            state.note_activity();
        }
    }

    #[cfg(feature = "runtime")]
    pub(super) async fn wait_for_dream_tick(interval: &mut Option<Interval>) {
        if let Some(interval) = interval {
            interval.tick().await;
        } else {
            pending::<()>().await;
        }
    }

    #[cfg(feature = "runtime")]
    pub(super) fn maybe_start_idle_dream(&self) {
        let Some(config) = self.dream_runtime else {
            return;
        };
        if !config.enabled {
            return;
        }
        let Some(state) = &self.dream_state else {
            return;
        };
        if !state.idle_due(config.idle_after) {
            return;
        }
        self.spawn_dream_task(DreamTrigger::Idle);
    }

    pub(super) fn handle_manual_dream_command(&self) -> String {
        if self.spawn_dream_task(DreamTrigger::Manual) {
            "Dream started in the background.".to_string()
        } else {
            "Dream is already in progress.".to_string()
        }
    }

    #[cfg(feature = "runtime")]
    pub(super) async fn run_shutdown_dream_if_due(&self) {
        let Some(config) = self.dream_runtime else {
            return;
        };
        if !config.enabled {
            return;
        }
        let Some(state) = &self.dream_state else {
            return;
        };
        if !state.mark_started() {
            return;
        }

        let Some(memory) = self.memory.clone() else {
            state.finish(None);
            return;
        };

        let last_completed = match memory.latest_dream_timestamp().await {
            Ok(value) => value,
            Err(error) => {
                eprintln!("[Dream] latest_dream_timestamp failed: {error}");
                state.finish(None);
                return;
            }
        };
        if !shutdown_due(last_completed, config.shutdown_after) {
            state.finish(None);
            return;
        }

        let outcome = memory.run_dream(DreamTrigger::Shutdown).await;
        self.finish_dream_run(state.clone(), DreamTrigger::Shutdown, outcome)
            .await;
    }

    pub(super) fn reset_dream_state(&self) {
        if let Some(state) = &self.dream_state {
            state.reset();
        }
    }

    fn spawn_dream_task(&self, trigger: DreamTrigger) -> bool {
        let Some(config) = self.dream_runtime else {
            return false;
        };
        if !config.enabled {
            return false;
        }
        let Some(state) = &self.dream_state else {
            return false;
        };
        if !state.mark_started() {
            return false;
        }

        let Some(memory) = self.memory.clone() else {
            state.finish(None);
            return false;
        };
        let channel = self.channel.clone();
        let autonomy = self.autonomy.clone();
        let state = state.clone();
        tokio::spawn(async move {
            let outcome = memory.run_dream(trigger).await;
            finish_dream_task(state, channel, autonomy, trigger, outcome).await;
        });
        true
    }

    async fn finish_dream_run(
        &self,
        state: Arc<DreamRuntimeState>,
        trigger: DreamTrigger,
        outcome: crate::error::Result<crate::traits::DreamRunOutcome>,
    ) {
        let channel = self.channel.clone();
        let autonomy = self.autonomy.clone();
        finish_dream_task(state, channel, autonomy, trigger, outcome).await;
    }
}

async fn finish_dream_task(
    state: Arc<DreamRuntimeState>,
    channel: Arc<dyn crate::traits::Channel>,
    autonomy: Option<crate::autonomy::loop_runner::AutonomousLoop>,
    trigger: DreamTrigger,
    outcome: crate::error::Result<crate::traits::DreamRunOutcome>,
) {
    let (message, completed_at, notify) = match outcome {
        Ok(outcome) => {
            let message = format!("Dream {trigger:?} completed: {}", outcome.summary);
            let notify = outcome.status != DreamRunStatus::AbortedConflict;
            (message, outcome.completed_at, notify)
        }
        Err(error) => (format!("Dream {trigger:?} failed: {error}"), None, false),
    };

    if let Some(autonomy) = autonomy {
        let timestamp = Utc::now().to_rfc3339();
        let _ = autonomy.append_notification_log(&format!("[{timestamp}] dream: {message}"));
    }

    if notify {
        let _ = channel
            .send_notification_with_level(
                &message,
                NotificationTopic::Autonomy,
                NotificationLevel::Info,
            )
            .await;
    }

    state.finish(completed_at);
}

fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn shutdown_due(last_completed: Option<u64>, threshold: Duration) -> bool {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    match last_completed {
        Some(last_completed) => now.saturating_sub(last_completed) >= threshold.as_secs(),
        None => true,
    }
}
