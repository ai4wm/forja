use super::Engine;
use crate::error::Result;
use crate::heartbeat::scheduler::HeartbeatScheduler;

impl Engine {
    pub fn with_heartbeat_scheduler(mut self, heartbeat_scheduler: HeartbeatScheduler) -> Self {
        self.heartbeat_scheduler = Some(heartbeat_scheduler);
        self
    }

    pub(super) fn start_heartbeat_runtime(&mut self) -> Result<()> {
        let Some(scheduler) = &mut self.heartbeat_scheduler else {
            return Ok(());
        };

        scheduler.start_all(self.heartbeat_sender.clone())?;
        Ok(())
    }

    pub(super) fn stop_heartbeat_runtime(&mut self) {
        if let Some(scheduler) = &mut self.heartbeat_scheduler {
            scheduler.stop_all();
        }
    }
}
