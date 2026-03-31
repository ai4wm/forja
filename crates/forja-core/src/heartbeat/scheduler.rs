use super::HeartbeatConfig;
use crate::error::Result;
use crate::gateway::{ChannelKind, Envelope, MessageType};
use chrono::Utc;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio::time::sleep;

pub struct HeartbeatScheduler {
    pub(super) configs: Vec<HeartbeatConfig>,
    pub(super) handles: Vec<JoinHandle<()>>,
}

impl Default for HeartbeatScheduler {
    fn default() -> Self {
        Self::new()
    }
}

impl HeartbeatScheduler {
    pub fn new() -> Self {
        Self {
            configs: Vec::new(),
            handles: Vec::new(),
        }
    }

    pub fn register(&mut self, config: HeartbeatConfig) {
        self.configs.push(config);
    }

    pub fn start_all(&mut self, sender: mpsc::Sender<Envelope>) -> Result<()> {
        if !self.handles.is_empty() {
            return Ok(());
        }

        for config in self.configs.iter().cloned() {
            if !config.enabled {
                continue;
            }

            let sender = sender.clone();
            self.handles.push(tokio::spawn(async move {
                loop {
                    sleep(config.interval).await;
                    let envelope = Envelope {
                        id: uuid::Uuid::new_v4().to_string(),
                        sender: config.agent_id.clone(),
                        text: "[heartbeat] checking for pending tasks".to_string(),
                        channel: ChannelKind::Internal,
                        timestamp: Utc::now(),
                        msg_type: MessageType::Heartbeat,
                    };

                    if sender.send(envelope).await.is_err() {
                        break;
                    }
                }
            }));
        }

        Ok(())
    }

    pub fn stop_all(&mut self) {
        for handle in self.handles.drain(..) {
            handle.abort();
        }
    }
}
