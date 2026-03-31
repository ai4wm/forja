use crate::multi::MultiChannel;
#[cfg(feature = "telegram")]
use crate::telegram::TelegramChannel;
use forja_core::Channel;
#[cfg(feature = "telegram")]
use teloxide::{dispatching::Dispatcher, dptree, Bot, RequestError};

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
    let typing_handle = tokio::spawn(async {
        std::future::pending::<()>().await;
    });
    let abort_handle = typing_handle.abort_handle();
    let channel = MultiChannel::for_shutdown_test(idle_shutdown_token(), typing_handle);

    assert!(channel.has_shutdown_token_for_test());
    assert!(!abort_handle.is_finished());

    channel.shutdown();
    tokio::task::yield_now().await;

    assert!(!channel.has_shutdown_token_for_test());
    assert!(abort_handle.is_finished());
}

#[tokio::test]
async fn test_multichannel_new_without_telegram_starts_in_cli_mode() {
    let channel = MultiChannel::new(None, Vec::new()).await;

    assert!(channel.is_cli_source());

    channel.shutdown();
}
