use super::logger::{AuditEvent, AuditLogger};
use serde_json::json;
use std::path::PathBuf;

#[test]
fn test_log_event_and_query_it_back() {
    let db_path = temp_db_path("single");
    let logger = AuditLogger::new(&db_path).expect("logger should initialize");
    let event = AuditEvent::new("tool_call", json!({ "tool_name": "shell" }));

    logger.log_event(event.clone()).expect("event should be logged");
    let events = logger.query_recent(10).expect("query should succeed");

    assert_eq!(events.len(), 1);
    assert_eq!(events[0].event_type, "tool_call");
    assert_eq!(events[0].payload, event.payload);

    cleanup(&db_path);
}

#[test]
fn test_multiple_events_maintain_insertion_order() {
    let db_path = temp_db_path("multiple");
    let logger = AuditLogger::new(&db_path).expect("logger should initialize");

    logger
        .log_event(AuditEvent::new("llm_call", json!({ "step": 1 })))
        .expect("first event should be logged");
    logger
        .log_event(AuditEvent::new("retry", json!({ "step": 2 })))
        .expect("second event should be logged");

    let events = logger.query_recent(10).expect("query should succeed");
    assert_eq!(events.len(), 2);
    assert_eq!(events[0].event_type, "llm_call");
    assert_eq!(events[1].event_type, "retry");

    cleanup(&db_path);
}

#[test]
fn test_empty_db_returns_empty_vec() {
    let db_path = temp_db_path("empty");
    let logger = AuditLogger::new(&db_path).expect("logger should initialize");

    let events = logger.query_recent(5).expect("query should succeed");
    assert!(events.is_empty());

    cleanup(&db_path);
}

fn temp_db_path(label: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "forja-audit-{label}-{}.db",
        uuid::Uuid::new_v4()
    ))
}

fn cleanup(path: &PathBuf) {
    let _ = std::fs::remove_file(path);
}
