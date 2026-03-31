use async_trait::async_trait;
use forja_core::gateway::adapter::{ChannelAdapter, CliAdapter};
use forja_core::{Channel, Content, Role, Message as CoreMessage};
#[cfg(feature = "telegram")]
use forja_core::gateway::adapter::TelegramAdapter;
#[cfg(feature = "telegram")]
use reqwest::Client;
use crate::cli::process_line;
use std::io::Write;
#[cfg(feature = "telegram")]
use std::sync::{Arc, Mutex as StdMutex};
#[cfg(feature = "telegram")]
use std::time::Duration;
use tokio::sync::{mpsc, Mutex};

#[cfg(feature = "telegram")]
use teloxide::dispatching::{Dispatcher, UpdateFilterExt};
#[cfg(feature = "telegram")]
use teloxide::prelude::*;

#[derive(Clone, Debug)]
pub enum ChannelSource {
    Cli,
    #[cfg(feature = "telegram")]
    Telegram { chat_id: i64 },
}

#[cfg(feature = "telegram")]
pub(crate) enum TelegramWorkerCommand {
    SendMessage { chat_id: i64, text: String },
    StartTyping { chat_id: i64 },
    StopTyping,
    Shutdown,
}

#[cfg(feature = "telegram")]
struct TelegramRuntimeHandle {
    command_tx: tokio::sync::mpsc::UnboundedSender<TelegramWorkerCommand>,
    thread_handle: std::thread::JoinHandle<()>,
}

pub struct MultiChannel {
    receiver: Mutex<mpsc::Receiver<(ChannelSource, CoreMessage)>>,
    last_source: Mutex<Option<ChannelSource>>,
    #[cfg(feature = "telegram")]
    telegram_runtime: StdMutex<Option<TelegramRuntimeHandle>>,
}

impl MultiChannel {
    pub async fn new(bot_token: Option<String>, allowed_chat_ids: Vec<i64>) -> Self {
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

        #[cfg(feature = "telegram")]
        let telegram_runtime = if let Some(bot_token) = bot_token {
            match Self::start_telegram_runtime(bot_token, allowed_chat_ids, tx.clone()) {
                Ok(runtime_handle) => Some(runtime_handle),
                Err(error) => {
                    eprintln!("[WARN] Telegram initialization failed: {error}");
                    None
                }
            }
        } else {
            None
        };

        #[cfg(not(feature = "telegram"))]
        let _ = (bot_token, allowed_chat_ids);

        Self {
            receiver: Mutex::new(rx),
            last_source: Mutex::new(Some(ChannelSource::Cli)),
            #[cfg(feature = "telegram")]
            telegram_runtime: StdMutex::new(telegram_runtime),
        }
    }

    /// CLI only (no Telegram)
    pub async fn new_cli_only() -> Self {
        Self::new(None, Vec::new()).await
    }

    #[cfg(feature = "telegram")]
    pub async fn new_both(bot_token: String, allowed_chat_ids: Vec<i64>) -> Self {
        Self::new(Some(bot_token), allowed_chat_ids).await
    }

    pub fn has_telegram(&self) -> bool {
        #[cfg(feature = "telegram")]
        {
            self.telegram_runtime
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .is_some()
        }

        #[cfg(not(feature = "telegram"))]
        {
            false
        }
    }

    #[cfg(feature = "telegram")]
    fn start_telegram_runtime(
        bot_token: String,
        allowed_chat_ids: Vec<i64>,
        tx: mpsc::Sender<(ChannelSource, CoreMessage)>,
    ) -> forja_core::error::Result<TelegramRuntimeHandle> {
        let (command_tx, command_rx) = tokio::sync::mpsc::unbounded_channel();
        let (init_tx, init_rx) = std::sync::mpsc::channel();
        let init_tx = Arc::new(StdMutex::new(Some(init_tx)));
        let init_tx_for_thread = init_tx.clone();
        let thread_handle = std::thread::spawn(move || {
            let runtime = match tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                Ok(runtime) => runtime,
                Err(error) => {
                    if let Some(init_tx) = init_tx_for_thread
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner())
                        .take()
                    {
                        let _ = init_tx.send(Err(format!(
                            "Failed to build telegram runtime: {error}"
                        )));
                    }
                    return;
                }
            };

            let init_tx_for_panic = init_tx_for_thread.clone();
            let thread_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                runtime.block_on(async move {
                    let client = match Client::builder()
                        .connect_timeout(Duration::from_secs(30))
                        .timeout(Duration::from_secs(60))
                        .build()
                    {
                        Ok(client) => client,
                        Err(error) => {
                            if let Some(init_tx) = init_tx_for_thread
                                .lock()
                                .unwrap_or_else(|poisoned| poisoned.into_inner())
                                .take()
                            {
                                let _ = init_tx.send(Err(format!(
                                    "Failed to build reqwest client: {error}"
                                )));
                            }
                            return;
                        }
                    };
                    let bot = Bot::with_client(bot_token, client);
                    match tokio::time::timeout(Duration::from_secs(60), bot.get_me()).await {
                        Ok(Ok(_)) => {}
                        Ok(Err(error)) => {
                            if let Some(init_tx) = init_tx_for_thread
                                .lock()
                                .unwrap_or_else(|poisoned| poisoned.into_inner())
                                .take()
                            {
                                let _ = init_tx.send(Err(format!(
                                    "Telegram getMe failed: {error}"
                                )));
                            }
                            return;
                        }
                        Err(_) => {
                            if let Some(init_tx) = init_tx_for_thread
                                .lock()
                                .unwrap_or_else(|poisoned| poisoned.into_inner())
                                .take()
                            {
                                let _ = init_tx.send(Err(
                                    "Telegram getMe timed out after 60 seconds".to_string(),
                                ));
                            }
                            return;
                        }
                    }

                    let tx_tg = tx.clone();
                    let allowed = allowed_chat_ids.clone();
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
                    let dispatcher_task = tokio::spawn(async move {
                        dispatcher.dispatch().await;
                    });

                    if let Some(init_tx) = init_tx_for_thread
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner())
                        .take()
                    {
                        let _ = init_tx.send(Ok(()));
                    }

                    let mut command_rx = command_rx;
                    let mut typing_handle: Option<tokio::task::JoinHandle<()>> = None;
                    let mut dispatcher_task = dispatcher_task;

                    loop {
                        tokio::select! {
                            command = command_rx.recv() => {
                                match command {
                                    Some(TelegramWorkerCommand::SendMessage { chat_id, text }) => {
                                        if let Err(error) = bot
                                            .send_message(teloxide::types::ChatId(chat_id), text)
                                            .await
                                        {
                                            eprintln!("Failed to send Telegram message: {error}");
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
                                                tokio::time::sleep(tokio::time::Duration::from_secs(4)).await;
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
                                        break;
                                    }
                                }
                            }
                            result = &mut dispatcher_task => {
                                if let Err(error) = result {
                                    eprintln!("Telegram dispatcher task failed: {error}");
                                }
                                if let Some(handle) = typing_handle.take() {
                                    handle.abort();
                                }
                                break;
                            }
                        }
                    }

                    dispatcher_task.abort();
                    let _ = dispatcher_task.await;
                });
            }));

            if thread_result.is_err() {
                if let Some(init_tx) = init_tx_for_panic
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .take()
                {
                    let _ = init_tx.send(Err("Telegram runtime panicked".to_string()));
                }
                eprintln!("[WARN] Telegram runtime panicked");
            }
        });

        match init_rx.recv_timeout(Duration::from_secs(65)) {
            Ok(Ok(())) => Ok(TelegramRuntimeHandle {
                command_tx,
                thread_handle,
            }),
            Ok(Err(error)) => {
                let _ = thread_handle.join();
                Err(forja_core::error::ForjaError::ChannelError(error))
            }
            Err(error) => {
                let _ = thread_handle.join();
                Err(forja_core::error::ForjaError::ChannelError(format!(
                    "Telegram runtime init channel failed: {error}"
                )))
            }
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

    fn shutdown_inner(&self) {
        #[cfg(feature = "telegram")]
        {
            if let Some(runtime) = self
                .telegram_runtime
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .take()
            {
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

        Self {
            receiver: Mutex::new(receiver),
            last_source: Mutex::new(None),
            telegram_runtime: StdMutex::new(Some(TelegramRuntimeHandle {
                command_tx,
                thread_handle,
            })),
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
                            println!("• {}", t);
                            print!("> ");
                            std::io::stdout().flush().ok();
                        }).await;
                    }
                    #[cfg(feature = "telegram")]
                    ChannelSource::Telegram { chat_id } => {
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

    fn shutdown(&self) {
        self.shutdown_inner();
    }
}

impl Drop for MultiChannel {
    fn drop(&mut self) {
        self.shutdown_inner();
    }
}
