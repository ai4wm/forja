use super::Engine;
use crate::autonomy::AutonomyTarget;
use crate::autonomy::AutonomyAction;
use crate::error::{ForjaError, Result};
use crate::traits::{NotificationLevel, NotificationTopic};
use chrono::Utc;
use serde::Deserialize;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

#[cfg_attr(not(feature = "runtime"), allow(dead_code))]
struct TaskExecutionOutcome {
    spec_id: String,
    route_label: String,
    duration_secs: u64,
    files_changed: Vec<String>,
    test_results: Vec<String>,
    committed: bool,
}

#[derive(Debug, Clone)]
#[cfg_attr(not(feature = "runtime"), allow(dead_code))]
struct TaskExecutionPlan {
    route_label: String,
    env_overrides: Vec<(String, String)>,
    fallback_cloud: Option<AutonomyTarget>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(not(feature = "runtime"), allow(dead_code))]
enum RouteKind {
    Local,
    Cloud,
}

#[derive(Debug, Clone)]
#[cfg_attr(not(feature = "runtime"), allow(dead_code))]
struct RouteDecision {
    route: RouteKind,
    reason: String,
}

#[derive(Debug, Deserialize)]
#[cfg_attr(not(feature = "runtime"), allow(dead_code))]
struct RouteDecisionResponse {
    route: String,
    reason: String,
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
                                    "Task #{} completed: {}\nRoute: {}\nDuration: {}s\nFiles changed: {}\nTests: {}",
                                    completed.id,
                                    completed.description,
                                    outcome.route_label,
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

    pub(super) fn handle_learned_tool_skills(&self) -> Result<String> {
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
        let plan = self.select_autonomy_execution_plan(task).await?;
        let (spec_id, mut test_results) = match execute_task_pipeline(
            task,
            &workspace,
            &plan.env_overrides,
            self.mode_state.exec_mode.as_str(),
        ) {
            Ok(result) => result,
            Err(error) => {
                if let Some(cloud_target) = &plan.fallback_cloud
                    && self.should_escalate_after_local_failure(task, &error)
                    && self.confirm_cloud_escalation(task, cloud_target)?
                {
                    self.check_current_agent_budget()?;
                    let cloud_overrides = route_env_overrides(cloud_target, self.mode_state.exec_mode.as_str());
                    let (spec_id, mut test_results) =
                        execute_task_pipeline(task, &workspace, &cloud_overrides, self.mode_state.exec_mode.as_str())?;
                    test_results.push(format!("route=cloud-escalated ({})", cloud_target.label));
                    let files_changed = git_lines(&workspace, &["diff", "--name-only"])?;
                    let commit_created =
                        maybe_autonomy_commit(&workspace, &spec_id, &files_changed, &test_results)?;
                    return Ok(TaskExecutionOutcome {
                        spec_id,
                        route_label: format!("cloud-escalated ({})", cloud_target.label),
                        duration_secs: started_at.elapsed().as_secs(),
                        files_changed,
                        test_results,
                        committed: commit_created,
                    });
                }
                return Err(error);
            }
        };
        test_results.push(format!("route={}", plan.route_label));
        let files_changed = git_lines(&workspace, &["diff", "--name-only"])?;
        let commit_created = maybe_autonomy_commit(&workspace, &spec_id, &files_changed, &test_results)?;

        Ok(TaskExecutionOutcome {
            spec_id,
            route_label: plan.route_label,
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
        let (topic, level) = notification_severity(label, message);
        let sent = self
            .channel
            .send_notification_with_level(message, topic, level)
            .await
            .unwrap_or(false);
        if !sent {
            let timestamp = Utc::now().to_rfc3339();
            autonomy.append_notification_log(&format!("[{timestamp}] {label}: {message}"))?;
        }
        Ok(())
    }

    async fn select_autonomy_execution_plan(
        &self,
        task: &crate::autonomy::QueuedTask,
    ) -> Result<TaskExecutionPlan> {
        let Some(runtime) = &self.autonomy_runtime else {
            return Ok(TaskExecutionPlan {
                route_label: "active".to_string(),
                env_overrides: Vec::new(),
                fallback_cloud: None,
            });
        };

        let Some(local_target) = &runtime.local_target else {
            return Ok(TaskExecutionPlan {
                route_label: "active".to_string(),
                env_overrides: Vec::new(),
                fallback_cloud: None,
            });
        };

        let decision = self.decide_autonomy_route(task).await?;
        let (target, fallback_cloud) = match decision.route {
            RouteKind::Local => (local_target.clone(), runtime.cloud_target.clone()),
            RouteKind::Cloud => (
                runtime
                    .cloud_target
                    .clone()
                    .unwrap_or_else(|| local_target.clone()),
                None,
            ),
        };

        Ok(TaskExecutionPlan {
            route_label: format!("{} ({})", route_kind_label(decision.route), decision.reason),
            env_overrides: route_env_overrides(&target, self.mode_state.exec_mode.as_str()),
            fallback_cloud,
        })
    }

    async fn decide_autonomy_route(
        &self,
        task: &crate::autonomy::QueuedTask,
    ) -> Result<RouteDecision> {
        let Some(runtime) = &self.autonomy_runtime else {
            return Ok(RouteDecision {
                route: RouteKind::Cloud,
                reason: "default-active".to_string(),
            });
        };

        if let Some(local_monitor) = &runtime.local_monitor {
            let context = self.autonomy_route_context(task).await;
            let response = local_monitor
                .chat(
                    &[
                        crate::types::Message::text(
                            crate::types::Role::System,
                            "You route background coding tasks between a local model and a cloud model. Respond with JSON only: {\"route\":\"local|cloud\",\"reason\":\"short reason\"}. Prefer local for simple repetitive tasks. Prefer cloud for complex refactors, migrations, security-sensitive work, retries, or tasks that mention architecture, performance, database, or large scope.",
                            None,
                        ),
                        crate::types::Message::text(
                            crate::types::Role::User,
                            format!(
                                "Task: {}\nRetry count: {}\nContext:\n{}",
                                task.description,
                                task.retry_count,
                                context
                            ),
                            None,
                        ),
                    ],
                    None,
                )
                .await;

            if let Ok(message) = response
                && let crate::types::Content::Text { text, .. } = message.content
                && let Ok(parsed) = serde_json::from_str::<RouteDecisionResponse>(text.trim())
            {
                let route = match parsed.route.trim().to_lowercase().as_str() {
                    "local" => RouteKind::Local,
                    "cloud" => RouteKind::Cloud,
                    _ => heuristic_route(task),
                };
                return Ok(RouteDecision {
                    route,
                    reason: parsed.reason.trim().to_string(),
                });
            }
        }

        Ok(RouteDecision {
            route: heuristic_route(task),
            reason: "heuristic".to_string(),
        })
    }

    async fn autonomy_route_context(&self, task: &crate::autonomy::QueuedTask) -> String {
        let mut sections = vec![format!(
            "ExecMode={} BudgetMode={:?}",
            self.mode_state.exec_mode.as_str(),
            self.budget_mode
        )];

        #[cfg(feature = "memory")]
        {
            let memory = self.load_memory_contents_or_empty().await;
            if !memory.trim().is_empty() {
                sections.push(truncate_context(&memory, 2_000));
            }
        }

        if let Some(task_ref) = &task.task_ref {
            sections.push(format!("Task reference: {task_ref}"));
        }

        sections.join("\n\n")
    }

    fn should_escalate_after_local_failure(
        &self,
        task: &crate::autonomy::QueuedTask,
        error: &ForjaError,
    ) -> bool {
        if task.retry_count > 0 {
            return true;
        }

        let error_text = error.to_string().to_lowercase();
        error_text.contains("timeout")
            || error_text.contains("budget")
            || error_text.contains("exceeded")
            || error_text.contains("failed")
    }

    fn confirm_cloud_escalation(
        &self,
        task: &crate::autonomy::QueuedTask,
        cloud_target: &AutonomyTarget,
    ) -> Result<bool> {
        let Some(runtime) = &self.autonomy_runtime else {
            return Ok(true);
        };
        if !runtime.cloud_escalation_requires_confirmation {
            return Ok(true);
        }
        let prompt = format!(
            "Escalate autonomy task '{}' to cloud model {}?",
            task.description, cloud_target.label
        );
        if let Some(confirmer) = &runtime.cloud_escalation_confirmer {
            return Ok(confirmer(&prompt));
        }
        if self.channel.is_cli_source() {
            println!("{prompt} [y/N]");
            let mut input = String::new();
            std::io::stdin()
                .read_line(&mut input)
                .map_err(|error| ForjaError::Internal(format!("failed to read confirmation: {error}")))?;
            let normalized = input.trim().to_lowercase();
            return Ok(matches!(normalized.as_str(), "y" | "yes" | "예" | "ㅇ"));
        }
        Ok(true)
    }

}

fn execute_task_pipeline(
    task: &crate::autonomy::QueuedTask,
    workspace: &Path,
    env_overrides: &[(String, String)],
    exec_mode: &str,
) -> Result<(String, Vec<String>)> {
    let mut test_results = Vec::new();
    let baseline_dirty = git_lines(workspace, &["status", "--porcelain"])?;
    let spec_id = resolve_or_plan_spec(task, workspace, env_overrides, exec_mode)?;

    run_checked_command(workspace, "auto", &["go", &spec_id, "--solo", "--auto"], env_overrides, exec_mode)?;
    run_checked_command(workspace, "cargo", &["build", "--workspace"], &[], exec_mode)?;
    test_results.push("cargo build --workspace=pass".to_string());
    run_checked_command(workspace, "cargo", &["clippy", "--workspace", "--", "-D", "warnings"], &[], exec_mode)?;
    test_results.push("cargo clippy --workspace -- -D warnings=pass".to_string());
    run_checked_command(workspace, "cargo", &["test", "-p", "forja-llm"], &[], exec_mode)?;
    test_results.push("cargo test -p forja-llm=pass".to_string());
    run_checked_command(workspace, "cargo", &["test", "-p", "forja-llm", "--", "--ignored"], &[], exec_mode)?;
    test_results.push("cargo test -p forja-llm -- --ignored=pass".to_string());
    if !baseline_dirty.is_empty() {
        test_results.push("auto-commit=skipped (dirty worktree before task)".to_string());
    }

    Ok((spec_id, test_results))
}

fn resolve_or_plan_spec(
    task: &crate::autonomy::QueuedTask,
    workspace: &Path,
    env_overrides: &[(String, String)],
    exec_mode: &str,
) -> Result<String> {
    if let Some(task_ref) = &task.task_ref {
        if task_ref.starts_with("SPEC-") {
            return Ok(task_ref.clone());
        }
        if let Some(spec_id) = spec_id_from_path(task_ref) {
            return Ok(spec_id);
        }
    }

    let before = list_spec_dirs(workspace)?;
    run_checked_command(
        workspace,
        "auto",
        &["plan", &task.description, "--solo"],
        env_overrides,
        exec_mode,
    )?;
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

fn run_checked_command(
    workspace: &Path,
    program: &str,
    args: &[&str],
    env_overrides: &[(String, String)],
    exec_mode: &str,
) -> Result<()> {
    let mut command = Command::new(program);
    command
        .args(args)
        .current_dir(workspace)
        .env("FORJA_MODE", exec_mode);
    for (key, value) in env_overrides {
        command.env(key, value);
    }
    let output = command
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

    run_checked_command(workspace, "git", &["add", "-A"], &[], "auto")?;
    let message = build_lore_commit_message(spec_id, files_changed, test_results);
    run_checked_command(workspace, "git", &["commit", "-m", &message], &[], "auto")?;
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

fn route_env_overrides(target: &AutonomyTarget, exec_mode: &str) -> Vec<(String, String)> {
    vec![
        ("FORJA_PROVIDER".to_string(), target.provider.clone()),
        ("FORJA_MODEL".to_string(), target.model.clone()),
        ("FORJA_MODE".to_string(), exec_mode.to_string()),
    ]
}

fn heuristic_route(task: &crate::autonomy::QueuedTask) -> RouteKind {
    let normalized = task.description.to_lowercase();
    if task.retry_count > 0
        || task.task_ref.as_deref().is_some_and(|task_ref| task_ref.starts_with("SPEC-"))
        || [
            "architecture",
            "security",
            "migration",
            "database",
            "performance",
            "benchmark",
            "refactor",
            "spec",
            "e2e",
            "full",
            "complex",
        ]
        .iter()
        .any(|keyword| normalized.contains(keyword))
        || normalized.len() > 120
    {
        RouteKind::Cloud
    } else {
        RouteKind::Local
    }
}

fn route_kind_label(route: RouteKind) -> &'static str {
    match route {
        RouteKind::Local => "local",
        RouteKind::Cloud => "cloud",
    }
}

fn truncate_context(value: &str, limit: usize) -> String {
    let collapsed = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.chars().count() <= limit {
        return collapsed;
    }

    let mut truncated = collapsed.chars().take(limit.saturating_sub(1)).collect::<String>();
    truncated.push('…');
    truncated
}

fn notification_severity(label: &str, message: &str) -> (NotificationTopic, NotificationLevel) {
    let normalized = label.to_lowercase();
    if normalized.contains("failed") || normalized.contains("error") {
        return (NotificationTopic::Error, NotificationLevel::Critical);
    }
    if normalized.contains("completed") {
        let level = if message.contains("Duration:") {
            NotificationLevel::Info
        } else {
            NotificationLevel::Warning
        };
        return (NotificationTopic::Task, level);
    }

    (NotificationTopic::Autonomy, NotificationLevel::Info)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn heuristic_route_prefers_cloud_for_complex_tasks() {
        let task = crate::autonomy::QueuedTask {
            id: 1,
            description: "Database migration and performance refactor".to_string(),
            source: "user".to_string(),
            status: "pending".to_string(),
            created_at: "2026-04-21T00:00:00Z".to_string(),
            started_at: None,
            completed_at: None,
            result: None,
            requires_approval: false,
            retry_count: 0,
            next_attempt_at: None,
            task_ref: None,
            cancel_requested: false,
        };

        assert_eq!(heuristic_route(&task), RouteKind::Cloud);
    }

    #[test]
    fn route_env_overrides_set_provider_and_model() {
        let target = AutonomyTarget {
            provider: "ollama".to_string(),
            model: "qwen3.5:9b".to_string(),
            label: "ollama/qwen3.5:9b".to_string(),
            local: true,
        };

        let envs = route_env_overrides(&target, "auto");

        assert!(envs.contains(&("FORJA_PROVIDER".to_string(), "ollama".to_string())));
        assert!(envs.contains(&("FORJA_MODEL".to_string(), "qwen3.5:9b".to_string())));
        assert!(envs.contains(&("FORJA_MODE".to_string(), "auto".to_string())));
    }
}
