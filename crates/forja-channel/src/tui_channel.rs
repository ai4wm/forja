#[cfg(feature = "tui")]
use async_trait::async_trait;
#[cfg(feature = "tui")]
use crossterm::event::{self, Event};
#[cfg(feature = "tui")]
use crossterm::execute;
#[cfg(feature = "tui")]
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
#[cfg(feature = "tui")]
use forja_core::error::{ForjaError, Result};
#[cfg(feature = "tui")]
use forja_core::notification::{Notification, Notifier};
#[cfg(feature = "tui")]
use forja_core::traits::Channel;
#[cfg(feature = "tui")]
use forja_core::types::{Content, Message, Role};
#[cfg(feature = "tui")]
use ratatui::Terminal;
#[cfg(feature = "tui")]
use ratatui::backend::CrosstermBackend;
#[cfg(feature = "tui")]
use std::collections::VecDeque;
#[cfg(feature = "tui")]
use std::io;
#[cfg(feature = "tui")]
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(feature = "tui")]
use std::sync::{Arc, Mutex};
#[cfg(feature = "tui")]
use std::thread;
#[cfg(feature = "tui")]
use std::time::Duration;
#[cfg(feature = "tui")]
use tokio::sync::{mpsc, oneshot};

#[cfg(feature = "tui")]
pub const TUI_EXIT_SENTINEL: &str = "/__exit_tui__";

#[cfg(feature = "tui")]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DisplayMessage {
    pub role: String,
    pub text: String,
}

#[cfg(feature = "tui")]
#[derive(Debug, Clone, Default)]
pub struct DisplayBuffer {
    pub messages: Vec<DisplayMessage>,
    pub scroll_offset: usize,
}

#[cfg(feature = "tui")]
impl DisplayBuffer {
    pub fn push(&mut self, message: DisplayMessage) {
        self.messages.push(message);
    }

    pub fn scroll_up(&mut self) {
        self.scroll_offset = self.scroll_offset.saturating_sub(1);
    }

    pub fn scroll_down(&mut self) {
        self.scroll_offset = self.scroll_offset.saturating_add(1);
    }

    pub fn clear(&mut self) {
        self.messages.clear();
        self.scroll_offset = 0;
    }
}

#[cfg(feature = "tui")]
#[derive(Debug, Clone, Default)]
pub struct AgentStatusInfo {
    pub exec_mode: String,
    pub model_name: String,
    pub think_level: String,
    pub role: String,
    pub background_running: bool,
    pub background_paused: bool,
    pub memory_entry_count: usize,
    pub uptime_seconds: u64,
    pub last_event: Option<String>,
}

#[cfg(feature = "tui")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusPane {
    Conversation,
    Notifications,
    Input,
}

#[cfg(feature = "tui")]
pub(crate) struct PendingConfirm {
    pub(crate) prompt: String,
    pub(crate) responder: Option<oneshot::Sender<bool>>,
}

#[cfg(feature = "tui")]
pub(crate) struct TuiState {
    pub(crate) messages: Arc<Mutex<DisplayBuffer>>,
    pub(crate) notifications: Arc<Mutex<VecDeque<Notification>>>,
    pub(crate) agent_status: Arc<Mutex<AgentStatusInfo>>,
    pub(crate) input: String,
    pub(crate) focus: FocusPane,
    pub(crate) help_overlay: bool,
    pub(crate) pending_confirm: Option<PendingConfirm>,
}

#[cfg(feature = "tui")]
pub struct TuiChannel {
    pub messages: Arc<Mutex<DisplayBuffer>>,
    pub notifications: Arc<Mutex<VecDeque<Notification>>>,
    pub agent_status: Arc<Mutex<AgentStatusInfo>>,
    input_tx: mpsc::UnboundedSender<Message>,
    input_rx: tokio::sync::Mutex<mpsc::UnboundedReceiver<Message>>,
    state: Arc<Mutex<TuiState>>,
    running: Arc<AtomicBool>,
    render_handle: Mutex<Option<thread::JoinHandle<()>>>,
}

#[cfg(feature = "tui")]
impl TuiChannel {
    pub fn new() -> Self {
        let messages = Arc::new(Mutex::new(DisplayBuffer::default()));
        let notifications = Arc::new(Mutex::new(VecDeque::new()));
        let agent_status = Arc::new(Mutex::new(AgentStatusInfo::default()));
        Self::with_shared_state(messages, notifications, agent_status)
    }

    pub fn with_shared_state(
        messages: Arc<Mutex<DisplayBuffer>>,
        notifications: Arc<Mutex<VecDeque<Notification>>>,
        agent_status: Arc<Mutex<AgentStatusInfo>>,
    ) -> Self {
        let (input_tx, input_rx) = mpsc::unbounded_channel();
        let state = Arc::new(Mutex::new(TuiState {
            messages: messages.clone(),
            notifications: notifications.clone(),
            agent_status: agent_status.clone(),
            input: String::new(),
            focus: FocusPane::Input,
            help_overlay: false,
            pending_confirm: None,
        }));
        Self {
            messages,
            notifications,
            agent_status,
            input_tx,
            input_rx: tokio::sync::Mutex::new(input_rx),
            state,
            running: Arc::new(AtomicBool::new(false)),
            render_handle: Mutex::new(None),
        }
    }

    pub fn start(&self) {
        if self.running.swap(true, Ordering::SeqCst) {
            return;
        }

        let state = self.state.clone();
        let input_tx = self.input_tx.clone();
        let running = self.running.clone();

        let handle = thread::spawn(move || {
            if enable_raw_mode().is_err() {
                running.store(false, Ordering::SeqCst);
                return;
            }

            let mut stdout = io::stdout();
            if execute!(stdout, EnterAlternateScreen).is_err() {
                let _ = disable_raw_mode();
                running.store(false, Ordering::SeqCst);
                return;
            }

            let backend = CrosstermBackend::new(stdout);
            let mut terminal = match Terminal::new(backend) {
                Ok(terminal) => terminal,
                Err(_) => {
                    let _ = disable_raw_mode();
                    running.store(false, Ordering::SeqCst);
                    return;
                }
            };

            while running.load(Ordering::SeqCst) {
                let _ = terminal.draw(|frame| crate::tui_layout::render(frame, &state));

                if event::poll(Duration::from_millis(66)).unwrap_or(false)
                    && let Ok(Event::Key(key)) = event::read()
                {
                    let should_exit = crate::tui_input::handle_key_event(
                        key,
                        &state,
                        &input_tx,
                        &running,
                    );
                    if should_exit {
                        let _ = input_tx.send(Message::text(Role::User, TUI_EXIT_SENTINEL, None));
                        break;
                    }
                }
            }

            let _ = disable_raw_mode();
            let mut stdout = io::stdout();
            let _ = execute!(stdout, LeaveAlternateScreen);
            running.store(false, Ordering::SeqCst);
        });

        if let Ok(mut render_handle) = self.render_handle.lock() {
            *render_handle = Some(handle);
        }
    }

    pub fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);
        if let Ok(mut render_handle) = self.render_handle.lock()
            && let Some(handle) = render_handle.take()
        {
            let _ = handle.join();
        }
    }

    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    pub fn notifications_handle(&self) -> Arc<Mutex<VecDeque<Notification>>> {
        self.notifications.clone()
    }

    pub fn agent_status_handle(&self) -> Arc<Mutex<AgentStatusInfo>> {
        self.agent_status.clone()
    }
}

#[cfg(feature = "tui")]
impl Default for TuiChannel {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(feature = "tui")]
#[async_trait]
impl Channel for TuiChannel {
    async fn receive(&self) -> Result<Message> {
        self.start();
        self.input_rx
            .lock()
            .await
            .recv()
            .await
            .ok_or_else(|| ForjaError::ChannelError("TUI input channel closed".to_string()))
    }

    async fn send(&self, message: Message) -> Result<()> {
        let display = match message.role {
            Role::User => Some(DisplayMessage {
                role: "User".to_string(),
                text: match &message.content {
                    Content::Text { text, .. } => text.clone(),
                    _ => String::new(),
                },
            }),
            Role::Assistant => Some(DisplayMessage {
                role: "Forja".to_string(),
                text: match &message.content {
                    Content::Text { text, .. } => text.clone(),
                    _ => String::new(),
                },
            }),
            Role::System => Some(DisplayMessage {
                role: "[Agent]".to_string(),
                text: match &message.content {
                    Content::Text { text, .. } => text.clone(),
                    _ => String::new(),
                },
            }),
            Role::Tool => None,
        };

        if let Some(display) = display
            && let Ok(mut buffer) = self.messages.lock()
        {
            buffer.push(display);
        }

        Ok(())
    }

    async fn confirm(&self, message: &str) -> Result<bool> {
        self.start();
        let (tx, rx) = oneshot::channel();
        if let Ok(mut state) = self.state.lock() {
            state.pending_confirm = Some(PendingConfirm {
                prompt: message.to_string(),
                responder: Some(tx),
            });
        }

        rx.await
            .map_err(|error| ForjaError::ChannelError(format!("TUI confirmation failed: {error}")))
    }

    fn is_cli_source(&self) -> bool {
        true
    }
}

#[cfg(feature = "tui")]
pub struct TuiNotificationBridge {
    notifications: Arc<Mutex<VecDeque<Notification>>>,
}

#[cfg(feature = "tui")]
impl TuiNotificationBridge {
    pub fn new(notifications: Arc<Mutex<VecDeque<Notification>>>) -> Self {
        Self { notifications }
    }
}

#[cfg(feature = "tui")]
impl Notifier for TuiNotificationBridge {
    fn notify(&self, notification: &Notification) -> Result<()> {
        if let Ok(mut notifications) = self.notifications.lock() {
            notifications.push_back(notification.clone());
            while notifications.len() > 50 {
                let _ = notifications.pop_front();
            }
        }
        Ok(())
    }

    fn is_available(&self) -> bool {
        true
    }
}

#[cfg(feature = "tui")]
#[cfg(test)]
mod tests {
    use super::{AgentStatusInfo, Channel, DisplayBuffer, DisplayMessage, TuiChannel};

    fn assert_channel_impl<T: Channel>() {}

    #[test]
    fn agent_status_info_fields_are_accessible() {
        let status = AgentStatusInfo {
            exec_mode: "auto".to_string(),
            model_name: "gpt-5.4".to_string(),
            think_level: "mid".to_string(),
            role: "assistant".to_string(),
            background_running: true,
            background_paused: false,
            memory_entry_count: 42,
            uptime_seconds: 10,
            last_event: Some("Test failed".to_string()),
        };

        assert_eq!(status.exec_mode, "auto");
        assert_eq!(status.memory_entry_count, 42);
        assert_eq!(status.last_event.as_deref(), Some("Test failed"));
    }

    #[test]
    fn display_buffer_push_scroll_and_clear() {
        let mut buffer = DisplayBuffer::default();
        buffer.push(DisplayMessage {
            role: "User".to_string(),
            text: "hello".to_string(),
        });
        buffer.scroll_down();
        buffer.scroll_up();
        buffer.clear();

        assert!(buffer.messages.is_empty());
        assert_eq!(buffer.scroll_offset, 0);
    }

    #[test]
    fn tui_channel_implements_channel_trait() {
        assert_channel_impl::<TuiChannel>();
    }
}
