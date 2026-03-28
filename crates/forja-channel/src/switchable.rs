use async_trait::async_trait;
use forja_core::error::Result;
use forja_core::traits::Channel;
use forja_core::types::Message;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

#[cfg(feature = "tui")]
use forja_core::types::Content;
#[cfg(feature = "tui")]
use crate::tui_channel::{AgentStatusInfo, TUI_EXIT_SENTINEL, TuiChannel};
#[cfg(feature = "tui")]
use std::collections::VecDeque;
#[cfg(feature = "tui")]
use std::sync::Mutex;
#[cfg(feature = "tui")]
use forja_core::notification::Notification;

pub struct SwitchableChannel {
    base: Arc<dyn Channel>,
    #[cfg(feature = "tui")]
    has_remote_sources: bool,
    #[cfg(feature = "tui")]
    tui: Arc<TuiChannel>,
    tui_active: AtomicBool,
}

impl SwitchableChannel {
    #[cfg(feature = "tui")]
    pub fn new(
        base: Arc<dyn Channel>,
        has_remote_sources: bool,
        messages: Arc<Mutex<crate::tui_channel::DisplayBuffer>>,
        notifications: Arc<Mutex<VecDeque<Notification>>>,
        agent_status: Arc<Mutex<AgentStatusInfo>>,
    ) -> Self {
        Self {
            base,
            #[cfg(feature = "tui")]
            has_remote_sources,
            tui: Arc::new(TuiChannel::with_shared_state(messages, notifications, agent_status)),
            tui_active: AtomicBool::new(false),
        }
    }

    #[cfg(not(feature = "tui"))]
    pub fn new(base: Arc<dyn Channel>, has_remote_sources: bool) -> Self {
        let _ = has_remote_sources;
        Self {
            base,
            tui_active: AtomicBool::new(false),
        }
    }

    #[cfg(feature = "tui")]
    pub fn enter_tui(&self) {
        self.tui.start();
        self.tui_active.store(true, Ordering::SeqCst);
    }

    #[cfg(feature = "tui")]
    pub fn exit_tui(&self) {
        self.tui.stop();
        self.tui_active.store(false, Ordering::SeqCst);
    }

    #[cfg(feature = "tui")]
    pub fn notifications_handle(&self) -> Arc<Mutex<VecDeque<Notification>>> {
        self.tui.notifications_handle()
    }

    #[cfg(feature = "tui")]
    pub fn agent_status_handle(&self) -> Arc<Mutex<AgentStatusInfo>> {
        self.tui.agent_status_handle()
    }

    pub fn is_tui_active(&self) -> bool {
        self.tui_active.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl Channel for SwitchableChannel {
    async fn receive(&self) -> Result<Message> {
        #[cfg(feature = "tui")]
        {
            loop {
                if self.is_tui_active() {
                    if self.has_remote_sources {
                        tokio::select! {
                            result = self.tui.receive() => {
                                let message = result?;
                                if is_tui_exit_message(&message) {
                                    self.exit_tui();
                                    continue;
                                }
                                return Ok(message);
                            }
                            result = self.base.receive() => {
                                let message = result?;
                                let _ = self.tui.send(message.clone()).await;
                                return Ok(message);
                            }
                        }
                    } else {
                        let message = self.tui.receive().await?;
                        if is_tui_exit_message(&message) {
                            self.exit_tui();
                            continue;
                        }
                        return Ok(message);
                    }
                }

                let message = self.base.receive().await?;
                let _ = self.tui.send(message.clone()).await;
                return Ok(message);
            }
        }

        #[cfg(not(feature = "tui"))]
        {
            self.base.receive().await
        }
    }

    async fn send(&self, message: Message) -> Result<()> {
        #[cfg(feature = "tui")]
        {
            let _ = self.tui.send(message.clone()).await;
            if self.is_tui_active() && (!self.has_remote_sources || self.base.is_cli_source()) {
                return Ok(());
            }
        }

        self.base.send(message).await
    }

    async fn confirm(&self, message: &str) -> Result<bool> {
        #[cfg(feature = "tui")]
        if self.is_tui_active() {
            return self.tui.confirm(message).await;
        }

        self.base.confirm(message).await
    }

    fn is_cli_source(&self) -> bool {
        #[cfg(feature = "tui")]
        if self.is_tui_active() {
            return true;
        }

        self.base.is_cli_source()
    }

    async fn cancel_typing(&self) {
        self.base.cancel_typing().await;
    }
}

#[cfg(feature = "tui")]
fn is_tui_exit_message(message: &Message) -> bool {
    matches!(
        &message.content,
        Content::Text { text, .. } if text == TUI_EXIT_SENTINEL
    )
}
