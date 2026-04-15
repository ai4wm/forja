use super::routes::{build_router_with_status, default_telegram_status_provider};
use super::DashboardServer;
use axum::body::{to_bytes, Body};
use axum::http::Request;
use forja_core::traits::TelegramConnectionStatus;
use rusqlite::Connection;
use std::path::PathBuf;
use std::sync::Arc;
use tower::util::ServiceExt;

#[tokio::test]
async fn test_audit_api_returns_json() {
    let db_path = temp_db_path("audit");
    let connection = create_test_db(&db_path);
    connection
        .execute(
            "INSERT INTO audit_log (timestamp, event_type, agent_id, channel, payload, token_count)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![
                "2026-03-31T00:00:00Z",
                "tool_call",
                "default",
                "cli",
                r#"{"tool_name":"shell"}"#,
                42
            ],
        )
        .expect("audit row should insert");
    drop(connection);

    let response = build_router_with_status(db_path.clone(), default_telegram_status_provider())
        .oneshot(Request::builder().uri("/api/audit?limit=10").body(Body::empty()).unwrap())
        .await
        .expect("request should succeed");

    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(json.is_array());
    assert_eq!(json[0]["event_type"], "tool_call");

    cleanup(&db_path);
}

#[tokio::test]
async fn test_debate_list_api() {
    let db_path = temp_db_path("debates");
    let connection = create_test_db(&db_path);
    connection
        .execute(
            "INSERT INTO audit_log (timestamp, event_type, agent_id, channel, payload, token_count)
             VALUES (?1, 'debate_message', 'architect', 'cli', ?2, 12)",
            rusqlite::params![
                "2026-03-31T00:00:00Z",
                r#"{"role":"Architect","phase":"Diverge","round":1,"content":"Yes, and..."}"#
            ],
        )
        .expect("first debate row should insert");
    connection
        .execute(
            "INSERT INTO audit_log (timestamp, event_type, agent_id, channel, payload, token_count)
             VALUES (?1, 'debate_message', 'synthesizer', 'cli', ?2, 20)",
            rusqlite::params![
                "2026-03-31T00:00:05Z",
                r#"{"role":"Synthesis","phase":"Converge","round":1,"content":"Summary line"}"#
            ],
        )
        .expect("second debate row should insert");
    drop(connection);

    let response = build_router_with_status(db_path.clone(), default_telegram_status_provider())
        .oneshot(Request::builder().uri("/api/debates").body(Body::empty()).unwrap())
        .await
        .expect("request should succeed");

    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(json.is_array());
    assert_eq!(json[0]["message_count"], 2);

    cleanup(&db_path);
}

#[tokio::test]
async fn test_budget_api() {
    let db_path = temp_db_path("budget");
    let connection = create_test_db(&db_path);
    connection
        .execute(
            "INSERT INTO agent_budgets (agent_id, monthly_limit, used_tokens, month_key, warning_emitted)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params!["default", 50_000, 25_000, "2026-03", 0],
        )
        .expect("budget row should insert");
    drop(connection);

    let response = build_router_with_status(db_path.clone(), default_telegram_status_provider())
        .oneshot(Request::builder().uri("/api/budget").body(Body::empty()).unwrap())
        .await
        .expect("request should succeed");

    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(json.is_array());
    assert_eq!(json[0]["agent_id"], "default");
    assert_eq!(json[0]["percent"], 50);

    cleanup(&db_path);
}

#[tokio::test]
async fn test_channel_status_api() {
    let db_path = temp_db_path("channel-status");
    let connection = create_test_db(&db_path);
    drop(connection);

    let response = build_router_with_status(
        db_path.clone(),
        Arc::new(|| TelegramConnectionStatus::Reconnecting),
    )
    .oneshot(
        Request::builder()
            .uri("/api/channel-status")
            .body(Body::empty())
            .unwrap(),
    )
    .await
    .expect("request should succeed");

    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["telegram"], "Reconnecting");

    cleanup(&db_path);
}

#[test]
fn test_dashboard_server_stop_without_start() {
    let db_path = temp_db_path("server");
    let mut server = DashboardServer::new(3999, db_path.clone());
    server.stop();
    cleanup(&db_path);
}

fn temp_db_path(label: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "forja-dashboard-{label}-{}.db",
        rand::random::<u64>()
    ))
}

fn create_test_db(path: &PathBuf) -> Connection {
    let connection = Connection::open(path).expect("db should open");
    connection
        .execute(
            "CREATE TABLE audit_log (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                timestamp TEXT NOT NULL,
                event_type TEXT NOT NULL,
                agent_id TEXT DEFAULT 'default',
                channel TEXT,
                payload TEXT NOT NULL,
                token_count INTEGER DEFAULT 0
            )",
            [],
        )
        .expect("audit_log table should be created");
    connection
        .execute(
            "CREATE TABLE agent_budgets (
                agent_id TEXT PRIMARY KEY,
                monthly_limit INTEGER NOT NULL,
                used_tokens INTEGER NOT NULL DEFAULT 0,
                month_key TEXT NOT NULL,
                warning_emitted INTEGER NOT NULL DEFAULT 0
            )",
            [],
        )
        .expect("agent_budgets table should be created");
    connection
}

fn cleanup(path: &PathBuf) {
    let _ = std::fs::remove_file(path);
}
