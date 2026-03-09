use async_trait::async_trait;
use forja_core::{Channel, Content, Role, Message as CoreMessage};
use std::io::Write;
use tokio::sync::{mpsc, Mutex};

#[cfg(feature = "telegram")]
use teloxide::{prelude::*, types::ParseMode};

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
    typing_handle: Mutex<Option<tokio::task::JoinHandle<()>>>,
}

impl MultiChannel {
    /// CLI 전용 (텔레그램 없음)
    pub async fn new_cli_only() -> Self {
        let (tx, rx) = mpsc::channel::<(ChannelSource, CoreMessage)>(100);

        let tx_cli = tx.clone();
        tokio::spawn(async move {
            loop {
                let line = tokio::task::spawn_blocking(|| {
                    let mut input = String::new();
                    std::io::stdin().read_line(&mut input).ok();
                    input.trim().to_string()
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
                    let mut input = String::new();
                    std::io::stdin().read_line(&mut input).ok();
                    input.trim().to_string()
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
            typing_handle: Mutex::new(None),
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
                    // 현재 줄(프롬프트 "> ") 지우고 출력
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

        if let Some(source) = last_src {
            if let Content::Text { text, .. } = &message.content {
                match source {
                    ChannelSource::Cli => {
                        let t = text.clone();
                        let _ = tokio::task::spawn_blocking(move || {
                            // 도구 폴백일 때 응답 출력 + 프롬프트 복원
                            println!("● {}", t);
                            print!("> ");
                            std::io::stdout().flush().ok();
                        }).await;
                    }
                    #[cfg(feature = "telegram")]
                    ChannelSource::Telegram { chat_id } => {
                        if let Some(bot) = &self.telegram_bot {
                            let send_res = bot
                                .send_message(teloxide::types::ChatId(chat_id), text.clone())
                                .parse_mode(ParseMode::Markdown)
                                .await;

                            if send_res.is_err() {
                                bot.send_message(teloxide::types::ChatId(chat_id), text.clone())
                                    .await
                                    .map_err(|e| {
                                        forja_core::error::ForjaError::ChannelError(format!(
                                            "Failed to send Telegram message: {}",
                                            e
                                        ))
                                    })?;
                            }
                            
                            // 터미널에 ● 로그
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
            if let Some(handle) = self.typing_handle.lock().await.take() {
                handle.abort();
            }
        }
    }
}
