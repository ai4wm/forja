#[cfg(feature = "runtime")]
use tokio::sync::mpsc::Sender;
#[cfg(feature = "runtime")]
use tokio::time::{interval, Duration};

/// 시스템 스케줄러 (runtime 피처가 활성화된 경우에만 동작)
///
/// 주어진 `interval_secs` 주기로 백그라운드에서 동작하며,
/// `event_tx`를 통해 Engine의 이벤트 루프로 스케줄러 이벤트를 발송합니다.
#[cfg(feature = "runtime")]
pub async fn run_scheduler(event_tx: Sender<String>, interval_secs: u64) {
    let mut ticker = interval(Duration::from_secs(interval_secs));
    
    // 첫 번째 tick은 바로 실행되므로 의도한 주기를 위해 스킵하거나
    // 향후 로직에 따라 즉시 실행으로 냅둬도 됩니다.
    ticker.tick().await;

    tokio::spawn(async move {
        loop {
            ticker.tick().await;
            
            // 지정된 시간 도달 시 Event Channel을 통해 Engine에 트리거 신호 발송
            let msg = "SYSTEM_SCHEDULER_EVENT: Routine check execution".to_string();
            
            if let Err(e) = event_tx.send(msg).await {
                eprintln!("[Scheduler] Event communication broken: {}", e);
                break;
            }
        }
    });
}
