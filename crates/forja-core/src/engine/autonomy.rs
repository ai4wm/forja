use super::Engine;
use crate::autonomy::loop_runner::AutonomousLoop;
use crate::autonomy::AutonomyAction;
use crate::error::{ForjaError, Result};

impl Engine {
    pub fn with_autonomy(mut self, loop_runner: AutonomousLoop) -> Self {
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
            if let AutonomyAction::ExecuteTask {
                task_id,
                description,
                tool_name,
                args,
                ..
            } = action
            {
                autonomy.mark_task_started(task_id)?;
                if let Some(tool) = self.tools.get(&tool_name) {
                    match tool.execute(args).await {
                        Ok(result) => {
                            let promoted = autonomy.record_tool_success(&tool_name)?;
                            autonomy.mark_task_completed(task_id, result.to_string())?;
                            if promoted {
                                self.log_autonomy_note("skill_promoted", &tool_name);
                            }
                        }
                        Err(error) => {
                            autonomy.add_unresolved(&description, &error.to_string())?;
                            autonomy.mark_task_failed(task_id, error.to_string())?;
                        }
                    }
                } else {
                    let error = format!("unknown tool: {tool_name}");
                    autonomy.add_unresolved(&description, &error)?;
                    autonomy.mark_task_failed(task_id, error)?;
                }
            }
        }

        Ok(())
    }

    pub(super) fn handle_task_command(&self, description: &str) -> Result<String> {
        let Some(autonomy) = &self.autonomy else {
            return Err(ForjaError::Internal("autonomy is not configured".to_string()));
        };

        let task_id = autonomy.enqueue_task(description, "user")?;
        Ok(format!("Queued task #{task_id}: {description}"))
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
}
