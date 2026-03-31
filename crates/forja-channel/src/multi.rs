use async_trait::async_trait;
use forja_core::gateway::adapter::{ChannelAdapter, CliAdapter, TelegramAdapter};
use forja_core::{Channel, Content, Role, Message as CoreMessage};
#[cfg(feature = "telegram")]
use reqwest::Client;
use crate::cli::process_line;
use std::io::Write;
use std::sync::Mutex as StdMutex;
#[cfg(feature = "telegram")]
use std::time::Duration;
use tokio::sync::{mpsc, Mutex};

#[cfg(feature = "telegram")]
use teloxide::dispatching::{Dispatcher, ShutdownToken, UpdateFilterExt};
#[cfg(feature = "telegram")]
use teloxide::prelude::*;

#[derive(Clone, Debug)]
pub enum ChannelSource {
    Cli,
    #[cfg(feature = "telegram")]
    Telegram { chat_id: i64 },
}

pub struct MultiChannel {
    receiver: Mutex<mpsc::Receiver<(ChannelSource, CoreMessage)>>,
    last_source: Mutex<Option<ChannelSource>>,
    #[cfg(feature = "telegram")]
    telegram_bot: Option<Bot>,
    #[cfg(feature = "telegram")]
    typing_handle: StdMutex<Option<tokio::task::JoinHandle<()>>>,
    #[cfg(feature = "telegram")]
    shutdown_token: StdMutex<Option<ShutdownToken>>,
}

impl MultiChannel {
    /// CLI only (no Telegram)
    pub async fn new_cli_only() -> Self {
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

        Self {
            receiver: Mutex::new(rx),
            last_source: Mutex::new(Some(ChannelSource::Cli)),
            #[cfg(feature = "telegram")]
            telegram_bot: None,
            #[cfg(feature = "telegram")]
            typing_handle: StdMutex::new(None),
            #[cfg(feature = "telegram")]
            shutdown_token: StdMutex::new(None),
        }
    }

    #[cfg(feature = "telegram")]
    pub async fn new_both(
        bot_token: String,
        allowed_chat_ids: Vec<i64>,
    ) -> forja_core::error::Result<Self> {
        let (tx, rx) = mpsc::channel::<(ChannelSource, CoreMessage)>(100);
        let client = Client::builder()
            .connect_timeout(Duration::from_secs(30))
            .timeout(Duration::from_secs(60))
            .build()
            .map_err(|error| {
                forja_core::error::ForjaError::ChannelError(format!(
                    "Failed to build reqwest client: {}",
                    error
                ))
            })?;
        let bot = Bot::with_client(bot_token, client);
        match tokio::time::timeout(Duration::from_secs(60), bot.get_me()).await {
            Ok(Ok(_)) => {}
            Ok(Err(error)) => {
                return Err(forja_core::error::ForjaError::ChannelError(format!(
                    "Telegram getMe failed: {}",
                    error
                )))
            }
            Err(_) => {
                return Err(forja_core::error::ForjaError::ChannelError(
                    "Telegram getMe timed out after 60 seconds".to_string(),
                ))
            }
        }
        
        let tx_tg = tx.clone();
        let allowed = allowed_chat_ids.clone();
        
        // Telegram dispatcher setup
        let handler = teloxide::types::Update::filter_message().endpoint(
            move |msg: teloxide::types::Message, bot: Bot, tx_tg: mpsc::Sender<(ChannelSource, CoreMessage)>| {
                let allowed = allowed.clone();
                async move {
                    let chat_id = msg.chat.id.0;
                    if !allowed.contains(&chat_id) {
                        let _ = bot.send_message(msg.chat.id, "[DENIED] Authorized users only.").await;
                        return Ok::<(), teloxide::RequestError>(());
                    }
                    if let Some(text) = msg.text() {
                        let core_msg = CoreMessage::text(Role::User, text.to_string(), None);
                        let _ = tx_tg.send((ChannelSource::Telegram { chat_id }, core_msg)).await;
                    }
                    Ok::<(), teloxide::RequestError>(())
                }
            }
        );

        let bot_dispatcher = bot.clone();
        let mut dispatcher = Dispatcher::builder(bot_dispatcher, handler)
            .dependencies(teloxide::dptree::deps![tx_tg])
            .build();
        let shutdown_token = dispatcher.shutdown_token();
        tokio::spawn(async move {
            dispatcher.dispatch().await;
        });

        // CLI stdin spawn
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
                })
                .await
                .unwrap_or_default();

                if line.is_empty() {
                    continue;
                }
                
                let core_msg = CoreMessage::text(Role::User, line, None);
                if tx_cli.send((ChannelSource::Cli, core_msg)).await.is_err() {
                    break;
                }
            }
        });

        Ok(Self {
            receiver: Mutex::new(rx),
            last_source: Mutex::new(None),
            telegram_bot: Some(bot),
            typing_handle: StdMutex::new(None),
            shutdown_token: StdMutex::new(Some(shutdown_token)),
        })
    }

    fn shutdown_inner(&self) {
        #[cfg(feature = "telegram")]
        {
            if let Some(handle) = self
                .typing_handle
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .take()
            {
                handle.abort();
            }

            if let Some(token) = self
                .shutdown_token
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .take()
            {
                let _ = token.shutdown();
            }
        }
    }

    #[cfg(feature = "telegram")]
    #[allow(dead_code)]
    pub(crate) fn for_shutdown_test(
        shutdown_token: ShutdownToken,
        typing_handle: tokio::task::JoinHandle<()>,
    ) -> Self {
        let (_sender, receiver) = mpsc::channel(1);

        Self {
            receiver: Mutex::new(receiver),
            last_source: Mutex::new(None),
            telegram_bot: Some(Bot::new("test-token")),
            typing_handle: StdMutex::new(Some(typing_handle)),
            shutdown_token: StdMutex::new(Some(shutdown_token)),
        }
    }

    #[cfg(feature = "telegram")]
    #[allow(dead_code)]
    pub(crate) fn has_shutdown_token_for_test(&self) -> bool {
        self.shutdown_token
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .is_some()
    }
}

#[async_trait]
impl Channel for MultiChannel {
    async fn receive(&self) -> forja_core::error::Result<CoreMessage> {
        let mut rx = self.receiver.lock().await;

        if let Some((source, msg)) = rx.recv().await {
            let mut last_src = self.last_source.lock().await;
            *last_src = Some(source.clone());

            #[cfg(feature = "telegram")]
            if let ChannelSource::Telegram { chat_id } = source {
                if let Some(bot) = &self.telegram_bot {
                    let bot_clone = bot.clone();
                    let handle = tokio::spawn(async move {
                        loop {
                            let _ = bot_clone
                                .send_chat_action(
                                    teloxide::types::ChatId(chat_id),
                                    teloxide::types::ChatAction::Typing,
                                )
                                .await;
                            tokio::time::sleep(tokio::time::Duration::from_secs(4)).await;
                        }
                    });
                    *self
                        .typing_handle
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(handle);
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
                            println!("● {}", t);
                            print!("> ");
                            std::io::stdout().flush().ok();
                        }).await;
                    }
                    #[cfg(feature = "telegram")]
                    ChannelSource::Telegram { chat_id } => {
                        if let Some(bot) = &self.telegram_bot {
                            bot.send_message(teloxide::types::ChatId(chat_id), text.clone())
                                .await
                                .map_err(|e| {
                                    forja_core::error::ForjaError::ChannelError(format!(
                                        "Failed to send Telegram message: {}",
                                        e
                                    ))
                                })?;
                            
                            // Print ● log to terminal
                            let log_text = text.clone();
                            let _ = tokio::task::spawn_blocking(move || {
                                println!("● {}", log_text);
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
        #[cfg(feature = "telegram")]
        {
            if let Some(handle) = self
                .typing_handle
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .take()
            {
                handle.abort();
            }
        }
    }

    fn shutdown(&self) {
        self.shutdown_inner();
    }
}

impl Drop for MultiChannel {
    fn drop(&mut self) {
        self.shutdown_inner();
    }
}
