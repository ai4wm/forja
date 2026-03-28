use crate::decision::{Decision, decide, describe_event};
use crate::events::{EventQueue, SystemEvent, classify_severity};
use crate::mode::ExecMode;
use crate::notification::{
    Notification, NotificationLevel, NotificationRouter, notification_level_from_severity,
};
use crate::traits::LlmProvider;
use crate::watchers::{FileWatcher, GitWatcher, IdleWatcher, SystemWatcher, WatcherConfig, WatcherHandles, watcher_names};
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;
use tokio::time::MissedTickBehavior;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentAction {
    Report(String),
    Escalate { context: String, question: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentCommand {
    Status,
    Logs(usize),
    Pause,
    Resume,
}

#[derive(Debug, Clone)]
pub struct BackgroundAgentOptions {
    pub watch_files: bool,
    pub watch_system: bool,
    pub watch_git: bool,
    pub idle_threshold_minutes: u64,
    pub auto_fix: bool,
    pub cwd: PathBuf,
    pub log_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentStatusSnapshot {
    pub running: bool,
    pub paused: bool,
    pub queue_len: usize,
    pub last_events: Vec<String>,
    pub watchers: Vec<String>,
}

pub fn parse_agent_command(input: &str) -> Option<AgentCommand> {
    let trimmed = input.trim();
    if trimmed == "/agent status" || trimmed == "/background" {
        return Some(AgentCommand::Status);
    }
    if trimmed == "/agent pause" || trimmed == "/background off" {
        return Some(AgentCommand::Pause);
    }
    if trimmed == "/agent resume" || trimmed == "/background auto" {
        return Some(AgentCommand::Resume);
    }
    if let Some(value) = trimmed.strip_prefix("/agent logs ") {
        let count = value.trim().parse::<usize>().ok().filter(|count| *count > 0).unwrap_or(10);
        return Some(AgentCommand::Logs(count));
    }
    if trimmed == "/agent logs" {
        return Some(AgentCommand::Logs(10));
    }
    None
}

pub fn format_escalation_prompt(context: &str, question: &str) -> String {
    format!("[Background Agent] {context}\nQuestion: {question}")
}

pub struct BackgroundManager {
    provider: Option<Arc<dyn LlmProvider>>,
    provider_name: Option<String>,
    model_name: Option<String>,
    interval: Duration,
    enabled: bool,
    active: Arc<AtomicBool>,
    paused: Arc<AtomicBool>,
    shutdown_tx: Option<oneshot::Sender<()>>,
    task: Option<JoinHandle<()>>,
    event_queue: EventQueue,
    recent_events: Arc<Mutex<VecDeque<SystemEvent>>>,
    log_path: PathBuf,
    auto_fix: bool,
    watcher_config: WatcherConfig,
    watcher_shutdown: Arc<AtomicBool>,
    watcher_handles: Option<WatcherHandles>,
    action_tx: Option<mpsc::UnboundedSender<AgentAction>>,
    exec_mode_handle: Option<Arc<Mutex<ExecMode>>>,
    last_user_activity: Arc<Mutex<Instant>>,
    notification_router: Option<Arc<NotificationRouter>>,
}

impl BackgroundManager {
    pub fn new(interval_seconds: u64) -> Self {
        let interval = Duration::from_secs(interval_seconds.max(1));
        Self {
            provider: None,
            provider_name: None,
            model_name: None,
            interval,
            enabled: false,
            active: Arc::new(AtomicBool::new(false)),
            paused: Arc::new(AtomicBool::new(false)),
            shutdown_tx: None,
            task: None,
            event_queue: EventQueue::new(),
            recent_events: Arc::new(Mutex::new(VecDeque::new())),
            log_path: PathBuf::from(".forja/logs/agent.log"),
            auto_fix: true,
            watcher_config: WatcherConfig {
                watch_files: true,
                watch_system: true,
                watch_git: true,
                idle_threshold_minutes: 30,
                cwd: PathBuf::from("."),
                interval,
            },
            watcher_shutdown: Arc::new(AtomicBool::new(false)),
            watcher_handles: None,
            action_tx: None,
            exec_mode_handle: None,
            last_user_activity: Arc::new(Mutex::new(Instant::now())),
            notification_router: None,
        }
    }

    pub fn configure(
        &mut self,
        provider_name: String,
        model_name: String,
        provider: Arc<dyn LlmProvider>,
        interval_seconds: u64,
    ) {
        self.provider = Some(provider);
        self.provider_name = Some(provider_name);
        self.model_name = Some(model_name);
        self.interval = Duration::from_secs(interval_seconds.max(1));
        self.watcher_config.interval = self.interval;
        self.enabled = true;
    }

    pub fn configure_agent(&mut self, options: BackgroundAgentOptions) {
        self.watcher_config.watch_files = options.watch_files;
        self.watcher_config.watch_system = options.watch_system;
        self.watcher_config.watch_git = options.watch_git;
        self.watcher_config.idle_threshold_minutes = options.idle_threshold_minutes;
        self.watcher_config.cwd = options.cwd;
        self.log_path = options.log_path;
        self.auto_fix = options.auto_fix;
    }

    pub fn set_action_sender(&mut self, action_tx: mpsc::UnboundedSender<AgentAction>) {
        self.action_tx = Some(action_tx);
    }

    pub fn set_notification_router(&mut self, notification_router: Arc<NotificationRouter>) {
        self.notification_router = Some(notification_router);
    }

    pub fn set_exec_mode_handle(&mut self, exec_mode_handle: Arc<Mutex<ExecMode>>) {
        self.exec_mode_handle = Some(exec_mode_handle);
    }

    pub fn disable(&mut self) {
        self.provider = None;
        self.provider_name = None;
        self.model_name = None;
        self.enabled = false;
    }

    pub fn pause(&self) {
        self.paused.store(true, Ordering::SeqCst);
    }

    pub fn resume(&self) {
        self.paused.store(false, Ordering::SeqCst);
    }

    pub fn is_paused(&self) -> bool {
        self.paused.load(Ordering::SeqCst)
    }

    pub fn record_user_activity(&self) {
        if let Ok(mut last_user_activity) = self.last_user_activity.lock() {
            *last_user_activity = Instant::now();
        }
    }

    pub fn event_queue(&self) -> EventQueue {
        self.event_queue.clone()
    }

    pub fn start(&mut self) {
        if !self.enabled || self.provider.is_none() || self.is_active() {
            return;
        }

        self.start_watchers();

        let interval = self.interval;
        let active = self.active.clone();
        let paused = self.paused.clone();
        let queue = self.event_queue.clone();
        let recent_events = self.recent_events.clone();
        let log_path = self.log_path.clone();
        let auto_fix = self.auto_fix;
        let action_tx = self.action_tx.clone();
        let exec_mode_handle = self.exec_mode_handle.clone();
        let notification_router = self.notification_router.clone();
        let (shutdown_tx, mut shutdown_rx) = oneshot::channel::<()>();
        self.shutdown_tx = Some(shutdown_tx);
        active.store(true, Ordering::SeqCst);

        self.task = Some(tokio::spawn(async move {
            let mut ticker = tokio::time::interval(interval);
            ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);

            loop {
                tokio::select! {
                    _ = ticker.tick() => {
                        if paused.load(Ordering::SeqCst) {
                            continue;
                        }
                        for event in queue.drain() {
                            push_recent_event(&recent_events, event.clone());
                            let severity = classify_severity(&event);
                            let exec_mode = exec_mode_handle
                                .as_ref()
                                .and_then(|handle| handle.lock().ok().map(|mode| *mode))
                                .unwrap_or(ExecMode::Auto);
                            let decision = decide(&event, &severity, &exec_mode);
                            handle_decision(
                                &event,
                                &severity,
                                decision,
                                auto_fix,
                                action_tx.as_ref(),
                                notification_router.as_ref(),
                                log_path.as_path(),
                            );
                        }
                    }
                    _ = &mut shutdown_rx => {
                        active.store(false, Ordering::SeqCst);
                        break;
                    }
                }
            }
        }));
    }

    fn start_watchers(&mut self) {
        self.watcher_shutdown.store(false, Ordering::SeqCst);
        let mut watcher_handles = WatcherHandles::new();
        let config = self.watcher_config.clone();

        if config.watch_files {
            watcher_handles.push(FileWatcher::spawn(
                self.event_queue.clone(),
                self.watcher_shutdown.clone(),
                config.clone(),
            ));
        }

        if config.watch_system {
            watcher_handles.push(SystemWatcher::spawn(
                self.event_queue.clone(),
                self.watcher_shutdown.clone(),
                config.clone(),
            ));
        }

        if config.watch_git {
            watcher_handles.push(GitWatcher::spawn(
                self.event_queue.clone(),
                self.watcher_shutdown.clone(),
                config.clone(),
            ));
        }

        watcher_handles.push(IdleWatcher::spawn(
            self.event_queue.clone(),
            self.watcher_shutdown.clone(),
            config,
            self.last_user_activity.clone(),
        ));

        self.watcher_handles = Some(watcher_handles);
    }

    pub async fn stop(&mut self) {
        if let Some(shutdown_tx) = self.shutdown_tx.take() {
            let _ = shutdown_tx.send(());
        }

        if let Some(task) = self.task.take() {
            let _ = task.await;
        }

        self.watcher_shutdown.store(true, Ordering::SeqCst);
        if let Some(watcher_handles) = self.watcher_handles.take() {
            watcher_handles.stop().await;
        }

        self.active.store(false, Ordering::SeqCst);
    }

    pub fn is_active(&self) -> bool {
        self.active.load(Ordering::SeqCst)
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled && self.provider.is_some()
    }

    pub fn get_provider(&self) -> Option<Arc<dyn LlmProvider>> {
        self.provider.clone()
    }

    pub fn provider_name(&self) -> Option<&str> {
        self.provider_name.as_deref()
    }

    pub fn model_name(&self) -> Option<&str> {
        self.model_name.as_deref()
    }

    pub fn interval_seconds(&self) -> u64 {
        self.interval.as_secs()
    }

    pub fn queue_len(&self) -> usize {
        self.event_queue.len()
    }

    pub fn get_recent_logs(&self, count: usize) -> Vec<String> {
        let Ok(contents) = std::fs::read_to_string(&self.log_path) else {
            return Vec::new();
        };
        contents
            .lines()
            .rev()
            .take(count)
            .map(|line| line.to_string())
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect()
    }

    pub fn status_snapshot(&self) -> AgentStatusSnapshot {
        AgentStatusSnapshot {
            running: self.is_active(),
            paused: self.is_paused(),
            queue_len: self.queue_len(),
            last_events: self
                .recent_events
                .lock()
                .map(|events| events.iter().rev().take(5).map(describe_event).collect::<Vec<_>>())
                .unwrap_or_default()
                .into_iter()
                .rev()
                .collect(),
            watchers: watcher_names(&self.watcher_config),
        }
    }
}

fn handle_decision(
    event: &SystemEvent,
    _severity: &crate::events::EventSeverity,
    decision: Decision,
    auto_fix_enabled: bool,
    action_tx: Option<&mpsc::UnboundedSender<AgentAction>>,
    notification_router: Option<&Arc<NotificationRouter>>,
    log_path: &Path,
) {
    let notification_level = notification_level_from_severity(_severity);
    match decision {
        Decision::Ignore => {}
        Decision::Log => {
            let _ = append_log(log_path, &format!("[INFO] {}", describe_event(event)));
        }
        Decision::AutoFix { action } => {
            if !auto_fix_enabled {
                if let Some(action_tx) = action_tx {
                    let _ = action_tx.send(AgentAction::Report(format!(
                        "Auto-fix disabled: {}",
                        describe_event(event)
                    )));
                }
                return;
            }

            if !is_allowed_auto_fix(&action) {
                let _ = append_log(log_path, &format!("[WARN] Refused unsafe auto-fix: {action}"));
                return;
            }

            let outcome = run_fix_command(&action);
            let _ = append_log(log_path, &format!("[AUTO_FIX] {action}: {outcome}"));
            route_notification(
                notification_router,
                Notification::new(
                    "Auto-fix completed",
                    format!("Auto-fixed: ran {action}"),
                    NotificationLevel::Info,
                ),
            );
            if let Some(action_tx) = action_tx {
                let _ = action_tx.send(AgentAction::Report(format!(
                    "Auto-fix result: {action} -> {outcome}"
                )));
            }
        }
        Decision::Report { message } => {
            let _ = append_log(log_path, &format!("[REPORT] {message}"));
            route_notification(
                notification_router,
                Notification::new("Forja Agent", message.clone(), notification_level),
            );
            if let Some(action_tx) = action_tx {
                let _ = action_tx.send(AgentAction::Report(message));
            }
        }
        Decision::Escalate { context, question } => {
            let _ = append_log(log_path, &format!("[ESCALATE] {context}"));
            route_notification(
                notification_router,
                Notification::new(
                    "Forja Agent escalation",
                    format!("{context}\nQuestion: {question}"),
                    notification_level,
                ),
            );
            if let Some(action_tx) = action_tx {
                let _ = action_tx.send(AgentAction::Escalate { context, question });
            }
        }
    }
}

fn route_notification(
    notification_router: Option<&Arc<NotificationRouter>>,
    notification: Notification,
) {
    if let Some(notification_router) = notification_router {
        let _ = notification_router.notify(&notification);
    }
}

fn append_log(path: &Path, line: &str) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S");
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    use std::io::Write;
    writeln!(file, "[{timestamp}] {line}")
}

fn is_allowed_auto_fix(action: &str) -> bool {
    let normalized = action.to_lowercase();
    ["cargo fmt", "cargo fix --allow-dirty", "cargo test"]
        .iter()
        .any(|allowed| normalized.starts_with(allowed))
}

fn run_fix_command(action: &str) -> String {
    let output = if cfg!(target_os = "windows") {
        Command::new("powershell")
            .args(["-NoProfile", "-Command", action])
            .output()
    } else {
        Command::new("sh").args(["-c", action]).output()
    };

    match output {
        Ok(output) if output.status.success() => "success".to_string(),
        Ok(output) => String::from_utf8_lossy(&output.stderr).trim().to_string(),
        Err(error) => error.to_string(),
    }
}

fn push_recent_event(events: &Arc<Mutex<VecDeque<SystemEvent>>>, event: SystemEvent) {
    if let Ok(mut events) = events.lock() {
        events.push_back(event);
        while events.len() > 50 {
            let _ = events.pop_front();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::{ForjaError, Result};
    use crate::events::{FileChangeType, SystemEvent};
    use crate::traits::LlmProvider;
    use crate::types::{Message, ToolDefinition};
    use async_trait::async_trait;
    use std::pin::Pin;
    use std::time::{SystemTime, UNIX_EPOCH};
    use tokio_stream::Stream;

    struct DummyProvider;

    #[async_trait]
    impl LlmProvider for DummyProvider {
        async fn chat(
            &self,
            _messages: &[Message],
            _tools: Option<&[ToolDefinition]>,
        ) -> Result<Message> {
            Err(ForjaError::LlmError("not implemented".to_string()))
        }

        async fn stream(
            &self,
            _messages: &[Message],
            _tools: Option<&[ToolDefinition]>,
        ) -> Result<Pin<Box<dyn Stream<Item = Result<String>> + Send>>> {
            Err(ForjaError::LlmError("not implemented".to_string()))
        }
    }

    fn unique_temp_dir(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        std::env::temp_dir().join(format!("forja_agent_{name}_{nanos}"))
    }

    #[tokio::test]
    async fn background_manager_starts_and_stops() {
        let provider: Arc<dyn LlmProvider> = Arc::new(DummyProvider);
        let mut manager = BackgroundManager::new(1);
        manager.configure(
            "groq".to_string(),
            "llama-3.1-8b-instant".to_string(),
            provider,
            1,
        );
        manager.configure_agent(BackgroundAgentOptions {
            watch_files: false,
            watch_system: false,
            watch_git: false,
            idle_threshold_minutes: 30,
            auto_fix: true,
            cwd: PathBuf::from("."),
            log_path: unique_temp_dir("logs").join("agent.log"),
        });

        manager.start();
        tokio::time::sleep(Duration::from_millis(20)).await;

        assert!(manager.is_active());
        assert!(manager.is_enabled());

        manager.stop().await;

        assert!(!manager.is_active());
    }

    #[test]
    fn background_manager_returns_provider_and_metadata() {
        let provider: Arc<dyn LlmProvider> = Arc::new(DummyProvider);
        let mut manager = BackgroundManager::new(30);
        manager.configure(
            "openrouter".to_string(),
            "meta-llama/llama-3.1-8b-instruct:free".to_string(),
            provider.clone(),
            30,
        );

        assert!(manager.get_provider().is_some());
        assert_eq!(manager.provider_name(), Some("openrouter"));
        assert_eq!(
            manager.model_name(),
            Some("meta-llama/llama-3.1-8b-instruct:free")
        );
        assert_eq!(manager.interval_seconds(), 30);
    }

    #[test]
    fn parse_agent_command_supports_agent_and_background_aliases() {
        assert_eq!(parse_agent_command("/agent status"), Some(AgentCommand::Status));
        assert_eq!(parse_agent_command("/agent logs 5"), Some(AgentCommand::Logs(5)));
        assert_eq!(parse_agent_command("/agent pause"), Some(AgentCommand::Pause));
        assert_eq!(parse_agent_command("/agent resume"), Some(AgentCommand::Resume));
        assert_eq!(parse_agent_command("/background"), Some(AgentCommand::Status));
        assert_eq!(parse_agent_command("/background off"), Some(AgentCommand::Pause));
        assert_eq!(parse_agent_command("/background auto"), Some(AgentCommand::Resume));
    }

    #[test]
    fn status_snapshot_includes_recent_events_and_watchers() {
        let mut manager = BackgroundManager::new(30);
        manager.configure_agent(BackgroundAgentOptions {
            watch_files: true,
            watch_system: true,
            watch_git: false,
            idle_threshold_minutes: 30,
            auto_fix: true,
            cwd: PathBuf::from("."),
            log_path: unique_temp_dir("status").join("agent.log"),
        });
        push_recent_event(
            &manager.recent_events,
            SystemEvent::FileChanged {
                path: "src/main.rs".to_string(),
                change_type: FileChangeType::Modified,
            },
        );

        let snapshot = manager.status_snapshot();

        assert!(snapshot.watchers.contains(&"file".to_string()));
        assert!(snapshot.watchers.contains(&"system".to_string()));
        assert_eq!(snapshot.last_events.len(), 1);
    }

    #[test]
    fn escalation_prompt_format_matches_expected_shape() {
        let prompt = format_escalation_prompt(
            "Git conflict detected on main",
            "What is the safest next action?",
        );

        assert_eq!(
            prompt,
            "[Background Agent] Git conflict detected on main\nQuestion: What is the safest next action?"
        );
    }
}
