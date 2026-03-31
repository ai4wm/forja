use super::manager::BudgetManager;
use super::BudgetStatus;
use rusqlite::{params, Connection};
use std::path::PathBuf;

#[test]
fn test_register_agent_and_initial_status_is_ok() {
    let db_path = temp_db_path("initial");
    let manager = BudgetManager::new(&db_path).expect("manager should initialize");

    manager
        .register_agent("default", 50_000)
        .expect("agent should register");
    let status = manager
        .check_budget("default")
        .expect("status should be readable");

    assert_eq!(
        status,
        BudgetStatus::Ok {
            used: 0,
            limit: 50_000,
        }
    );

    cleanup(&db_path);
}

#[test]
fn test_record_usage_below_warning_remains_ok() {
    let db_path = temp_db_path("below-warning");
    let manager = BudgetManager::new(&db_path).expect("manager should initialize");

    manager
        .register_agent("default", 1_000)
        .expect("agent should register");
    let status = manager
        .record_usage("default", 790)
        .expect("usage should be recorded");

    assert_eq!(
        status,
        BudgetStatus::Ok {
            used: 790,
            limit: 1_000,
        }
    );

    cleanup(&db_path);
}

#[test]
fn test_record_usage_at_warning_threshold_returns_warning() {
    let db_path = temp_db_path("warning");
    let manager = BudgetManager::new(&db_path).expect("manager should initialize");

    manager
        .register_agent("default", 1_000)
        .expect("agent should register");
    let status = manager
        .record_usage("default", 800)
        .expect("usage should be recorded");

    assert_eq!(
        status,
        BudgetStatus::Warning {
            used: 800,
            limit: 1_000,
        }
    );

    cleanup(&db_path);
}

#[test]
fn test_record_usage_at_limit_returns_exceeded() {
    let db_path = temp_db_path("exceeded");
    let manager = BudgetManager::new(&db_path).expect("manager should initialize");

    manager
        .register_agent("default", 1_000)
        .expect("agent should register");
    let status = manager
        .record_usage("default", 1_000)
        .expect("usage should be recorded");

    assert_eq!(
        status,
        BudgetStatus::Exceeded {
            used: 1_000,
            limit: 1_000,
        }
    );

    cleanup(&db_path);
}

#[test]
fn test_month_rollover_resets_usage_to_zero() {
    let db_path = temp_db_path("rollover");
    let manager = BudgetManager::new(&db_path).expect("manager should initialize");

    manager
        .register_agent("default", 1_000)
        .expect("agent should register");
    manager
        .record_usage("default", 500)
        .expect("initial usage should be recorded");

    let connection = Connection::open(&db_path).expect("test db should open");
    connection
        .execute(
            "UPDATE agent_budgets SET month_key = ?1 WHERE agent_id = ?2",
            params!["2000-01", "default"],
        )
        .expect("month key should be updated");

    let status = manager
        .record_usage("default", 100)
        .expect("usage after rollover should be recorded");

    assert_eq!(
        status,
        BudgetStatus::Ok {
            used: 100,
            limit: 1_000,
        }
    );

    cleanup(&db_path);
}

#[test]
fn test_unregistered_agent_returns_error() {
    let db_path = temp_db_path("missing");
    let manager = BudgetManager::new(&db_path).expect("manager should initialize");

    let error = manager
        .check_budget("missing")
        .expect_err("unregistered agent should error");
    assert!(error.to_string().contains("missing"));

    cleanup(&db_path);
}

fn temp_db_path(label: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "forja-budget-{label}-{}.db",
        uuid::Uuid::new_v4()
    ))
}

fn cleanup(path: &PathBuf) {
    let _ = std::fs::remove_file(path);
}
