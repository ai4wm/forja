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

/// Telegram 봇 채널의 핵심 인터페이스.
#[cfg(feature = "telegram")]
pub struct TelegramChannel {
    bot: Bot,
    receiver: Mutex<mpsc::Receiver<(i64, CoreMessage)>>,
    last_chat_id: Mutex<Option<i64>>,
    #[allow(dead_code)]
    allowed_chat_ids: Vec<i64>,
}

#[cfg(feature = "telegram")]
impl TelegramChannel {
    /// TelegramChannel 생성자. 내부적으로 백그라운드 태스크에서 봇 롱폴링을 시작합니다.
    pub async fn new(bot_token: String, allowed_chat_ids: Vec<i64>) -> Self {
        let bot = Bot::new(bot_token);
        // 채널 버퍼는 여유롭게 100 할당 (추가 최적화 가능)
        let (tx, rx) = mpsc::channel::<(i64, CoreMessage)>(100);

        // 허용 ID 리스트 복제
        let allowed_cloned = allowed_chat_ids.clone();

        // 텔레그램 디스패처 설정
        let handler = Update::filter_message().endpoint(
            move |msg: teloxide::types::Message, bot: Bot, tx: mpsc::Sender<(i64, CoreMessage)>| {
                let allowed = allowed_cloned.clone();
                async move {
                    let chat_id = msg.chat.id.0; // i64 타입 추출

                    if !allowed.contains(&chat_id) {
                        // 화이트리스트 외 접근 차단: 안내 문구 전송 후 버림
                        let _ = bot.send_message(
                            msg.chat.id, 
                            "[DENIED] Authorized users only."
                        ).await;
                        return Ok::<(), RequestError>(());
                    }

                    if let Some(text) = msg.text() {
                        let core_msg = CoreMessage::text(Role::User, text.to_string());
                        
                        // 송신 실패 (채널 파괴 등) 시도 캐치 (현재는 무시)
                        let _ = tx.send((chat_id, core_msg)).await;
                    }

                    Ok::<(), RequestError>(())
                }
            },
        );

        let bot_clone = bot.clone();
        
        // 백그라운드 태스크에서 비동식 봇 수신 처리 구동
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
        }
    }
}

#[cfg(feature = "telegram")]
#[async_trait]
impl Channel for TelegramChannel {
    async fn receive(&self) -> forja_core::error::Result<CoreMessage> {
        let mut rx = self.receiver.lock().await;

        // mpsc::Receiver에서 들어오는 메시지를 무한 대기
        if let Some((chat_id, msg)) = rx.recv().await {
            let mut last_id = self.last_chat_id.lock().await;
            *last_id = Some(chat_id);
            // 터미널에 수신 로그 출력
            if let Content::Text { ref text } = msg.content {
                println!("\nUser [Telegram]: {}", text);
            }
            Ok(msg)
        } else {
            Err(forja_core::error::ForjaError::ChannelError(
                "Telegram receiver channel closed unexpectedly".to_string()
            ))
        }
    }

    async fn send(&self, message: CoreMessage) -> forja_core::error::Result<()> {
        let last_id = *self.last_chat_id.lock().await;

        if let Some(chat_id) = last_id {
            if let Content::Text { text } = &message.content {
                self.bot
                    .send_message(teloxide::types::ChatId(chat_id), text)
                    .await
                    .map_err(|e| forja_core::error::ForjaError::ChannelError(format!(
                        "Failed to send Telegram message: {}", e
                    )))?;
                // 터미널에 전송 로그 출력
                println!("\n🤖 Assistant [Telegram]: {}", text);
            }
        } else {
            // 시스템 단독 실행 초기화 등, 대상자가 아직 없는 경우는 그냥 스킵하거나 경고 로깅
            eprintln!("[WARN] Telegram send drop: Empty last_chat_id");
        }

        Ok(())
    }
}
