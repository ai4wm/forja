#[cfg(feature = "telegram")]
use crate::multi::ChannelSource;
#[cfg(feature = "telegram")]
use forja_core::traits::TelegramConnectionStatus;
#[cfg(feature = "telegram")]
use forja_core::{Message as CoreMessage, Role};
#[cfg(feature = "telegram")]
use reqwest::Client;
#[cfg(feature = "telegram")]
use std::sync::{Arc, Mutex as StdMutex};
#[cfg(feature = "telegram")]
use std::time::Duration;
#[cfg(feature = "telegram")]
use teloxide::dispatching::{Dispatcher, UpdateFilterExt};
#[cfg(feature = "telegram")]
use teloxide::prelude::*;
#[cfg(feature = "telegram")]
use tokio::sync::mpsc;

#[cfg(feature = "telegram")]
#[derive(Clone, Debug)]
pub(crate) enum TelegramWorkerCommand {
    SendMessage { chat_id: i64, text: String },
    StartTyping { chat_id: i64 },
    StopTyping,
    Shutdown,
}

#[cfg(feature = "telegram")]
pub(crate) struct TelegramRuntimeHandle {
    pub(crate) command_tx: tokio::sync::mpsc::UnboundedSender<TelegramWorkerCommand>,
    pub(crate) thread_handle: std::thread::JoinHandle<()>,
    pub(crate) status: TelegramStatusHandle,
}

#[cfg(feature = "telegram")]
#[derive(Clone)]
pub struct TelegramStatusHandle {
    state: Arc<StdMutex<TelegramConnectionStatus>>,
}

#[cfg(feature = "telegram")]
impl TelegramStatusHandle {
    pub(crate) fn new(status: TelegramConnectionStatus) -> Self {
        Self {
            state: Arc::new(StdMutex::new(status)),
        }
    }

    pub fn snapshot(&self) -> TelegramConnectionStatus {
        *self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    pub(crate) fn set(&self, status: TelegramConnectionStatus) {
        if let Ok(mut state) = self.state.lock() {
            *state = status;
        }
    }
}

#[cfg(feature = "telegram")]
pub(crate) fn reconnect_backoff_secs(attempt: u32) -> u64 {
    let shift = attempt.saturating_sub(1).min(5);
    let backoff = 1u64 << shift;
    backoff.min(30)
}

#[cfg(feature = "telegram")]
pub(crate) fn start_telegram_supervisor(
    bot_token: String,
    allowed_chat_ids: Vec<i64>,
    tx: mpsc::Sender<(ChannelSource, CoreMessage)>,
) -> TelegramRuntimeHandle {
    let status = TelegramStatusHandle::new(TelegramConnectionStatus::Reconnecting);
    let status_for_thread = status.clone();
    let (command_tx, command_rx) = tokio::sync::mpsc::unbounded_channel();
    let thread_handle = std::thread::spawn(move || {
        let runtime = match tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        {
            Ok(runtime) => runtime,
            Err(error) => {
                eprintln!("[WARN] Failed to build telegram runtime: {error}");
                status_for_thread.set(TelegramConnectionStatus::Disconnected);
                return;
            }
        };

        let mut command_rx = command_rx;
        let mut attempt = 1u32;

        loop {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                runtime.block_on(run_telegram_attempt(
                    &bot_token,
                    &allowed_chat_ids,
                    &tx,
                    &mut command_rx,
                    status_for_thread.clone(),
                ))
            }));

            match result {
                Ok(TelegramAttemptResult::Shutdown) => break,
                Ok(TelegramAttemptResult::Reconnect) => {
                    status_for_thread.set(TelegramConnectionStatus::Reconnecting);
                }
                Err(_) => {
                    eprintln!("[WARN] Telegram runtime panicked");
                    status_for_thread.set(TelegramConnectionStatus::Reconnecting);
                }
            }

            let delay_secs = reconnect_backoff_secs(attempt);
            attempt = attempt.saturating_add(1);
            if runtime.block_on(wait_for_backoff_or_shutdown(&mut command_rx, delay_secs)) {
                break;
            }
        }

        status_for_thread.set(TelegramConnectionStatus::Disconnected);
    });

    TelegramRuntimeHandle {
        command_tx,
        thread_handle,
        status,
    }
}

#[cfg(feature = "telegram")]
enum TelegramAttemptResult {
    Reconnect,
    Shutdown,
}

#[cfg(feature = "telegram")]
async fn run_telegram_attempt(
    bot_token: &str,
    allowed_chat_ids: &[i64],
    tx: &mpsc::Sender<(ChannelSource, CoreMessage)>,
    command_rx: &mut tokio::sync::mpsc::UnboundedReceiver<TelegramWorkerCommand>,
    status: TelegramStatusHandle,
) -> TelegramAttemptResult {
    let client = match Client::builder()
        .connect_timeout(Duration::from_secs(30))
        .timeout(Duration::from_secs(60))
        .build()
    {
        Ok(client) => client,
        Err(error) => {
            eprintln!("[WARN] Failed to build reqwest client: {error}");
            return TelegramAttemptResult::Reconnect;
        }
    };

    let bot = Bot::with_client(bot_token.to_string(), client);
    match tokio::time::timeout(Duration::from_secs(60), bot.get_me()).await {
        Ok(Ok(_)) => {}
        Ok(Err(error)) => {
            eprintln!("[WARN] Telegram getMe failed: {error}");
            return TelegramAttemptResult::Reconnect;
        }
        Err(_) => {
            eprintln!("[WARN] Telegram getMe timed out after 60 seconds");
            return TelegramAttemptResult::Reconnect;
        }
    }

    let tx_tg = tx.clone();
    let allowed = allowed_chat_ids.to_vec();
    let handler = teloxide::types::Update::filter_message().endpoint(
        move |msg: teloxide::types::Message,
              bot: Bot,
              tx_tg: mpsc::Sender<(ChannelSource, CoreMessage)>| {
            let allowed = allowed.clone();
            async move {
                let chat_id = msg.chat.id.0;
                if !allowed.contains(&chat_id) {
                    let _ = bot
                        .send_message(msg.chat.id, "[DENIED] Authorized users only.")
                        .await;
                    return Ok::<(), teloxide::RequestError>(());
                }
                if let Some(text) = msg.text() {
                    let core_msg = CoreMessage::text(Role::User, text.to_string(), None);
                    let _ = tx_tg
                        .send((ChannelSource::Telegram { chat_id }, core_msg))
                        .await;
                }
                Ok::<(), teloxide::RequestError>(())
            }
        },
    );

    let bot_dispatcher = bot.clone();
    let mut dispatcher = Dispatcher::builder(bot_dispatcher, handler)
        .dependencies(teloxide::dptree::deps![tx_tg])
        .build();
    let shutdown_token = dispatcher.shutdown_token();
    let dispatcher_task = tokio::spawn(async move {
        dispatcher.dispatch().await;
    });
    let mut typing_handle: Option<tokio::task::JoinHandle<()>> = None;
    let mut dispatcher_task = dispatcher_task;
    status.set(TelegramConnectionStatus::Connected);

    loop {
        tokio::select! {
            command = command_rx.recv() => {
                match command {
                    Some(TelegramWorkerCommand::SendMessage { chat_id, text }) => {
                        if let Err(error) = bot.send_message(teloxide::types::ChatId(chat_id), text).await {
                            eprintln!("[WARN] Failed to send Telegram message: {error}");
                        }
                    }
                    Some(TelegramWorkerCommand::StartTyping { chat_id }) => {
                        if let Some(handle) = typing_handle.take() {
                            handle.abort();
                        }
                        let bot_clone = bot.clone();
                        typing_handle = Some(tokio::spawn(async move {
                            loop {
                                let _ = bot_clone
                                    .send_chat_action(
                                        teloxide::types::ChatId(chat_id),
                                        teloxide::types::ChatAction::Typing,
                                    )
                                    .await;
                                tokio::time::sleep(Duration::from_secs(4)).await;
                            }
                        }));
                    }
                    Some(TelegramWorkerCommand::StopTyping) => {
                        if let Some(handle) = typing_handle.take() {
                            handle.abort();
                        }
                    }
                    Some(TelegramWorkerCommand::Shutdown) | None => {
                        if let Some(handle) = typing_handle.take() {
                            handle.abort();
                        }
                        let _ = shutdown_token.shutdown();
                        dispatcher_task.abort();
                        let _ = dispatcher_task.await;
                        return TelegramAttemptResult::Shutdown;
                    }
                }
            }
            result = &mut dispatcher_task => {
                if let Err(error) = result {
                    eprintln!("[WARN] Telegram dispatcher task failed: {error}");
                }
                if let Some(handle) = typing_handle.take() {
                    handle.abort();
                }
                return TelegramAttemptResult::Reconnect;
            }
        }
    }
}

#[cfg(feature = "telegram")]
async fn wait_for_backoff_or_shutdown(
    command_rx: &mut tokio::sync::mpsc::UnboundedReceiver<TelegramWorkerCommand>,
    delay_secs: u64,
) -> bool {
    let sleep = tokio::time::sleep(Duration::from_secs(delay_secs));
    tokio::pin!(sleep);

    loop {
        tokio::select! {
            _ = &mut sleep => return false,
            command = command_rx.recv() => {
                match command {
                    Some(TelegramWorkerCommand::Shutdown) | None => return true,
                    Some(TelegramWorkerCommand::SendMessage { .. })
                    | Some(TelegramWorkerCommand::StartTyping { .. })
                    | Some(TelegramWorkerCommand::StopTyping) => {}
                }
            }
        }
    }
}
