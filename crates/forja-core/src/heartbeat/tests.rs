use super::scheduler::HeartbeatScheduler;
use super::HeartbeatConfig;
use crate::gateway::{ChannelKind, MessageType};
use std::time::Duration;

#[test]
fn test_register_config_stores_it() {
    let mut scheduler = HeartbeatScheduler::new();
    scheduler.register(HeartbeatConfig {
        agent_id: "default".to_string(),
        interval: Duration::from_secs(60),
        enabled: true,
    });

    assert_eq!(scheduler.configs.len(), 1);
    assert_eq!(scheduler.configs[0].agent_id, "default");
}

#[tokio::test(start_paused = true)]
async fn test_start_all_with_disabled_config_spawns_no_tasks() {
    let mut scheduler = HeartbeatScheduler::new();
    scheduler.register(HeartbeatConfig {
        agent_id: "default".to_string(),
        interval: Duration::from_secs(60),
        enabled: false,
    });
    let (sender, mut receiver) = tokio::sync::mpsc::channel(4);

    scheduler
        .start_all(sender)
        .expect("disabled scheduler should start cleanly");

    assert!(scheduler.handles.is_empty());
    assert!(receiver.try_recv().is_err());
}

#[tokio::test(start_paused = true)]
async fn test_start_all_with_enabled_config_sends_heartbeat() {
    let mut scheduler = HeartbeatScheduler::new();
    scheduler.register(HeartbeatConfig {
        agent_id: "agent-1".to_string(),
        interval: Duration::from_secs(5),
        enabled: true,
    });
    let (sender, mut receiver) = tokio::sync::mpsc::channel(4);

    scheduler
        .start_all(sender)
        .expect("enabled scheduler should start cleanly");
    tokio::time::advance(Duration::from_secs(5)).await;

    let envelope = receiver.recv().await.expect("heartbeat should be sent");
    assert_eq!(envelope.sender, "agent-1");
    assert_eq!(envelope.channel, ChannelKind::Internal);
    assert_eq!(envelope.msg_type, MessageType::Heartbeat);

    scheduler.stop_all();
}
