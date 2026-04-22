#[cfg(feature = "runtime")]
use tokio::sync::mpsc::Sender;
#[cfg(feature = "runtime")]
use tokio::time::{Duration, interval};

/// System scheduler, active only when the runtime feature is enabled.
///
/// Runs in the background at the given `interval_secs` and sends scheduler
/// events into the engine loop through `event_tx`.
#[cfg(feature = "runtime")]
pub async fn run_scheduler(event_tx: Sender<String>, interval_secs: u64) {
    let mut ticker = interval(Duration::from_secs(interval_secs));

    // The first tick fires immediately; keep or skip it depending on the policy.
    ticker.tick().await;

    tokio::spawn(async move {
        loop {
            ticker.tick().await;

            // Send a scheduler trigger to the engine through the event channel.
            let msg = "SYSTEM_SCHEDULER_EVENT: Routine check execution".to_string();

            if let Err(e) = event_tx.send(msg).await {
                eprintln!("[Scheduler] Event communication broken: {}", e);
                break;
            }
        }
    });
}
