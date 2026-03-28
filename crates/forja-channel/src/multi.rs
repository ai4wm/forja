use async_trait::async_trait;
use forja_core::{Channel, Content, Role, Message as CoreMessage};
use crate::cli::{confirm_via_stdin, process_line};
use std::io::Write;
use tokio::sync::{mpsc, Mutex};

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
    allowed_chat_ids: Vec<i64>,
    #[cfg(feature = "telegram")]
    typing_handle: Mutex<Option<tokio::task::JoinHandle<()>>>,
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
            allowed_chat_ids: Vec::new(),
            #[cfg(feature = "telegram")]
            typing_handle: Mutex::new(None),
        }
    }

    #[cfg(feature = "telegram")]
    pub async fn new_both(bot_token: String, allowed_chat_ids: Vec<i64>) -> Self {
        let (tx, rx) = mpsc::channel::<(ChannelSource, CoreMessage)>(100);
        let bot = Bot::new(bot_token);
        
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
        tokio::spawn(async move {
            teloxide::dispatching::Dispatcher::builder(bot_dispatcher, handler)
                .dependencies(teloxide::dptree::deps![tx_tg])
                .enable_ctrlc_handler()
                .build()
                .dispatch()
                .await;
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

        Self {
            receiver: Mutex::new(rx),
            last_source: Mutex::new(None),
            telegram_bot: Some(bot),
            allowed_chat_ids,
            typing_handle: Mutex::new(None),
        }
    }
}

fn render_cli_output(text: String) {
    println!("● {}", text);
    print!("> ");
    std::io::stdout().flush().ok();
}

#[cfg(feature = "telegram")]
fn target_chat_ids(last_source: &Option<ChannelSource>, allowed_chat_ids: &[i64]) -> Vec<i64> {
    match last_source {
        Some(ChannelSource::Telegram { chat_id }) => vec![*chat_id],
        Some(ChannelSource::Cli) => Vec::new(),
        None => allowed_chat_ids.to_vec(),
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
                    *self.typing_handle.lock().await = Some(handle);
                }

                if let Content::Text { ref text, .. } = msg.content {
                    // Clear current line (prompt ">") and print
                    print!("\r\x1b[K");
                    println!("[TG] {}", text);
                }
            }

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

        if let Content::Text { text, .. } = &message.content {
            match last_src {
                Some(ChannelSource::Cli) => {
                    let terminal_text = text.clone();
                    let _ = tokio::task::spawn_blocking(move || render_cli_output(terminal_text))
                        .await;
                }
                #[cfg(feature = "telegram")]
                Some(ChannelSource::Telegram { .. }) | None => {
                    if let Some(bot) = &self.telegram_bot {
                        for chat_id in target_chat_ids(&last_src, &self.allowed_chat_ids) {
                            if let Err(error) = bot
                                .send_message(teloxide::types::ChatId(chat_id), text.clone())
                                .await
                            {
                                eprintln!(
                                    "[WARN] Failed to send Telegram message to {chat_id}: {error}"
                                );
                            }
                        }
                    }

                    let terminal_text = text.clone();
                    let _ = tokio::task::spawn_blocking(move || render_cli_output(terminal_text))
                        .await;
                }
                #[cfg(not(feature = "telegram"))]
                None => {
                    let terminal_text = text.clone();
                    let _ = tokio::task::spawn_blocking(move || render_cli_output(terminal_text))
                        .await;
                }
            }
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

    async fn confirm(&self, message: &str) -> forja_core::error::Result<bool> {
        match self.last_source.lock().await.clone() {
            Some(ChannelSource::Cli) | None => confirm_via_stdin(message).await,
            #[cfg(feature = "telegram")]
            Some(ChannelSource::Telegram { .. }) => Ok(true),
        }
    }

    async fn cancel_typing(&self) {
        #[cfg(feature = "telegram")]
        {
            if let Some(handle) = self.typing_handle.lock().await.take() {
                handle.abort();
            }
        }
    }
}

#[cfg(all(test, feature = "telegram"))]
mod tests {
    use super::{ChannelSource, target_chat_ids};

    #[test]
    fn target_chat_ids_broadcasts_when_no_source_is_known() {
        let targets = target_chat_ids(&None, &[10, 20]);

        assert_eq!(targets, vec![10, 20]);
    }

    #[test]
    fn target_chat_ids_routes_to_last_telegram_source_when_present() {
        let targets = target_chat_ids(&Some(ChannelSource::Telegram { chat_id: 42 }), &[10, 20]);

        assert_eq!(targets, vec![42]);
    }
}
