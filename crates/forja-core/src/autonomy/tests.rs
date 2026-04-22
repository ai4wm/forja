use super::AutonomyConfig;
use super::loop_runner::AutonomousLoop;
use super::skills::SkillRegistry;
use super::unresolved::UnresolvedStore;
use crate::creation::{DebateMessage, DebatePhase, DebateResult, DebateRunMetadata, TaskItem};
use serde_json::Value;
use std::path::PathBuf;

#[test]
fn test_skill_promotion() {
    let db_path = temp_db_path("skill-promotion");
    let registry = SkillRegistry::new(&db_path).expect("registry should initialize");

    for _ in 0..5 {
        registry
            .record_success("shell")
            .expect("success should record");
    }

    let promoted = registry
        .check_and_promote("shell", 5)
        .expect("promotion should check");
    assert!(promoted);
    assert!(registry.is_auto_approved("shell").unwrap());

    cleanup(&db_path);
}

#[test]
fn test_skill_not_promoted() {
    let db_path = temp_db_path("skill-not-promoted");
    let registry = SkillRegistry::new(&db_path).expect("registry should initialize");

    for _ in 0..4 {
        registry
            .record_success("shell")
            .expect("success should record");
    }

    let promoted = registry
        .check_and_promote("shell", 5)
        .expect("promotion should check");
    assert!(!promoted);
    assert!(!registry.is_auto_approved("shell").unwrap());

    cleanup(&db_path);
}

#[test]
fn test_unresolved_retry() {
    let db_path = temp_db_path("unresolved-retry");
    let store = UnresolvedStore::new(&db_path).expect("store should initialize");
    store
        .add("shell {\"command\":\"Get-Date\"}", "error", 3)
        .expect("task should insert");

    let pending = store.get_pending().expect("pending should load");
    store
        .increment_retry(pending[0].id)
        .expect("retry should increment");
    let all = store.list_all().expect("all should load");
    assert_eq!(all[0].retry_count, 1);

    cleanup(&db_path);
}

#[test]
fn test_unresolved_max_retries() {
    let db_path = temp_db_path("unresolved-fail");
    let store = UnresolvedStore::new(&db_path).expect("store should initialize");
    store
        .add("shell {\"command\":\"Get-Date\"}", "error", 3)
        .expect("task should insert");

    let task = store.get_pending().unwrap().remove(0);
    store.increment_retry(task.id).unwrap();
    store.increment_retry(task.id).unwrap();
    store.mark_failed(task.id).unwrap();
    let all = store.list_all().unwrap();
    assert_eq!(all[0].status, "failed");

    cleanup(&db_path);
}

#[test]
fn test_task_enqueue_and_list() {
    let db_path = temp_db_path("task-enqueue");
    let loop_runner =
        AutonomousLoop::new(AutonomyConfig::default(), &db_path).expect("loop should initialize");
    loop_runner
        .enqueue_task("shell {\"command\":\"Get-Date\"}", "user")
        .expect("task should enqueue");

    let tasks = loop_runner.get_pending_tasks().expect("tasks should load");
    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0].source, "user");

    cleanup(&db_path);
}

#[test]
fn test_enqueue_from_debate() {
    let db_path = temp_db_path("task-from-debate");
    let loop_runner =
        AutonomousLoop::new(AutonomyConfig::default(), &db_path).expect("loop should initialize");
    let debate_result = DebateResult {
        summary: "summary".to_string(),
        task_list: vec![
            TaskItem {
                name: "Add gateway adapter".to_string(),
                assigned_role: "Architecture".to_string(),
                estimated_hours: 2.0,
                priority: 1,
            },
            TaskItem {
                name: "Implement auth flow".to_string(),
                assigned_role: "Build".to_string(),
                estimated_hours: 4.0,
                priority: 2,
            },
        ],
        transcript: vec![DebateMessage {
            agent_id: "architect".to_string(),
            role: "Architect".to_string(),
            phase: DebatePhase::Diverge,
            round: 1,
            content: "Yes, and...".to_string(),
            tokens: 3,
        }],
        total_tokens: 3,
        total_rounds: 1,
        metadata: DebateRunMetadata {
            active_agent_count: 1,
            diverge_rounds: 1,
            conflict_rounds: 0,
            combination_rounds: 0,
            mutation_rounds: 0,
            converge_rounds: 0,
        },
        active_agent_count: 1,
    };

    let ids = loop_runner
        .enqueue_from_debate(&debate_result)
        .expect("debate tasks should enqueue");
    assert_eq!(ids.len(), 2);
    assert_eq!(loop_runner.get_pending_tasks().unwrap().len(), 2);

    cleanup(&db_path);
}

#[test]
fn test_task_queue_is_persisted_to_queue_json_in_fifo_order() {
    let db_path = temp_db_path("queue-json");
    let loop_runner =
        AutonomousLoop::new(AutonomyConfig::default(), &db_path).expect("loop should initialize");

    loop_runner
        .enqueue_task("SPEC-RUNTIME-001", "user")
        .expect("first task should enqueue");
    loop_runner
        .enqueue_task("Refactor telemetry output", "telegram")
        .expect("second task should enqueue");

    let queue_path = db_path
        .parent()
        .unwrap()
        .join(db_path.file_stem().unwrap())
        .join("tasks")
        .join("queue.json");
    let raw = std::fs::read_to_string(&queue_path).expect("queue.json should exist");
    let queue: Value = serde_json::from_str(&raw).expect("queue.json should be valid json");
    let tasks = queue["tasks"].as_array().expect("tasks should be an array");

    assert_eq!(tasks.len(), 2);
    assert_eq!(tasks[0]["description"], "SPEC-RUNTIME-001");
    assert_eq!(tasks[1]["description"], "Refactor telemetry output");
    assert_eq!(tasks[0]["status"], "pending");

    cleanup(&db_path);
}

#[test]
fn test_mark_task_started_writes_current_checkpoint_file() {
    let db_path = temp_db_path("current-json");
    let loop_runner =
        AutonomousLoop::new(AutonomyConfig::default(), &db_path).expect("loop should initialize");
    let task_id = loop_runner
        .enqueue_task("SPEC-CHANNEL-001", "user")
        .expect("task should enqueue");

    loop_runner
        .mark_task_started(task_id)
        .expect("task should mark started");

    let current_path = db_path
        .parent()
        .unwrap()
        .join(db_path.file_stem().unwrap())
        .join("tasks")
        .join("current.json");
    let raw = std::fs::read_to_string(&current_path).expect("current.json should exist");
    let current: Value = serde_json::from_str(&raw).expect("current.json should be valid json");

    assert_eq!(current["task"]["id"], task_id);
    assert_eq!(current["task"]["status"], "running");

    cleanup(&db_path);
}

#[test]
fn test_autonomy_mode_start_stop_and_status_are_tracked() {
    let db_path = temp_db_path("mode-state");
    let loop_runner =
        AutonomousLoop::new(AutonomyConfig::default(), &db_path).expect("loop should initialize");

    assert!(!loop_runner.is_active());
    loop_runner.start().expect("autonomy should start");
    assert!(loop_runner.is_active());
    assert!(loop_runner.status_summary().contains("running=false"));
    loop_runner
        .request_stop()
        .expect("stop should be requested");
    assert!(loop_runner.stop_requested());

    cleanup(&db_path);
}

#[test]
fn test_queue_empty_notification_is_emitted_once_per_empty_state() {
    let db_path = temp_db_path("queue-empty-once");
    let loop_runner =
        AutonomousLoop::new(AutonomyConfig::default(), &db_path).expect("loop should initialize");
    loop_runner.start().expect("autonomy should start");

    let first = loop_runner.tick().expect("first tick should succeed");
    let second = loop_runner.tick().expect("second tick should succeed");

    assert_eq!(first, vec![super::AutonomyAction::QueueEmpty]);
    assert!(second.is_empty());

    cleanup(&db_path);
}

#[test]
fn test_restart_repairs_current_json_into_pending_queue_and_reactivates_mode() {
    let db_path = temp_db_path("restart-repair");
    let loop_runner =
        AutonomousLoop::new(AutonomyConfig::default(), &db_path).expect("loop should initialize");
    let task_id = loop_runner
        .enqueue_task("SPEC-RUNTIME-001", "user")
        .expect("task should enqueue");
    loop_runner
        .mark_task_started(task_id)
        .expect("task should mark started");

    let restarted = AutonomousLoop::new(AutonomyConfig::default(), &db_path)
        .expect("loop should initialize again");
    let pending = restarted.get_pending_tasks().expect("pending should load");

    assert!(restarted.is_active());
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].id, task_id);
    assert_eq!(pending[0].status, "pending");

    cleanup(&db_path);
}

fn temp_db_path(label: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "forja-autonomy-{label}-{}.db",
        uuid::Uuid::new_v4()
    ))
}

fn cleanup(path: &PathBuf) {
    let _ = std::fs::remove_file(path);
    let _ = std::fs::remove_dir_all(
        path.parent()
            .unwrap_or_else(|| std::path::Path::new("."))
            .join(path.file_stem().unwrap_or_default()),
    );
}
