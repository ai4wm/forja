use super::DashboardServer;
use super::routes::{build_router, build_router_with_status, default_telegram_status_provider};
use axum::body::{Body, to_bytes};
use axum::http::Request;
use forja_channel::dashboard_bridge::DashboardBridge;
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
        .oneshot(
            Request::builder()
                .uri("/api/audit?limit=10")
                .body(Body::empty())
                .unwrap(),
        )
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
        .oneshot(
            Request::builder()
                .uri("/api/debates")
                .body(Body::empty())
                .unwrap(),
        )
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
        .oneshot(
            Request::builder()
                .uri("/api/budget")
                .body(Body::empty())
                .unwrap(),
        )
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

#[tokio::test]
async fn test_history_api() {
    let db_path = temp_db_path("history");
    let connection = create_test_db(&db_path);
    connection
        .execute(
            "INSERT INTO audit_log (timestamp, event_type, agent_id, channel, payload, token_count)
             VALUES (?1, 'llm_call', 'default', 'cli', ?2, 10)",
            rusqlite::params!["2026-03-31T00:00:00Z", r#"{"mode":"chat"}"#],
        )
        .unwrap();
    drop(connection);

    let response = build_router_with_status(db_path.clone(), default_telegram_status_provider())
        .oneshot(
            Request::builder()
                .uri("/api/history")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json[0]["event_type"], "llm_call");

    cleanup(&db_path);
}

#[tokio::test]
async fn test_tools_api() {
    let db_path = temp_db_path("tools");
    let connection = create_test_db(&db_path);
    connection
        .execute(
            "INSERT INTO audit_log (timestamp, event_type, agent_id, channel, payload, token_count)
             VALUES (?1, 'tool_call', 'default', 'cli', ?2, 0)",
            rusqlite::params!["2026-03-31T00:00:00Z", r#"{"tool_name":"shell"}"#],
        )
        .unwrap();
    drop(connection);

    let response = build_router_with_status(db_path.clone(), default_telegram_status_provider())
        .oneshot(
            Request::builder()
                .uri("/api/tools")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json[0]["tool_name"], "shell");

    cleanup(&db_path);
}

#[tokio::test]
async fn test_memory_api_without_memory_db_returns_zero_counts() {
    let db_path = temp_db_path("memory");
    let connection = create_test_db(&db_path);
    drop(connection);

    let response = build_router_with_status(db_path.clone(), default_telegram_status_provider())
        .oneshot(
            Request::builder()
                .uri("/api/memory")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["memory_entries"], 0);
    assert_eq!(json["memory_summaries"], 0);

    cleanup(&db_path);
}

#[tokio::test]
async fn test_memory_browser_routes_return_entries_and_summaries() {
    let db_path = temp_db_path("memory-browser");
    let connection = create_test_db(&db_path);
    drop(connection);

    let memory_dir = db_path
        .parent()
        .expect("audit db should have parent")
        .join("memory");
    std::fs::create_dir_all(&memory_dir).unwrap();
    let memory_db_path = memory_dir.join("memory.db");
    let memory_connection = Connection::open(&memory_db_path).unwrap();
    memory_connection
        .execute_batch(
            "CREATE TABLE memory_entries (
                id TEXT PRIMARY KEY,
                timestamp INTEGER NOT NULL,
                role TEXT NOT NULL,
                content TEXT NOT NULL,
                source TEXT NOT NULL
            );
            CREATE VIRTUAL TABLE memory_entries_fts USING fts5(id, role, content, source);
            CREATE TABLE memory_summaries (
                source TEXT PRIMARY KEY,
                summary TEXT NOT NULL,
                created_at INTEGER NOT NULL
            );
            CREATE VIRTUAL TABLE memory_summaries_fts USING fts5(source, summary);",
        )
        .unwrap();
    memory_connection
        .execute(
            "INSERT INTO memory_entries (id, timestamp, role, content, source)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![
                "entry-1",
                1_700_000_000_i64,
                "user",
                "desktop memory",
                "live/2026-04-22"
            ],
        )
        .unwrap();
    memory_connection
        .execute(
            "INSERT INTO memory_entries_fts (id, role, content, source)
             VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params!["entry-1", "user", "desktop memory", "live/2026-04-22"],
        )
        .unwrap();
    memory_connection
        .execute(
            "INSERT INTO memory_summaries (source, summary, created_at)
             VALUES (?1, ?2, ?3)",
            rusqlite::params!["topic/desktop", "desktop summary", 1_700_000_001_i64],
        )
        .unwrap();
    memory_connection
        .execute(
            "INSERT INTO memory_summaries_fts (source, summary)
             VALUES (?1, ?2)",
            rusqlite::params!["topic/desktop", "desktop summary"],
        )
        .unwrap();
    drop(memory_connection);

    let app = build_router_with_status(db_path.clone(), default_telegram_status_provider());
    let entries_response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/memory/entries?q=desktop")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let entries_body = to_bytes(entries_response.into_body(), usize::MAX)
        .await
        .unwrap();
    let entries_json: serde_json::Value = serde_json::from_slice(&entries_body).unwrap();
    assert_eq!(entries_json[0]["id"], "entry-1");

    let summaries_response = app
        .oneshot(
            Request::builder()
                .uri("/api/memory/summaries?q=desktop")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let summaries_body = to_bytes(summaries_response.into_body(), usize::MAX)
        .await
        .unwrap();
    let summaries_json: serde_json::Value = serde_json::from_slice(&summaries_body).unwrap();
    assert_eq!(summaries_json[0]["source"], "topic/desktop");

    cleanup(&db_path);
}

#[tokio::test]
async fn test_chat_post_enqueues_dashboard_message() {
    let db_path = temp_db_path("chat");
    let connection = create_test_db(&db_path);
    drop(connection);

    let (input_tx, mut input_rx) = tokio::sync::mpsc::channel::<String>(1);
    let (event_tx, _) = tokio::sync::broadcast::channel(4);
    let bridge = DashboardBridge::new(input_tx, event_tx);
    let app = build_router(
        db_path.clone(),
        default_telegram_status_provider(),
        Some(bridge),
    );

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/chat")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"text":"hello desktop"}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["ok"], true);
    assert_eq!(input_rx.recv().await.as_deref(), Some("hello desktop"));

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
    let base =
        std::env::temp_dir().join(format!("forja-dashboard-{label}-{}", rand::random::<u64>()));
    std::fs::create_dir_all(&base).expect("temp dashboard dir should be created");
    base.join("audit.db")
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
    if let Some(parent) = path.parent() {
        let _ = std::fs::remove_dir_all(parent);
    }
}
