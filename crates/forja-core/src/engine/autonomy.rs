use super::Engine;
use crate::autonomy::AutonomyAction;
use crate::error::{ForjaError, Result};
use chrono::Utc;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

struct TaskExecutionOutcome {
    spec_id: String,
    duration_secs: u64,
    files_changed: Vec<String>,
    test_results: Vec<String>,
    committed: bool,
}

impl Engine {
    pub fn with_autonomy(mut self, loop_runner: crate::autonomy::loop_runner::AutonomousLoop) -> Self {
        self.autonomy = Some(loop_runner);
        self
    }

    pub(super) async fn handle_autonomy_tick(&mut self) -> Result<()> {
        let Some(autonomy) = self.autonomy.clone() else {
            return Ok(());
        };

        let actions = autonomy.tick()?;
        for action in actions {
            self.log_autonomy_action(&action);
            match action {
                AutonomyAction::ExecuteTask { task } => {
                    let task = *task;
                    let started = autonomy.mark_task_started(task.id)?;
                    let started_at = std::time::Instant::now();
                    self.emit_autonomy_notification(
                        &autonomy,
                        "task started",
                        &format!("Task #{} started: {}", started.id, started.description),
                    )
                    .await?;

                    match self.execute_autonomy_task(&started, started_at).await {
                        Ok(outcome) => {
                            let summary = format!(
                                "spec={} duration={}s committed={} tests={}",
                                outcome.spec_id,
                                outcome.duration_secs,
                                outcome.committed,
                                outcome.test_results.join("; ")
                            );
                            let completed = autonomy.mark_task_completed(task.id, summary.clone())?;
                            self.emit_autonomy_notification(
                                &autonomy,
                                "task completed",
                                &format!(
                                    "Task #{} completed: {}\nDuration: {}s\nFiles changed: {}\nTests: {}",
                                    completed.id,
                                    completed.description,
                                    outcome.duration_secs,
                                    format_files(&outcome.files_changed),
                                    outcome.test_results.join("; ")
                                ),
                            )
                            .await?;
                        }
                        Err(error) => {
                            let failed = autonomy.record_task_failure(task.id, &error.to_string())?;
                            if failed.status == "failed" {
                                autonomy.add_unresolved(&task.description, &error.to_string())?;
                                self.emit_autonomy_notification(
                                    &autonomy,
                                    "task failed",
                                    &format!(
                                        "Task #{} failed: {}\nError: {}",
                                        failed.id, failed.description, error
                                    ),
                                )
                                .await?;
                            }
                        }
                    }
                }
                AutonomyAction::QueueEmpty => {
                    self.emit_autonomy_notification(
                        &autonomy,
                        "queue empty",
                        "Autonomy queue is empty.",
                    )
                    .await?;
                }
            }
        }

        Ok(())
    }

    pub(super) fn handle_task_command(&self, description: &str) -> Result<String> {
        let Some(autonomy) = &self.autonomy else {
            return Err(ForjaError::Internal("autonomy is not configured".to_string()));
        };

        let trimmed = description.trim();
        if let Some(task_description) = trimmed.strip_prefix("add ") {
            let task_id = autonomy.enqueue_task(task_description.trim(), "user")?;
            return Ok(format!("Queued task #{task_id}: {}", task_description.trim()));
        }
        if trimmed == "list" {
            let tasks = autonomy.list_tasks()?;
            if tasks.is_empty() {
                return Ok("No queued tasks.".to_string());
            }
            return Ok(tasks
                .into_iter()
                .map(|task| format!("- #{} {} [{}]", task.id, task.description, task.status))
                .collect::<Vec<_>>()
                .join("\n"));
        }
        if let Some(task_id) = trimmed.strip_prefix("cancel ") {
            let task_id = task_id
                .trim()
                .parse::<i64>()
                .map_err(|error| ForjaError::Internal(format!("Invalid task id: {error}")))?;
            autonomy.cancel_task(task_id)?;
            return Ok(format!("Cancelled task #{task_id}."));
        }

        let task_id = autonomy.enqueue_task(trimmed, "user")?;
        Ok(format!("Queued task #{task_id}: {trimmed}"))
    }

    pub(super) fn handle_autonomy_command(&self, command: &str) -> Result<String> {
        let Some(autonomy) = &self.autonomy else {
            return Err(ForjaError::Internal("autonomy is not configured".to_string()));
        };

        match command.trim() {
            "start" => {
                autonomy.start()?;
                Ok("Autonomy mode started.".to_string())
            }
            "stop" => {
                autonomy.request_stop()?;
                Ok("Autonomy mode will stop after the current task.".to_string())
            }
            "status" => Ok(autonomy.status_summary()),
            _ => Ok("Usage: /autonomy <start|stop|status>".to_string()),
        }
    }

    pub(super) fn handle_skills_command(&self) -> Result<String> {
        let Some(autonomy) = &self.autonomy else {
            return Err(ForjaError::Internal("autonomy is not configured".to_string()));
        };

        let skills = autonomy.skill_registry.list_skills()?;
        if skills.is_empty() {
            return Ok("No skills recorded yet.".to_string());
        }

        Ok(skills
            .into_iter()
            .map(|skill| {
                format!(
                    "- {} | success={} | auto_approved={}",
                    skill.tool_name,
                    skill.success_count,
                    skill.auto_approved
                )
            })
            .collect::<Vec<_>>()
            .join("\n"))
    }

    pub(super) fn handle_unresolved_command(&self) -> Result<String> {
        let Some(autonomy) = &self.autonomy else {
            return Err(ForjaError::Internal("autonomy is not configured".to_string()));
        };

        let tasks = autonomy.unresolved_store.list_all()?;
        if tasks.is_empty() {
            return Ok("No unresolved tasks.".to_string());
        }

        Ok(tasks
            .into_iter()
            .map(|task| {
                format!(
                    "- #{} {} | retry {}/{} | status={}",
                    task.id,
                    task.task,
                    task.retry_count,
                    task.max_retries,
                    task.status
                )
            })
            .collect::<Vec<_>>()
            .join("\n"))
    }

    async fn execute_autonomy_task(
        &self,
        task: &crate::autonomy::QueuedTask,
        started_at: std::time::Instant,
    ) -> Result<TaskExecutionOutcome> {
        let workspace = std::env::current_dir()
            .map_err(|error| ForjaError::Internal(format!("workspace detection failed: {error}")))?;
        let (spec_id, test_results) = execute_task_pipeline(task, &workspace)?;
        let files_changed = git_lines(&workspace, &["diff", "--name-only"])?;
        let commit_created = maybe_autonomy_commit(&workspace, &spec_id, &files_changed, &test_results)?;

        Ok(TaskExecutionOutcome {
            spec_id,
            duration_secs: started_at.elapsed().as_secs(),
            files_changed,
            test_results,
            committed: commit_created,
        })
    }

    async fn emit_autonomy_notification(
        &self,
        autonomy: &crate::autonomy::loop_runner::AutonomousLoop,
        label: &str,
        message: &str,
    ) -> Result<()> {
        let sent = self.channel.send_notification(message).await.unwrap_or(false);
        if !sent {
            let timestamp = Utc::now().to_rfc3339();
            autonomy.append_notification_log(&format!("[{timestamp}] {label}: {message}"))?;
        }
        Ok(())
    }
}

fn execute_task_pipeline(
    task: &crate::autonomy::QueuedTask,
    workspace: &Path,
) -> Result<(String, Vec<String>)> {
    let mut test_results = Vec::new();
    let baseline_dirty = git_lines(workspace, &["status", "--porcelain"])?;
    let spec_id = resolve_or_plan_spec(task, workspace)?;

    run_checked_command(workspace, "auto", &["go", &spec_id, "--solo", "--auto"])?;
    run_checked_command(workspace, "cargo", &["build", "--workspace"])?;
    test_results.push("cargo build --workspace=pass".to_string());
    run_checked_command(workspace, "cargo", &["clippy", "--workspace", "--", "-D", "warnings"])?;
    test_results.push("cargo clippy --workspace -- -D warnings=pass".to_string());
    run_checked_command(workspace, "cargo", &["test", "-p", "forja-llm"])?;
    test_results.push("cargo test -p forja-llm=pass".to_string());
    run_checked_command(workspace, "cargo", &["test", "-p", "forja-llm", "--", "--ignored"])?;
    test_results.push("cargo test -p forja-llm -- --ignored=pass".to_string());
    if !baseline_dirty.is_empty() {
        test_results.push("auto-commit=skipped (dirty worktree before task)".to_string());
    }

    Ok((spec_id, test_results))
}

fn resolve_or_plan_spec(task: &crate::autonomy::QueuedTask, workspace: &Path) -> Result<String> {
    if let Some(task_ref) = &task.task_ref {
        if task_ref.starts_with("SPEC-") {
            return Ok(task_ref.clone());
        }
        if let Some(spec_id) = spec_id_from_path(task_ref) {
            return Ok(spec_id);
        }
    }

    let before = list_spec_dirs(workspace)?;
    run_checked_command(workspace, "auto", &["plan", &task.description, "--solo"])?;
    let after = list_spec_dirs(workspace)?;
    let created = after
        .into_iter()
        .find(|spec_id| !before.contains(spec_id))
        .ok_or_else(|| {
            ForjaError::Internal("auto plan completed but no new SPEC directory was detected".to_string())
        })?;
    Ok(created)
}

fn spec_id_from_path(path: &str) -> Option<String> {
    let path = PathBuf::from(path);
    if !path.exists() {
        return None;
    }
    let raw = fs::read_to_string(path).ok()?;
    raw.lines()
        .find_map(|line| line.strip_prefix("id: ").map(str::trim))
        .map(str::to_string)
}

fn list_spec_dirs(workspace: &Path) -> Result<Vec<String>> {
    let specs_dir = workspace.join(".autopus").join("specs");
    if !specs_dir.exists() {
        return Ok(Vec::new());
    }
    let mut entries = fs::read_dir(specs_dir)
        .map_err(|error| ForjaError::Storage(error.to_string()))?
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.path().is_dir())
        .filter_map(|entry| entry.file_name().to_str().map(str::to_string))
        .collect::<Vec<_>>();
    entries.sort();
    Ok(entries)
}

fn run_checked_command(workspace: &Path, program: &str, args: &[&str]) -> Result<()> {
    let output = Command::new(program)
        .args(args)
        .current_dir(workspace)
        .output()
        .map_err(|error| {
            ForjaError::Internal(format!("failed to execute {program} {}: {error}", args.join(" ")))
        })?;

    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    Err(ForjaError::Internal(format!(
        "{program} {} failed: {stderr}",
        args.join(" ")
    )))
}

fn git_lines(workspace: &Path, args: &[&str]) -> Result<Vec<String>> {
    let output = Command::new("git")
        .args(args)
        .current_dir(workspace)
        .output()
        .map_err(|error| ForjaError::Internal(format!("git {} failed: {error}", args.join(" "))))?;
    if !output.status.success() {
        return Ok(Vec::new());
    }
    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect())
}

fn maybe_autonomy_commit(
    workspace: &Path,
    spec_id: &str,
    files_changed: &[String],
    test_results: &[String],
) -> Result<bool> {
    if test_results
        .iter()
        .any(|result| result.contains("auto-commit=skipped"))
    {
        return Ok(false);
    }

    let changed = git_lines(workspace, &["status", "--porcelain"])?;
    if changed.is_empty() {
        return Ok(false);
    }

    run_checked_command(workspace, "git", &["add", "-A"])?;
    let message = build_lore_commit_message(spec_id, files_changed, test_results);
    run_checked_command(workspace, "git", &["commit", "-m", &message])?;
    Ok(true)
}

fn build_lore_commit_message(spec_id: &str, files_changed: &[String], test_results: &[String]) -> String {
    format!(
        "feat(autonomy): complete {spec_id}\n\nAutonomous execution completed for {spec_id}.\nFiles changed: {}\n\nConstraint: auto-commit only after passing build, clippy, and tests\nConfidence: medium\nScope-risk: system\nReversibility: moderate\nDirective: review unattended autonomy behavior before enabling wider task categories\nTested: {}\nRelated: {spec_id}\n\n🐙 Autopus <noreply@autopus.co>",
        format_files(files_changed),
        test_results.join("; "),
    )
}

fn format_files(files_changed: &[String]) -> String {
    if files_changed.is_empty() {
        return "none".to_string();
    }
    files_changed.join(", ")
}
