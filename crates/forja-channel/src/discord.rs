use async_trait::async_trait;
use forja_core::{Channel, Content, Message as CoreMessage, Role};
use serenity::{
    client::{Client, Context, EventHandler},
    http::Typing,
    model::{channel::Message as DiscordMessage, gateway::GatewayIntents, id::ChannelId},
};
use std::{
    sync::{Arc, Mutex as StdMutex},
    thread::JoinHandle,
    time::Duration,
};
use tokio::sync::{Mutex, mpsc};

#[derive(Clone, Debug, Default)]
pub struct DiscordAllowlist {
    pub allowed_user_ids: Vec<u64>,
    pub allowed_channel_ids: Vec<u64>,
    pub allowed_guild_ids: Vec<u64>,
}

#[derive(Clone, Debug)]
pub(crate) enum DiscordWorkerCommand {
    SendMessage { channel_id: u64, text: String },
    StartTyping { channel_id: u64 },
    StopTyping,
    Shutdown,
}

struct DiscordRuntimeHandle {
    command_tx: tokio::sync::mpsc::UnboundedSender<DiscordWorkerCommand>,
    thread_handle: JoinHandle<()>,
}

pub struct DiscordChannel {
    receiver: Mutex<mpsc::Receiver<(u64, CoreMessage)>>,
    last_channel_id: Mutex<Option<u64>>,
    runtime: StdMutex<Option<DiscordRuntimeHandle>>,
}

impl DiscordChannel {
    pub async fn new(
        bot_token: String,
        allowlist: DiscordAllowlist,
    ) -> forja_core::error::Result<Self> {
        let (tx, rx) = mpsc::channel(100);
        Ok(Self {
            receiver: Mutex::new(rx),
            last_channel_id: Mutex::new(None),
            runtime: StdMutex::new(Some(start_discord_supervisor(bot_token, allowlist, tx))),
        })
    }

    fn shutdown_inner(&self) {
        if let Some(runtime) = self
            .runtime
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take()
        {
            let _ = runtime.command_tx.send(DiscordWorkerCommand::Shutdown);
            let _ = runtime.thread_handle.join();
        }
    }

    #[cfg(test)]
    pub(crate) fn for_shutdown_test(
        command_tx: tokio::sync::mpsc::UnboundedSender<DiscordWorkerCommand>,
        thread_handle: JoinHandle<()>,
    ) -> Self {
        let (_sender, receiver) = mpsc::channel(1);
        Self {
            receiver: Mutex::new(receiver),
            last_channel_id: Mutex::new(None),
            runtime: StdMutex::new(Some(DiscordRuntimeHandle {
                command_tx,
                thread_handle,
            })),
        }
    }

    #[cfg(test)]
    pub(crate) fn has_runtime_for_test(&self) -> bool {
        self.runtime
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .is_some()
    }

    fn command_tx(&self) -> Option<tokio::sync::mpsc::UnboundedSender<DiscordWorkerCommand>> {
        self.runtime
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
            .map(|runtime| runtime.command_tx.clone())
    }
}

#[async_trait]
impl Channel for DiscordChannel {
    async fn receive(&self) -> forja_core::error::Result<CoreMessage> {
        let mut rx = self.receiver.lock().await;
        let Some((channel_id, msg)) = rx.recv().await else {
            return Err(forja_core::error::ForjaError::ChannelError(
                "Discord receiver channel closed unexpectedly".to_string(),
            ));
        };
        *self.last_channel_id.lock().await = Some(channel_id);
        if let Some(command_tx) = self.command_tx() {
            let _ = command_tx.send(DiscordWorkerCommand::StartTyping { channel_id });
        }
        Ok(msg)
    }

    async fn send(&self, message: CoreMessage) -> forja_core::error::Result<()> {
        self.cancel_typing().await;
        let Some(channel_id) = *self.last_channel_id.lock().await else {
            return Ok(());
        };
        let Some(command_tx) = self.command_tx() else {
            return Err(forja_core::error::ForjaError::ChannelError(
                "Discord worker is not running".to_string(),
            ));
        };

        if let Content::Text { text, .. } = message.content {
            command_tx
                .send(DiscordWorkerCommand::SendMessage { channel_id, text })
                .map_err(|_| {
                    forja_core::error::ForjaError::ChannelError(
                        "Discord worker command channel closed".to_string(),
                    )
                })?;
        }
        Ok(())
    }

    async fn cancel_typing(&self) {
        if let Some(command_tx) = self.command_tx() {
            let _ = command_tx.send(DiscordWorkerCommand::StopTyping);
        }
    }

    fn shutdown(&self) {
        self.shutdown_inner();
    }

    fn active_channel_name(&self) -> Option<&'static str> {
        Some("discord")
    }
}

impl Drop for DiscordChannel {
    fn drop(&mut self) {
        self.shutdown_inner();
    }
}

pub(crate) fn discord_reconnect_backoff_secs(attempt: u32) -> u64 {
    (1u64 << attempt.saturating_sub(1).min(5)).min(30)
}

#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn is_allowed_discord_user(allowed_user_ids: &[u64], user_id: u64) -> bool {
    is_allowed_discord_source(
        &DiscordAllowlist {
            allowed_user_ids: allowed_user_ids.to_vec(),
            ..DiscordAllowlist::default()
        },
        user_id,
        0,
        None,
    )
}

pub(crate) fn is_allowed_discord_source(
    allowlist: &DiscordAllowlist,
    user_id: u64,
    channel_id: u64,
    guild_id: Option<u64>,
) -> bool {
    let any_filters = !allowlist.allowed_user_ids.is_empty()
        || !allowlist.allowed_channel_ids.is_empty()
        || !allowlist.allowed_guild_ids.is_empty();
    if !any_filters {
        return false;
    }

    let user_ok =
        allowlist.allowed_user_ids.is_empty() || allowlist.allowed_user_ids.contains(&user_id);
    let channel_ok = allowlist.allowed_channel_ids.is_empty()
        || allowlist.allowed_channel_ids.contains(&channel_id);
    let guild_ok = allowlist.allowed_guild_ids.is_empty()
        || guild_id
            .map(|id| allowlist.allowed_guild_ids.contains(&id))
            .unwrap_or(false);

    user_ok && channel_ok && guild_ok
}

fn start_discord_supervisor(
    bot_token: String,
    allowlist: DiscordAllowlist,
    tx: mpsc::Sender<(u64, CoreMessage)>,
) -> DiscordRuntimeHandle {
    let (command_tx, command_rx) = tokio::sync::mpsc::unbounded_channel();
    let thread_handle = std::thread::spawn(move || {
        let runtime = match tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        {
            Ok(runtime) => runtime,
            Err(error) => {
                eprintln!("[WARN] Failed to build Discord runtime: {error}");
                return;
            }
        };

        let mut command_rx = command_rx;
        let mut attempt = 1u32;
        loop {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                runtime.block_on(run_discord_attempt(
                    &bot_token,
                    &allowlist,
                    &tx,
                    &mut command_rx,
                ))
            }));
            match result {
                Ok(DiscordAttemptResult::Shutdown) => break,
                Ok(DiscordAttemptResult::Reconnect) => {}
                Err(_) => eprintln!("[WARN] Discord runtime panicked"),
            }
            let delay_secs = discord_reconnect_backoff_secs(attempt);
            attempt = attempt.saturating_add(1);
            if runtime.block_on(wait_for_backoff_or_shutdown(&mut command_rx, delay_secs)) {
                break;
            }
        }
    });
    DiscordRuntimeHandle {
        command_tx,
        thread_handle,
    }
}

enum DiscordAttemptResult {
    Reconnect,
    Shutdown,
}

async fn run_discord_attempt(
    bot_token: &str,
    allowlist: &DiscordAllowlist,
    tx: &mpsc::Sender<(u64, CoreMessage)>,
    command_rx: &mut tokio::sync::mpsc::UnboundedReceiver<DiscordWorkerCommand>,
) -> DiscordAttemptResult {
    let intents = GatewayIntents::DIRECT_MESSAGES
        | GatewayIntents::GUILD_MESSAGES
        | GatewayIntents::MESSAGE_CONTENT;
    let handler = DiscordEventHandler {
        allowlist: Arc::new(allowlist.clone()),
        inbound_tx: tx.clone(),
    };
    let mut client = match Client::builder(bot_token, intents)
        .event_handler(handler)
        .await
    {
        Ok(client) => client,
        Err(error) => {
            eprintln!("[WARN] Failed to build Discord client: {error}");
            return DiscordAttemptResult::Reconnect;
        }
    };

    let http = client.http.clone();
    let shard_manager = client.shard_manager.clone();
    let mut client_task = tokio::spawn(async move { client.start().await });
    let mut typing: Option<Typing> = None;

    loop {
        tokio::select! {
            command = command_rx.recv() => match command {
                Some(DiscordWorkerCommand::SendMessage { channel_id, text }) => {
                    if let Some(active) = typing.take() { active.stop(); }
                    if let Err(error) = ChannelId::new(channel_id).say(&http, text).await {
                        eprintln!("[WARN] Failed to send Discord message: {error}");
                    }
                }
                Some(DiscordWorkerCommand::StartTyping { channel_id }) => {
                    if let Some(active) = typing.take() { active.stop(); }
                    typing = Some(ChannelId::new(channel_id).start_typing(&http));
                }
                Some(DiscordWorkerCommand::StopTyping) => {
                    if let Some(active) = typing.take() { active.stop(); }
                }
                Some(DiscordWorkerCommand::Shutdown) | None => {
                    if let Some(active) = typing.take() { active.stop(); }
                    shard_manager.shutdown_all().await;
                    let _ = (&mut client_task).await;
                    return DiscordAttemptResult::Shutdown;
                }
            },
            result = &mut client_task => {
                if let Some(active) = typing.take() { active.stop(); }
                match result {
                    Ok(Ok(())) => {}
                    Ok(Err(error)) => eprintln!("[WARN] Discord client stopped: {error}"), Err(error) => eprintln!("[WARN] Discord client task failed: {error}"),
                }
                return DiscordAttemptResult::Reconnect;
            }
        }
    }
}

async fn wait_for_backoff_or_shutdown(
    command_rx: &mut tokio::sync::mpsc::UnboundedReceiver<DiscordWorkerCommand>,
    delay_secs: u64,
) -> bool {
    let sleep = tokio::time::sleep(Duration::from_secs(delay_secs));
    tokio::pin!(sleep);
    loop {
        tokio::select! {
            _ = &mut sleep => return false,
            command = command_rx.recv() => match command {
                Some(DiscordWorkerCommand::Shutdown) | None => return true,
                Some(DiscordWorkerCommand::SendMessage { .. })
                | Some(DiscordWorkerCommand::StartTyping { .. })
                | Some(DiscordWorkerCommand::StopTyping) => {}
            }
        }
    }
}

struct DiscordEventHandler {
    allowlist: Arc<DiscordAllowlist>,
    inbound_tx: mpsc::Sender<(u64, CoreMessage)>,
}

#[serenity::async_trait]
impl EventHandler for DiscordEventHandler {
    async fn message(&self, ctx: Context, msg: DiscordMessage) {
        if msg.author.bot || msg.content.trim().is_empty() {
            return;
        }
        if !is_allowed_discord_source(
            &self.allowlist,
            msg.author.id.get(),
            msg.channel_id.get(),
            msg.guild_id.map(|id| id.get()),
        ) {
            let _ = msg
                .channel_id
                .say(&ctx.http, "[DENIED] Authorized users only.")
                .await;
            return;
        }
        let channel_id = msg.channel_id.get();
        let core_msg = CoreMessage::text(Role::User, msg.content.clone(), None);
        let _ = self.inbound_tx.send((channel_id, core_msg)).await;
    }
}
