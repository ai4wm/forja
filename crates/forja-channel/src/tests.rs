#[cfg(feature = "discord")]
use crate::discord::{
    DiscordAllowlist, DiscordChannel, DiscordWorkerCommand, discord_reconnect_backoff_secs,
    is_allowed_discord_source, is_allowed_discord_user,
};
#[cfg(feature = "telegram")]
use crate::multi::ChannelSource;
use crate::multi::MultiChannel;
#[cfg(feature = "telegram")]
use crate::telegram::TelegramChannel;
#[cfg(feature = "telegram")]
use crate::telegram_supervisor::TelegramWorkerCommand;
#[cfg(feature = "telegram")]
use crate::telegram_supervisor::reconnect_backoff_secs;
use forja_core::Channel;
#[cfg(feature = "notification")]
use forja_core::traits::NotificationState;
#[cfg(feature = "telegram")]
use forja_core::traits::TelegramConnectionStatus;
#[cfg(feature = "telegram")]
use forja_core::{Message, Role};
#[cfg(any(feature = "telegram", feature = "discord"))]
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
#[cfg(feature = "telegram")]
use teloxide::{Bot, RequestError, dispatching::Dispatcher, dptree};

#[cfg(feature = "telegram")]
fn idle_shutdown_token() -> teloxide::dispatching::ShutdownToken {
    Dispatcher::builder(
        Bot::new("test-token"),
        dptree::entry().endpoint(|| async { Ok::<(), RequestError>(()) }),
    )
    .build()
    .shutdown_token()
}

#[cfg(feature = "telegram")]
#[tokio::test]
async fn test_telegram_shutdown_token() {
    let typing_handle = tokio::spawn(async {
        std::future::pending::<()>().await;
    });
    let abort_handle = typing_handle.abort_handle();
    let channel = TelegramChannel::for_shutdown_test(idle_shutdown_token(), typing_handle);

    assert!(channel.has_shutdown_token_for_test());
    assert!(!abort_handle.is_finished());

    channel.shutdown();
    tokio::task::yield_now().await;

    assert!(!channel.has_shutdown_token_for_test());
    assert!(abort_handle.is_finished());
}

#[cfg(feature = "telegram")]
#[tokio::test]
async fn test_multichannel_shutdown() {
    let finished = Arc::new(AtomicBool::new(false));
    let finished_for_thread = finished.clone();
    let (command_tx, mut command_rx) = tokio::sync::mpsc::unbounded_channel();
    let thread_handle = std::thread::spawn(move || {
        while let Some(command) = command_rx.blocking_recv() {
            if matches!(command, TelegramWorkerCommand::Shutdown) {
                break;
            }
        }
        finished_for_thread.store(true, Ordering::SeqCst);
    });
    let channel = MultiChannel::for_shutdown_test(command_tx, thread_handle);

    assert!(channel.has_telegram_runtime_for_test());
    assert!(!finished.load(Ordering::SeqCst));

    channel.shutdown();
    tokio::task::yield_now().await;

    assert!(!channel.has_telegram_runtime_for_test());
    assert!(finished.load(Ordering::SeqCst));
    assert_eq!(
        channel.telegram_status(),
        Some(TelegramConnectionStatus::Disconnected)
    );
}

#[tokio::test]
async fn test_multichannel_new_without_telegram_starts_in_cli_mode() {
    let channel = MultiChannel::new(
        None,
        Vec::new(),
        #[cfg(feature = "discord")]
        None,
        #[cfg(feature = "voice")]
        None,
        #[cfg(feature = "notification")]
        NotificationState::default(),
    )
    .await;

    assert!(channel.is_cli_source());
    #[cfg(feature = "telegram")]
    assert_eq!(
        channel.telegram_status(),
        Some(TelegramConnectionStatus::Disconnected)
    );
    #[cfg(not(feature = "telegram"))]
    assert_eq!(channel.telegram_status(), None);

    channel.shutdown();
}

#[cfg(feature = "telegram")]
#[test]
fn test_reconnect_backoff_caps_at_thirty_seconds() {
    assert_eq!(reconnect_backoff_secs(1), 1);
    assert_eq!(reconnect_backoff_secs(2), 2);
    assert_eq!(reconnect_backoff_secs(3), 4);
    assert_eq!(reconnect_backoff_secs(4), 8);
    assert_eq!(reconnect_backoff_secs(5), 16);
    assert_eq!(reconnect_backoff_secs(6), 30);
    assert_eq!(reconnect_backoff_secs(7), 30);
}

#[cfg(feature = "telegram")]
#[tokio::test]
async fn test_multichannel_send_drops_telegram_messages_when_disconnected() {
    let channel = MultiChannel::for_status_test(
        TelegramConnectionStatus::Disconnected,
        Some(ChannelSource::Telegram { chat_id: 123 }),
    );

    channel
        .send(Message::text(Role::Assistant, "reply", None))
        .await
        .unwrap();
}

#[cfg(feature = "discord")]
#[tokio::test]
async fn test_discord_shutdown_stops_worker_thread() {
    let finished = Arc::new(AtomicBool::new(false));
    let finished_for_thread = finished.clone();
    let (command_tx, mut command_rx) = tokio::sync::mpsc::unbounded_channel();
    let thread_handle = std::thread::spawn(move || {
        while let Some(command) = command_rx.blocking_recv() {
            if matches!(command, DiscordWorkerCommand::Shutdown) {
                break;
            }
        }
        finished_for_thread.store(true, Ordering::SeqCst);
    });
    let channel = DiscordChannel::for_shutdown_test(command_tx, thread_handle);

    assert!(channel.has_runtime_for_test());
    assert!(!finished.load(Ordering::SeqCst));

    channel.shutdown();
    tokio::task::yield_now().await;

    assert!(!channel.has_runtime_for_test());
    assert!(finished.load(Ordering::SeqCst));
}

#[cfg(feature = "discord")]
#[test]
fn test_discord_reconnect_backoff_caps_at_thirty_seconds() {
    assert_eq!(discord_reconnect_backoff_secs(1), 1);
    assert_eq!(discord_reconnect_backoff_secs(2), 2);
    assert_eq!(discord_reconnect_backoff_secs(3), 4);
    assert_eq!(discord_reconnect_backoff_secs(4), 8);
    assert_eq!(discord_reconnect_backoff_secs(5), 16);
    assert_eq!(discord_reconnect_backoff_secs(6), 30);
    assert_eq!(discord_reconnect_backoff_secs(7), 30);
}

#[cfg(feature = "discord")]
#[test]
fn test_discord_whitelist_allows_only_configured_users() {
    let allowed = vec![7, 11, 42];

    assert!(is_allowed_discord_user(&allowed, 7));
    assert!(is_allowed_discord_user(&allowed, 42));
    assert!(!is_allowed_discord_user(&allowed, 8));
    assert!(!is_allowed_discord_user(&allowed, 0));
}

#[cfg(feature = "discord")]
#[test]
fn test_discord_allowlist_supports_user_channel_and_guild_filters() {
    let allowlist = DiscordAllowlist {
        allowed_user_ids: vec![7],
        allowed_channel_ids: vec![77],
        allowed_guild_ids: vec![777],
    };

    assert!(is_allowed_discord_source(&allowlist, 7, 77, Some(777)));
    assert!(!is_allowed_discord_source(&allowlist, 8, 77, Some(777)));
    assert!(!is_allowed_discord_source(&allowlist, 7, 78, Some(777)));
    assert!(!is_allowed_discord_source(&allowlist, 7, 77, Some(778)));
    assert!(!is_allowed_discord_source(
        &DiscordAllowlist::default(),
        7,
        77,
        Some(777)
    ));
}
