mod assets;
mod audit;
mod autonomy;
mod budget;
mod chat;
mod debates;
mod memory;

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use forja_channel::dashboard_bridge::DashboardBridge;
use forja_core::traits::TelegramConnectionStatus;
use rusqlite::{Connection, OpenFlags};
use serde_json::json;
use std::path::PathBuf;
use std::sync::Arc;
use tower_http::cors::{Any, CorsLayer};

pub(crate) type TelegramStatusProvider = Arc<dyn Fn() -> TelegramConnectionStatus + Send + Sync>;

#[derive(Clone)]
pub(crate) struct DashboardState {
    pub(super) db_path: PathBuf,
    pub(super) telegram_status: TelegramStatusProvider,
    pub(super) dashboard_bridge: Option<DashboardBridge>,
}

#[cfg_attr(not(test), allow(dead_code))]
#[cfg(test)]
pub(crate) fn build_router_with_status(
    db_path: PathBuf,
    telegram_status: TelegramStatusProvider,
) -> Router {
    build_router(db_path, telegram_status, None)
}

pub(crate) fn build_router(
    db_path: PathBuf,
    telegram_status: TelegramStatusProvider,
    dashboard_bridge: Option<DashboardBridge>,
) -> Router {
    Router::new()
        .route("/", get(assets::index))
        .route("/assets/dashboard.css", get(assets::dashboard_css))
        .route("/assets/dashboard.js", get(assets::dashboard_js))
        .route("/api/audit", get(audit::get_audit))
        .route("/api/conversation", get(audit::get_conversation))
        .route("/api/debates", get(debates::get_debates))
        .route("/api/debate/:id", get(debates::get_debate))
        .route("/api/budget", get(budget::get_budget))
        .route("/api/skills", get(autonomy::get_skills))
        .route("/api/history", get(audit::get_history))
        .route("/api/tools", get(audit::get_tools))
        .route("/api/memory", get(memory::get_memory))
        .route("/api/memory/entries", get(memory::get_memory_entries))
        .route("/api/memory/summaries", get(memory::get_memory_summaries))
        .route("/api/events", get(audit::stream_events))
        .route("/api/chat", post(chat::post_chat))
        .route("/api/chat/stream", get(chat::stream_chat))
        .route("/api/unresolved", get(autonomy::get_unresolved))
        .route("/api/tasks", get(autonomy::get_tasks))
        .route("/api/channel-status", get(autonomy::get_channel_status))
        .route("/api/approve/:id", post(autonomy::approve_task))
        .with_state(DashboardState {
            db_path,
            telegram_status,
            dashboard_bridge,
        })
        .layer(CorsLayer::new().allow_origin(Any).allow_methods(Any))
}

pub(crate) fn default_telegram_status_provider() -> TelegramStatusProvider {
    Arc::new(|| TelegramConnectionStatus::Disconnected)
}

pub(super) fn open_read_only(db_path: &PathBuf) -> Result<Connection, DashboardError> {
    Connection::open_with_flags(db_path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(DashboardError::Db)
}

pub(super) fn table_exists(
    connection: &Connection,
    table_name: &str,
) -> Result<bool, DashboardError> {
    connection
        .query_row(
            "SELECT EXISTS(
                SELECT 1
                FROM sqlite_master
                WHERE type = 'table' AND name = ?1
            )",
            [table_name],
            |row| row.get::<_, i64>(0),
        )
        .map(|value| value != 0)
        .map_err(DashboardError::Db)
}

#[derive(Debug)]
pub(crate) enum DashboardError {
    Db(rusqlite::Error),
    Json(serde_json::Error),
    NotFound(String),
}

impl From<rusqlite::Error> for DashboardError {
    fn from(value: rusqlite::Error) -> Self {
        Self::Db(value)
    }
}

impl From<serde_json::Error> for DashboardError {
    fn from(value: serde_json::Error) -> Self {
        Self::Json(value)
    }
}

impl IntoResponse for DashboardError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            Self::Db(error) => (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()),
            Self::Json(error) => (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()),
            Self::NotFound(message) => (StatusCode::NOT_FOUND, message),
        };

        (status, Json(json!({ "error": message }))).into_response()
    }
}
