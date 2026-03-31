use super::{DashboardHandler, Engine};

impl Engine {
    pub fn with_dashboard_handler(mut self, handler: DashboardHandler) -> Self {
        self.dashboard_handler = Some(handler);
        self
    }
}
