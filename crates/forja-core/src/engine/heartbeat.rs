use super::Engine;
use crate::audit::logger::AuditEvent;
use crate::error::Result;
use crate::heartbeat::scheduler::HeartbeatScheduler;
use serde_json::json;

impl Engine {
    pub fn with_heartbeat_scheduler(mut self, heartbeat_scheduler: HeartbeatScheduler) -> Self {
        self.heartbeat_scheduler = Some(heartbeat_scheduler);
        self
    }

    pub(super) fn start_heartbeat_runtime(&mut self) -> Result<()> {
        let Some(scheduler) = &mut self.heartbeat_scheduler else {
            return Ok(());
        };
        if self.heartbeat_sink_handle.is_some() {
            return Ok(());
        }

        scheduler.start_all(self.heartbeat_sender.clone())?;

        let Some(mut receiver) = self.heartbeat_receiver.take() else {
            return Ok(());
        };
        let audit_logger = self.audit_logger.clone();

        self.heartbeat_sink_handle = Some(tokio::spawn(async move {
            while let Some(envelope) = receiver.recv().await {
                let Some(audit_logger) = &audit_logger else {
                    continue;
                };

                let event = AuditEvent::new(
                    "heartbeat",
                    json!({
                        "text": envelope.text,
                        "message_type": "Heartbeat",
                    }),
                )
                .with_agent_id(envelope.sender)
                .with_channel("internal");
                let _ = audit_logger.log_event(event);
            }
        }));

        Ok(())
    }

    pub(super) fn stop_heartbeat_runtime(&mut self) {
        if let Some(handle) = self.heartbeat_sink_handle.take() {
            handle.abort();
        }
        if let Some(scheduler) = &mut self.heartbeat_scheduler {
            scheduler.stop_all();
        }
    }
}
