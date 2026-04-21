use crate::multi::MultiChannel;
#[cfg(feature = "notification")]
use forja_core::traits::NotificationState;
#[cfg(feature = "telegram")]
use forja_core::traits::TelegramConnectionStatus;
#[cfg(feature = "telegram")]
use crate::telegram_supervisor::TelegramWorkerCommand;
#[cfg(feature = "telegram")]
use crate::multi::ChannelSource;
#[cfg(feature = "telegram")]
use crate::telegram::TelegramChannel;
#[cfg(feature = "telegram")]
use crate::telegram_supervisor::reconnect_backoff_secs;
use forja_core::Channel;
#[cfg(feature = "telegram")]
use forja_core::{Message, Role};
#[cfg(feature = "telegram")]
use teloxide::{dispatching::Dispatcher, dptree, Bot, RequestError};
#[cfg(feature = "telegram")]
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

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
    assert_eq!(channel.telegram_status(), Some(TelegramConnectionStatus::Disconnected));
}

#[tokio::test]
async fn test_multichannel_new_without_telegram_starts_in_cli_mode() {
    let channel = MultiChannel::new(
        None,
        Vec::new(),
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
