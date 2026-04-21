use async_trait::async_trait;
#[cfg(feature = "notification")]
use crate::notification::NotificationManager;
#[cfg(feature = "telegram")]
use crate::telegram_supervisor::{
    start_telegram_supervisor, TelegramRuntimeHandle, TelegramStatusHandle, TelegramWorkerCommand,
};
#[cfg(feature = "voice")]
use crate::voice::{VoiceChannel, VoiceConfig};
use forja_core::gateway::adapter::{ChannelAdapter, CliAdapter};
use forja_core::traits::{
    NotificationLevel, NotificationState, NotificationTopic, TelegramConnectionStatus,
    VoiceChannelStatus,
};
use forja_core::{Channel, Content, Role, Message as CoreMessage};
#[cfg(feature = "telegram")]
use forja_core::gateway::adapter::TelegramAdapter;
use crate::cli::process_line;
use std::io::Write;
#[cfg(feature = "voice")]
use std::sync::Arc;
#[cfg(feature = "telegram")]
use std::sync::Mutex as StdMutex;
use tokio::sync::{mpsc, Mutex};

#[derive(Clone, Debug)]
pub enum ChannelSource {
    Cli,
    #[cfg(feature = "voice")]
    Voice,
    #[cfg(feature = "telegram")]
    Telegram { chat_id: i64 },
}

pub struct MultiChannel {
    receiver: Mutex<mpsc::Receiver<(ChannelSource, CoreMessage)>>,
    last_source: Mutex<Option<ChannelSource>>,
    #[cfg(feature = "telegram")]
    telegram_runtime: StdMutex<Option<TelegramRuntimeHandle>>,
    #[cfg(feature = "voice")]
    voice_channel: Option<Arc<VoiceChannel>>,
    #[cfg(feature = "notification")]
    notification_manager: NotificationManager,
    #[cfg(feature = "telegram")]
    telegram_status: TelegramStatusHandle,
    #[cfg(feature = "telegram")]
    notification_chat_ids: Vec<i64>,
}

impl MultiChannel {
    pub async fn new(
        bot_token: Option<String>,
        allowed_chat_ids: Vec<i64>,
        #[cfg(feature = "voice")] voice_config: Option<VoiceConfig>,
        #[cfg(feature = "notification")] notification_state: NotificationState,
    ) -> Self {
        let (tx, rx) = mpsc::channel::<(ChannelSource, CoreMessage)>(100);

        let tx_cli = tx.clone();
        tokio::spawn(async move {
            loop {
                let line = tokio::task::spawn_blocking(|| {
                    let mut buffer = String::new();
                    loop {
                        let mut input = String::new();
                        if std::io::stdin().read_line(&mut input).ok().unwrap_or(0) == 0 {
                            return String::new();
                        }
                        let trimmed = input.trim_end_matches(['\r', '\n']);
                        if process_line(trimmed, &mut buffer) {
                            print!("... ");
                            std::io::stdout().flush().ok();
                            continue;
                        }

                        return buffer;
                    }
                }).await.unwrap_or_default();

                if line.is_empty() { continue; }
                let msg = CoreMessage::text(Role::User, line, None);
                if tx_cli.send((ChannelSource::Cli, msg)).await.is_err() { break; }
            }
        });

        #[cfg(feature = "voice")]
        let voice_channel = voice_config.map(|config| Arc::new(VoiceChannel::new(config)));
        #[cfg(feature = "voice")]
        if let Some(voice_channel) = voice_channel.clone() {
            let tx_voice = tx.clone();
            tokio::spawn(async move {
                loop {
                    let message = match voice_channel.receive().await {
                        Ok(message) => message,
                        Err(_) => break,
                    };
                    if tx_voice.send((ChannelSource::Voice, message)).await.is_err() {
                        break;
                    }
                }
            });
        }

        #[cfg(feature = "telegram")]
        let (telegram_runtime, telegram_status) = if let Some(bot_token) = bot_token {
            let runtime_handle =
                start_telegram_supervisor(bot_token, allowed_chat_ids.clone(), tx.clone());
            let status = runtime_handle.status.clone();
            (Some(runtime_handle), status)
        } else {
            (
                None,
                TelegramStatusHandle::new(TelegramConnectionStatus::Disconnected),
            )
        };

        #[cfg(not(feature = "telegram"))]
        let _ = (bot_token, allowed_chat_ids);

        Self {
            receiver: Mutex::new(rx),
            last_source: Mutex::new(Some(ChannelSource::Cli)),
            #[cfg(feature = "telegram")]
            telegram_runtime: StdMutex::new(telegram_runtime),
            #[cfg(feature = "voice")]
            voice_channel,
            #[cfg(feature = "notification")]
            notification_manager: NotificationManager::new(notification_state),
            #[cfg(feature = "telegram")]
            telegram_status,
            #[cfg(feature = "telegram")]
            notification_chat_ids: allowed_chat_ids,
        }
    }

    /// CLI only (no Telegram)
    pub async fn new_cli_only() -> Self {
        Self::new(
            None,
            Vec::new(),
            #[cfg(feature = "voice")]
            None,
            #[cfg(feature = "notification")]
            NotificationState::default(),
        )
        .await
    }

    #[cfg(feature = "telegram")]
    pub async fn new_both(bot_token: String, allowed_chat_ids: Vec<i64>) -> Self {
        Self::new(
            Some(bot_token),
            allowed_chat_ids,
            #[cfg(feature = "voice")]
            None,
            #[cfg(feature = "notification")]
            NotificationState::default(),
        )
        .await
    }

    pub fn has_telegram(&self) -> bool {
        #[cfg(feature = "telegram")]
        {
            matches!(
                self.telegram_status(),
                Some(TelegramConnectionStatus::Connected | TelegramConnectionStatus::Reconnecting)
            )
        }

        #[cfg(not(feature = "telegram"))]
        {
            false
        }
    }

    #[cfg(feature = "telegram")]
    fn telegram_command_tx(&self) -> Option<tokio::sync::mpsc::UnboundedSender<TelegramWorkerCommand>> {
        self.telegram_runtime
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .as_ref()
            .map(|runtime| runtime.command_tx.clone())
    }

    pub fn telegram_status(&self) -> Option<TelegramConnectionStatus> {
        #[cfg(feature = "telegram")]
        {
            Some(self.telegram_status.snapshot())
        }

        #[cfg(not(feature = "telegram"))]
        {
            None
        }
    }

    #[cfg(feature = "telegram")]
    pub fn telegram_status_handle(
        &self,
    ) -> crate::telegram_supervisor::TelegramStatusHandle {
        self.telegram_status.clone()
    }

    fn shutdown_inner(&self) {
        #[cfg(feature = "voice")]
        if let Some(voice_channel) = &self.voice_channel {
            voice_channel.shutdown();
        }

        #[cfg(feature = "telegram")]
        {
            if let Some(runtime) = self
                .telegram_runtime
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .take()
            {
                self.telegram_status
                    .set(TelegramConnectionStatus::Disconnected);
                let _ = runtime.command_tx.send(TelegramWorkerCommand::Shutdown);
                let _ = runtime.thread_handle.join();
            }
        }
    }

    #[cfg(feature = "telegram")]
    #[allow(dead_code)]
    pub(crate) fn for_shutdown_test(
        command_tx: tokio::sync::mpsc::UnboundedSender<TelegramWorkerCommand>,
        thread_handle: std::thread::JoinHandle<()>,
    ) -> Self {
        let (_sender, receiver) = mpsc::channel(1);
        let telegram_status = TelegramStatusHandle::new(TelegramConnectionStatus::Connected);

        Self {
            receiver: Mutex::new(receiver),
            last_source: Mutex::new(None),
            telegram_runtime: StdMutex::new(Some(TelegramRuntimeHandle {
                command_tx,
                thread_handle,
                status: telegram_status.clone(),
            })),
            #[cfg(feature = "voice")]
            voice_channel: None,
            #[cfg(feature = "notification")]
            notification_manager: NotificationManager::new(NotificationState::default()),
            telegram_status,
            notification_chat_ids: Vec::new(),
        }
    }

    #[cfg(feature = "telegram")]
    #[allow(dead_code)]
    pub(crate) fn has_telegram_runtime_for_test(&self) -> bool {
        self.telegram_runtime
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .is_some()
    }

    #[cfg(all(feature = "telegram", test))]
    pub(crate) fn for_status_test(
        status: TelegramConnectionStatus,
        last_source: Option<ChannelSource>,
    ) -> Self {
        let (_sender, receiver) = mpsc::channel(1);
        let telegram_status = TelegramStatusHandle::new(status);
        Self {
            receiver: Mutex::new(receiver),
            last_source: Mutex::new(last_source),
            telegram_runtime: StdMutex::new(None),
            #[cfg(feature = "voice")]
            voice_channel: None,
            #[cfg(feature = "notification")]
            notification_manager: NotificationManager::new(NotificationState::default()),
            telegram_status,
            notification_chat_ids: Vec::new(),
        }
    }
}

#[async_trait]
impl Channel for MultiChannel {
    async fn receive(&self) -> forja_core::error::Result<CoreMessage> {
        let mut rx = self.receiver.lock().await;

        if let Some((source, msg)) = rx.recv().await {
            let mut last_src = self.last_source.lock().await;
            *last_src = Some(source.clone());

            #[cfg(feature = "voice")]
            if matches!(source, ChannelSource::Voice)
                && let Some(voice_channel) = &self.voice_channel
            {
                voice_channel.cancel_typing().await;
            }

            #[cfg(feature = "telegram")]
            if let ChannelSource::Telegram { chat_id } = source {
                if let Some(command_tx) = self.telegram_command_tx() {
                    let _ = command_tx.send(TelegramWorkerCommand::StartTyping { chat_id });
                }

                if let Content::Text { ref text, .. } = msg.content {
                    // Clear current line (prompt ">") and print
                    print!("\r\x1b[K");
                    println!("[TG] {}", text);
                }
            }

            let msg = match source {
                ChannelSource::Cli => {
                    let adapter = CliAdapter;
                    adapter.from_envelope(adapter.to_envelope(msg))
                }
                #[cfg(feature = "voice")]
                ChannelSource::Voice => msg,
                #[cfg(feature = "telegram")]
                ChannelSource::Telegram { .. } => {
                    let adapter = TelegramAdapter;
                    adapter.from_envelope(adapter.to_envelope(msg))
                }
            };

            Ok(msg)
        } else {
            Err(forja_core::error::ForjaError::ChannelError(
                "MultiChannel receiver closed unexpectedly".to_string(),
            ))
        }
    }

    async fn send(&self, message: CoreMessage) -> forja_core::error::Result<()> {
        self.cancel_typing().await;

        let last_src = self.last_source.lock().await.clone();

        if let Some(source) = last_src {
            let message = match source {
                ChannelSource::Cli => {
                    let adapter = CliAdapter;
                    adapter.from_envelope(adapter.to_envelope(message))
                }
                #[cfg(feature = "voice")]
                ChannelSource::Voice => message,
                #[cfg(feature = "telegram")]
                ChannelSource::Telegram { .. } => {
                    let adapter = TelegramAdapter;
                    adapter.from_envelope(adapter.to_envelope(message))
                }
            };

            if let Content::Text { text, .. } = &message.content {
                match source {
                    ChannelSource::Cli => {
                        let t = text.clone();
                        let _ = tokio::task::spawn_blocking(move || {
                            // Tool fallback: print response + restore prompt
                            println!("• {}", t);
                            print!("> ");
                            std::io::stdout().flush().ok();
                        }).await;
                    }
                    #[cfg(feature = "voice")]
                    ChannelSource::Voice => {
                        if let Some(voice_channel) = &self.voice_channel {
                            let _ = voice_channel
                                .send(CoreMessage::text(Role::Assistant, text.clone(), None))
                                .await;
                        }
                        let log_text = text.clone();
                        let _ = tokio::task::spawn_blocking(move || {
                            println!("• {}", log_text);
                            print!("> ");
                            std::io::stdout().flush().ok();
                        })
                        .await;
                    }
                    #[cfg(feature = "telegram")]
                    ChannelSource::Telegram { chat_id } => {
                        if !matches!(
                            self.telegram_status(),
                            Some(TelegramConnectionStatus::Connected)
                        ) {
                            let warning = "[WARN] Telegram unavailable; dropped outgoing Telegram message.".to_string();
                            let _ = tokio::task::spawn_blocking(move || {
                                println!("{warning}");
                                print!("> ");
                                std::io::stdout().flush().ok();
                            }).await;
                            return Ok(());
                        }

                        if let Some(command_tx) = self.telegram_command_tx() {
                            command_tx
                                .send(TelegramWorkerCommand::SendMessage {
                                    chat_id,
                                    text: text.clone(),
                                })
                                .map_err(|_| {
                                    forja_core::error::ForjaError::ChannelError(
                                        "Telegram worker command channel closed".to_string(),
                                    )
                                })?;

                            // Print • log to terminal
                            let log_text = text.clone();
                            let _ = tokio::task::spawn_blocking(move || {
                                println!("• {}", log_text);
                                print!("> ");
                                std::io::stdout().flush().ok();
                            }).await;
                        }
                    }
                }
            }
        } else {
            eprintln!("[WARN] MultiChannel send drop: Empty last_source");
        }

        Ok(())
    }

    fn is_cli_source(&self) -> bool {
        if let Ok(source) = self.last_source.try_lock() {
            matches!(*source, Some(ChannelSource::Cli))
        } else {
            false
        }
    }

    async fn cancel_typing(&self) {
        #[cfg(feature = "voice")]
        if let Some(voice_channel) = &self.voice_channel {
            voice_channel.cancel_typing().await;
        }

        #[cfg(feature = "telegram")]
        {
            if let Some(command_tx) = self.telegram_command_tx() {
                let _ = command_tx.send(TelegramWorkerCommand::StopTyping);
            }
        }
    }

    async fn log_line(&self, text: &str) {
        if self.is_cli_source() {
            let line = text.to_string();
            let _ = tokio::task::spawn_blocking(move || {
                print!("\r\x1b[K");
                println!("{line}");
                std::io::stdout().flush().ok();
            })
            .await;
        }
    }

    async fn send_notification(&self, text: &str) -> forja_core::error::Result<bool> {
        #[cfg(feature = "notification")]
        {
            let delivered = self.notification_manager.send(
                "Forja",
                text,
                NotificationTopic::Autonomy,
                NotificationLevel::Info,
            )?;
            if delivered {
                return Ok(true);
            }
        }

        #[cfg(feature = "telegram")]
        {
            if !matches!(
                self.telegram_status(),
                Some(TelegramConnectionStatus::Connected)
            ) {
                return Ok(false);
            }

            let Some(command_tx) = self.telegram_command_tx() else {
                return Ok(false);
            };

            let mut delivered = false;
            for chat_id in &self.notification_chat_ids {
                command_tx
                    .send(TelegramWorkerCommand::SendMessage {
                        chat_id: *chat_id,
                        text: text.to_string(),
                    })
                    .map_err(|_| {
                        forja_core::error::ForjaError::ChannelError(
                            "Telegram worker command channel closed".to_string(),
                        )
                    })?;
                delivered = true;
            }

            return Ok(delivered);
        }

        #[cfg(not(feature = "telegram"))]
        {
            let _ = text;
            Ok(false)
        }
    }

    async fn send_notification_with_level(
        &self,
        text: &str,
        _topic: NotificationTopic,
        _level: NotificationLevel,
    ) -> forja_core::error::Result<bool> {
        #[cfg(feature = "notification")]
        {
            let delivered = self
                .notification_manager
                .send("Forja", text, _topic, _level)?;
            if delivered {
                return Ok(true);
            }
        }

        self.send_notification(text).await
    }

    fn shutdown(&self) {
        self.shutdown_inner();
    }

    fn telegram_status(&self) -> Option<TelegramConnectionStatus> {
        #[cfg(feature = "telegram")]
        {
            Some(self.telegram_status.snapshot())
        }

        #[cfg(not(feature = "telegram"))]
        {
            None
        }
    }

    fn supports_voice(&self) -> bool {
        #[cfg(feature = "voice")]
        {
            self.voice_channel.is_some()
        }

        #[cfg(not(feature = "voice"))]
        {
            false
        }
    }

    fn voice_status(&self) -> Option<VoiceChannelStatus> {
        #[cfg(feature = "voice")]
        {
            self.voice_channel
                .as_ref()
                .and_then(|voice_channel| voice_channel.voice_status())
        }

        #[cfg(not(feature = "voice"))]
        {
            None
        }
    }

    async fn set_voice_enabled(&self, enabled: bool) -> forja_core::error::Result<VoiceChannelStatus> {
        #[cfg(feature = "voice")]
        {
            if let Some(voice_channel) = &self.voice_channel {
                return voice_channel.set_voice_enabled(enabled).await;
            }
            return Ok(VoiceChannelStatus::Unavailable);
        }

        #[cfg(not(feature = "voice"))]
        {
            let _ = enabled;
            Ok(VoiceChannelStatus::Unavailable)
        }
    }

    fn notification_state(&self) -> Option<NotificationState> {
        #[cfg(feature = "notification")]
        {
            Some(self.notification_manager.state())
        }

        #[cfg(not(feature = "notification"))]
        {
            None
        }
    }

    async fn set_notifications_enabled(&self, enabled: bool) -> forja_core::error::Result<NotificationState> {
        #[cfg(feature = "notification")]
        {
            return Ok(self.notification_manager.set_enabled(enabled));
        }

        #[cfg(not(feature = "notification"))]
        {
            let _ = enabled;
            Ok(NotificationState::default())
        }
    }
}

impl Drop for MultiChannel {
    fn drop(&mut self) {
        self.shutdown_inner();
    }
}
