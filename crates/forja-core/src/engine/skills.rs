use super::Engine;
use crate::error::{ForjaError, Result};
use crate::traits::{NotificationLevel, NotificationTopic};

impl Engine {
    pub(super) fn handle_skills_command(&self) -> Result<String> {
        let mut sections = Vec::new();

        if let Some(skill_registry) = &self.skill_registry {
            let skills = skill_registry.list_skills()?;
            if !skills.is_empty() {
                sections.push(format!(
                    "Loaded skills:\n{}",
                    skills
                        .into_iter()
                        .map(|skill| {
                            let stats = format!(
                                "success={} failure={}",
                                skill.success_count, skill.failure_count
                            );
                            let suggestion = skill
                                .suggestion
                                .as_deref()
                                .map(|value| format!(" | suggestion={value}"))
                                .unwrap_or_default();
                            format!(
                                "- {} | trigger={} | {} | source={}{}",
                                skill.name,
                                skill.trigger,
                                stats,
                                skill.source_path.display(),
                                suggestion
                            )
                        })
                        .collect::<Vec<_>>()
                        .join("\n")
                ));
            }
        }

        if let Ok(learned) = self.handle_learned_tool_skills()
            && learned != "No skills recorded yet."
        {
            sections.push(format!("Learned tool skills:\n{learned}"));
        }

        if sections.is_empty() {
            return Ok("No skills loaded.".to_string());
        }

        Ok(sections.join("\n\n"))
    }

    pub(super) async fn run_skill_command(&mut self, skill_name: &str) -> Result<String> {
        let skill_registry = self
            .skill_registry
            .clone()
            .ok_or_else(|| ForjaError::Internal("skill registry is not configured".to_string()))?;
        let skill = skill_registry
            .find_by_name(skill_name)?
            .ok_or_else(|| ForjaError::Internal(format!("Unknown skill: {skill_name}")))?;
        let steps = skill_registry.extract_shell_steps(&skill);
        if steps.is_empty() {
            let error = format!("Skill '{}' does not contain executable shell steps.", skill.name);
            skill_registry.record_failure(&skill.name, &error)?;
            let _ = self
                .channel
                .send_notification_with_level(&error, NotificationTopic::Skill, NotificationLevel::Warning)
                .await;
            return Err(ForjaError::Internal(error));
        }

        let Some(shell_tool) = self.tools.get("shell").cloned() else {
            let error = "Shell tool is not available, so the skill cannot run.".to_string();
            skill_registry.record_failure(&skill.name, &error)?;
            let _ = self
                .channel
                .send_notification_with_level(&error, NotificationTopic::Skill, NotificationLevel::Warning)
                .await;
            return Err(ForjaError::Internal(error));
        };

        let mut outputs = Vec::new();
        for step in steps {
            let args = serde_json::json!({ "command": step.command });
            let result = shell_tool.execute(args).await?;
            let status = result["status"].as_str().unwrap_or_default();
            let output = result["output"]
                .as_str()
                .or_else(|| result["data"].as_str())
                .unwrap_or_default()
                .trim()
                .to_string();

            outputs.push(format!("{}: {}", step.language, output));

            if status == "error" || status == "blocked" || status == "warning" {
                let error = if output.is_empty() {
                    format!("Skill '{}' step failed with status '{}'.", skill.name, status)
                } else {
                    format!("Skill '{}' step failed: {}", skill.name, output)
                };
                skill_registry.record_failure(&skill.name, &error)?;
                let _ = self
                    .channel
                    .send_notification_with_level(&error, NotificationTopic::Skill, NotificationLevel::Warning)
                    .await;
                return Err(ForjaError::Internal(error));
            }
        }

        skill_registry.record_success(&skill.name)?;
        let suggestion = skill_registry.improvement_suggestion(&skill.name)?;
        let suggestion = suggestion
            .map(|value| format!("\nImprovement suggestion: {value}"))
            .unwrap_or_default();

        let reply = format!(
            "Skill '{}' completed.\n{}{}",
            skill.name,
            outputs.join("\n"),
            suggestion
        );
        let _ = self
            .channel
            .send_notification_with_level(
                &format!("Skill '{}' completed.", skill.name),
                NotificationTopic::Skill,
                NotificationLevel::Info,
            )
            .await;

        Ok(reply)
    }
}
