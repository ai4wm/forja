#[cfg(feature = "telegram")]
use async_trait::async_trait;
#[cfg(feature = "telegram")]
use forja_core::{Channel, Content, Message as CoreMessage, Role};
#[cfg(feature = "telegram")]
use teloxide::{prelude::*, RequestError};
#[cfg(feature = "telegram")]
use tokio::sync::{mpsc, Mutex};

#[cfg(feature = "telegram")]
use teloxide::dispatching::UpdateFilterExt;
#[cfg(feature = "telegram")]
use teloxide::types::Update;

/// Core interface for the Telegram bot channel.
#[cfg(feature = "telegram")]
pub struct TelegramChannel {
    bot: Bot,
    receiver: Mutex<mpsc::Receiver<(i64, CoreMessage)>>,
    last_chat_id: Mutex<Option<i64>>,
    #[allow(dead_code)]
    allowed_chat_ids: Vec<i64>,
    typing_handle: Mutex<Option<tokio::task::JoinHandle<()>>>,
}

#[cfg(feature = "telegram")]
impl TelegramChannel {
    /// Constructor. Starts bot long-polling in a background task.
    pub async fn new(bot_token: String, allowed_chat_ids: Vec<i64>) -> Self {
        let bot = Bot::new(bot_token);
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
                        let _ = bot.send_message(
                            msg.chat.id, 
                            "[DENIED] Authorized users only."
                        ).await;
                        return Ok::<(), RequestError>(());
                    }

                    if let Some(text) = msg.text() {
                        let core_msg = CoreMessage::text(Role::User, text.to_string(), None);
                        
                        // Send failure catch (ignored for now)
                        let _ = tx.send((chat_id, core_msg)).await;
                    }

                    Ok::<(), RequestError>(())
                }
            },
        );

        let bot_clone = bot.clone();
        
        // Start async bot receive handler in background task
        tokio::spawn(async move {
            Dispatcher::builder(bot_clone, handler)
                .dependencies(dptree::deps![tx])
                .enable_ctrlc_handler()
                .build()
                .dispatch()
                .await;
        });

        Self {
            bot,
            receiver: Mutex::new(rx),
            last_chat_id: Mutex::new(None),
            allowed_chat_ids,
            typing_handle: Mutex::new(None),
        }
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
                        .send_chat_action(teloxide::types::ChatId(tid), teloxide::types::ChatAction::Typing)
                        .await;
                    tokio::time::sleep(tokio::time::Duration::from_secs(4)).await;
                }
            });
            *self.typing_handle.lock().await = Some(handle);

            // Print receive log to terminal
            if let Content::Text { ref text, .. } = msg.content {
                print!("\r\x1b[K");
                println!("[TG] {}", text);
            }
            Ok(msg)
        } else {
            Err(forja_core::error::ForjaError::ChannelError(
                "Telegram receiver channel closed unexpectedly".to_string()
            ))
        }
    }

    async fn send(&self, message: CoreMessage) -> forja_core::error::Result<()> {
        // Stop typing action when starting to send
        if let Some(handle) = self.typing_handle.lock().await.take() {
            handle.abort();
        }

        let last_id = *self.last_chat_id.lock().await;

        if let Some(chat_id) = last_id {
            if let Content::Text { text, .. } = &message.content {
                let send_res = self.bot
                    .send_message(teloxide::types::ChatId(chat_id), text.to_string())
                    .parse_mode(teloxide::types::ParseMode::MarkdownV2)
                    .await;

                if send_res.is_err() {
                    // Fallback to plain text if markdown parsing fails
                    self.bot
                        .send_message(teloxide::types::ChatId(chat_id), text.to_string())
                        .await
                        .map_err(|e| forja_core::error::ForjaError::ChannelError(format!(
                            "Failed to send Telegram message: {}", e
                        )))?;
                }

                // Print send log to terminal
                let log_text = text.clone();
                let _ = tokio::task::spawn_blocking(move || {
                    use std::io::Write;
                    println!("● {}", log_text);
                    print!("> ");
                    std::io::stdout().flush().ok();
                }).await;
            }
        } else {
            // Skip or log warning if no target chat exists yet
            eprintln!("[WARN] Telegram send drop: Empty last_chat_id");
        }

        Ok(())
    }
}

#[cfg(feature = "telegram")]
#[allow(dead_code)]
fn escape_markdown_v2(text: &str) -> String {
    // MarkdownV2 escape characters. Using HTML mode for simpler escaping.
    text.to_string()
}
