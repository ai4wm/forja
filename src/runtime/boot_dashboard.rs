use crate::bootstrap::BootstrapPaths;
use crate::config::ForjaConfig;
use crate::dashboard::DashboardServer;
use crate::dashboard::routes::TelegramStatusProvider;
use forja_channel::dashboard_bridge::DashboardBridge;
use forja_core::engine::{DashboardHandler, TuiHandler};
use forja_core::error::ForjaError;
use std::sync::{Arc, Mutex};

pub(crate) struct DashboardBundle {
    pub(crate) dashboard_server: Arc<Mutex<DashboardServer>>,
    pub(crate) dashboard_handler: DashboardHandler,
    pub(crate) tui_handler: TuiHandler,
}

pub(crate) fn build_dashboard_bundle(
    forja_cfg: &ForjaConfig,
    bootstrap_paths: &BootstrapPaths,
    telegram_status_provider: TelegramStatusProvider,
    dashboard_bridge: DashboardBridge,
) -> DashboardBundle {
    let audit_db_path = bootstrap_paths.forja_dir.join("audit.db");
    let dashboard_server = Arc::new(Mutex::new(
        DashboardServer::new(forja_cfg.dashboard.port, audit_db_path)
            .with_telegram_status(telegram_status_provider)
            .with_dashboard_bridge(dashboard_bridge),
    ));
    let dashboard_server_for_handler = dashboard_server.clone();
    let dashboard_handler: DashboardHandler = Arc::new(move || {
        let mut server = dashboard_server_for_handler
            .lock()
            .map_err(|error| ForjaError::Internal(error.to_string()))?;
        server.start()
    });
    let audit_db_path = bootstrap_paths.forja_dir.join("audit.db");
    let memory_db_path = bootstrap_paths.forja_dir.join("memory").join("memory.db");
    let tui_handler: TuiHandler =
        Arc::new(move || crate::tui::launch_tui(&audit_db_path, &memory_db_path));

    DashboardBundle {
        dashboard_server,
        dashboard_handler,
        tui_handler,
    }
}
