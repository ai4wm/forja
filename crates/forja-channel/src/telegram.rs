#[cfg(feature = "telegram")]
use async_trait::async_trait;
#[cfg(feature = "telegram")]
use forja_core::gateway::adapter::{ChannelAdapter, TelegramAdapter};
#[cfg(feature = "telegram")]
use forja_core::{Channel, Content, Message as CoreMessage, Role};
#[cfg(feature = "telegram")]
use reqwest::Client;
#[cfg(feature = "telegram")]
use std::sync::Mutex as StdMutex;
#[cfg(feature = "telegram")]
use std::time::Duration;
#[cfg(feature = "telegram")]
use teloxide::dispatching::{Dispatcher, ShutdownToken, UpdateFilterExt};
#[cfg(feature = "telegram")]
use teloxide::types::Update;
#[cfg(feature = "telegram")]
use teloxide::{RequestError, prelude::*};
#[cfg(feature = "telegram")]
use tokio::sync::{Mutex, mpsc};

/// Core interface for the Telegram bot channel.
#[cfg(feature = "telegram")]
pub struct TelegramChannel {
    bot: Bot,
    receiver: Mutex<mpsc::Receiver<(i64, CoreMessage)>>,
    last_chat_id: Mutex<Option<i64>>,
    #[allow(dead_code)]
    allowed_chat_ids: Vec<i64>,
    typing_handle: StdMutex<Option<tokio::task::JoinHandle<()>>>,
    shutdown_token: StdMutex<Option<ShutdownToken>>,
}

#[cfg(feature = "telegram")]
impl TelegramChannel {
    /// Constructor. Starts bot long-polling in a background task.
    pub async fn new(
        bot_token: String,
        allowed_chat_ids: Vec<i64>,
    ) -> forja_core::error::Result<Self> {
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
                )));
            }
            Err(_) => {
                return Err(forja_core::error::ForjaError::ChannelError(
                    "Telegram getMe timed out after 60 seconds".to_string(),
                ));
            }
        }
        // Channel buffer size 100 (can be optimized)
        let (tx, rx) = mpsc::channel::<(i64, CoreMessage)>(100);

        // Clone allowed ID list
        let allowed_cloned = allowed_chat_ids.clone();

        // Configure Telegram dispatcher
        let handler = Update::filter_message().endpoint(
            move |msg: teloxide::types::Message, bot: Bot, tx: mpsc::Sender<(i64, CoreMessage)>| {
                let allowed = allowed_cloned.clone();
                async move {
                    let chat_id = msg.chat.id.0; // Extract i64 chat ID

                    if !allowed.contains(&chat_id) {
                        // Block access outside whitelist: send notice and discard
                        let _ = bot
                            .send_message(msg.chat.id, "[DENIED] Authorized users only.")
                            .await;
                        return Ok::<(), RequestError>(());
                    }

                    if let Some(text) = msg.text() {
                        let adapter = TelegramAdapter;
                        let raw = CoreMessage::text(Role::User, text.to_string(), None);
                        let core_msg = adapter.from_envelope(adapter.to_envelope(raw));

                        // Send failure catch (ignored for now)
                        let _ = tx.send((chat_id, core_msg)).await;
                    }

                    Ok::<(), RequestError>(())
                }
            },
        );

        let bot_clone = bot.clone();
        let mut dispatcher = Dispatcher::builder(bot_clone, handler)
            .dependencies(dptree::deps![tx])
            .build();
        let shutdown_token = dispatcher.shutdown_token();

        // Start async bot receive handler in background task
        tokio::spawn(async move {
            dispatcher.dispatch().await;
        });

        Ok(Self {
            bot,
            receiver: Mutex::new(rx),
            last_chat_id: Mutex::new(None),
            allowed_chat_ids,
            typing_handle: StdMutex::new(None),
            shutdown_token: StdMutex::new(Some(shutdown_token)),
        })
    }

    fn shutdown_inner(&self) {
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

    #[cfg(test)]
    #[allow(dead_code)]
    pub(crate) fn for_shutdown_test(
        shutdown_token: ShutdownToken,
        typing_handle: tokio::task::JoinHandle<()>,
    ) -> Self {
        let (_sender, receiver) = mpsc::channel(1);

        Self {
            bot: Bot::new("test-token"),
            receiver: Mutex::new(receiver),
            last_chat_id: Mutex::new(None),
            allowed_chat_ids: Vec::new(),
            typing_handle: StdMutex::new(Some(typing_handle)),
            shutdown_token: StdMutex::new(Some(shutdown_token)),
        }
    }

    #[cfg(test)]
    #[allow(dead_code)]
    pub(crate) fn has_shutdown_token_for_test(&self) -> bool {
        self.shutdown_token
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .is_some()
    }
}

#[cfg(feature = "telegram")]
#[async_trait]
impl Channel for TelegramChannel {
    async fn receive(&self) -> forja_core::error::Result<CoreMessage> {
        let mut rx = self.receiver.lock().await;

        // Wait for incoming messages from mpsc::Receiver
        if let Some((chat_id, msg)) = rx.recv().await {
            let mut last_id = self.last_chat_id.lock().await;
            *last_id = Some(chat_id);

            // Start typing indicator background loop
            let bot_clone = self.bot.clone();
            let tid = chat_id;
            let handle = tokio::spawn(async move {
                loop {
                    let _ = bot_clone
                        .send_chat_action(
                            teloxide::types::ChatId(tid),
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

            // Print receive log to terminal
            if let Content::Text { ref text, .. } = msg.content {
                print!("\r\x1b[K");
                println!("[TG] {}", text);
            }
            Ok(msg)
        } else {
            Err(forja_core::error::ForjaError::ChannelError(
                "Telegram receiver channel closed unexpectedly".to_string(),
            ))
        }
    }

    async fn send(&self, message: CoreMessage) -> forja_core::error::Result<()> {
        let adapter = TelegramAdapter;
        let message = adapter.from_envelope(adapter.to_envelope(message));
        // Stop typing action when starting to send
        if let Some(handle) = self
            .typing_handle
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .take()
        {
            handle.abort();
        }

        let last_id = *self.last_chat_id.lock().await;

        if let Some(chat_id) = last_id {
            if let Content::Text { text, .. } = &message.content {
                let send_res = self
                    .bot
                    .send_message(teloxide::types::ChatId(chat_id), text.to_string())
                    .parse_mode(teloxide::types::ParseMode::MarkdownV2)
                    .await;

                if send_res.is_err() {
                    // Fallback to plain text if markdown parsing fails
                    self.bot
                        .send_message(teloxide::types::ChatId(chat_id), text.to_string())
                        .await
                        .map_err(|e| {
                            forja_core::error::ForjaError::ChannelError(format!(
                                "Failed to send Telegram message: {}",
                                e
                            ))
                        })?;
                }

                // Print send log to terminal
                let log_text = text.clone();
                let _ = tokio::task::spawn_blocking(move || {
                    use std::io::Write;
                    println!("• {}", log_text);
                    print!("> ");
                    std::io::stdout().flush().ok();
                })
                .await;
            }
        } else {
            // Skip or log warning if no target chat exists yet
            eprintln!("[WARN] Telegram send drop: Empty last_chat_id");
        }

        Ok(())
    }

    fn shutdown(&self) {
        self.shutdown_inner();
    }

    fn active_channel_name(&self) -> Option<&'static str> {
        Some("telegram")
    }
}

#[cfg(feature = "telegram")]
impl Drop for TelegramChannel {
    fn drop(&mut self) {
        self.shutdown_inner();
    }
}

#[cfg(feature = "telegram")]
#[allow(dead_code)]
fn escape_markdown_v2(text: &str) -> String {
    // MarkdownV2 escape characters. Using HTML mode for simpler escaping.
    text.to_string()
}
