pub mod scheduler;

use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeartbeatConfig {
    pub agent_id: String,
    pub interval: Duration,
    pub enabled: bool,
}

#[cfg(test)]
mod tests;
