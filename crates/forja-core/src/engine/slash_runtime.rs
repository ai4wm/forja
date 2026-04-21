use super::{Engine, SlashCommandResult};
use crate::error::Result;
use crate::types::{Content, Message, Role};

impl Engine {
    #[cfg(feature = "runtime")]
    pub(crate) async fn dispatch_slash_command(&mut self, user_msg: &Message) -> Result<bool> {
        let slash_result = if let Content::Text { text, .. } = &user_msg.content {
            if let Some(handler) = &self.slash_handler.clone() {
                handler(text, &mut self.provider, &mut self.mode_state)
            } else {
                None
            }
        } else {
            None
        };

        let Some(slash_result) = slash_result else {
            return Ok(false);
        };

        self.handle_slash_result(user_msg, slash_result).await?;
        Ok(true)
    }

    #[cfg(feature = "runtime")]
    async fn handle_slash_result(
        &mut self,
        user_msg: &Message,
        slash_result: SlashCommandResult,
    ) -> Result<()> {
        match slash_result {
            SlashCommandResult::Reply(reply) => {
                let _ = self.channel.send(Message::text(Role::Assistant, &reply, None)).await;
            }
            SlashCommandResult::ReplyAndSave { user_text, reply } => {
                let user_msg_save = Message::text(Role::User, &user_text, None);
                let reply_msg = Message::text(Role::Assistant, &reply, None);
                let _ = self.channel.send(reply_msg.clone()).await;
                self.push_message(user_msg_save.clone());
                self.push_message(reply_msg);
                #[cfg(feature = "memory")]
                self.save_turn_memory_entries(&user_msg_save, Some(&reply)).await;
            }
            SlashCommandResult::Debate { topic } => {
                let result = self.run_debate_command(&topic).await?;
                let final_reply = format!(
                    "[Debate Result]\nSummary: {}\nTasks:\n{}",
                    result.summary,
                    result
                        .task_list
                        .iter()
                        .map(|task| {
                            format!(
                                "- {} | {} | {}h | P{}",
                                task.name,
                                task.assigned_role,
                                task.estimated_hours,
                                task.priority
                            )
                        })
                        .collect::<Vec<_>>()
                        .join("\n")
                );
                let reply_msg = Message::text(Role::Assistant, &final_reply, None);
                let _ = self.channel.send(reply_msg.clone()).await;
                self.push_message(user_msg.clone());
                self.push_message(reply_msg.clone());
                #[cfg(feature = "memory")]
                self.save_turn_memory_entries(user_msg, Some(&final_reply)).await;
            }
            SlashCommandResult::Dashboard => {
                let reply = match &self.dashboard_handler {
                    Some(handler) => match handler() {
                        Ok(url) => format!("[Dashboard] {url} opened"),
                        Err(error) => format!("❌ Dashboard failed: {error}"),
                    },
                    None => "❌ Dashboard handler is not configured.".to_string(),
                };
                let _ = self.channel.send(Message::text(Role::Assistant, &reply, None)).await;
            }
            SlashCommandResult::Tui => {
                let reply = match &self.tui_handler {
                    Some(handler) => match handler() {
                        Ok(message) => message,
                        Err(error) => format!("❌ TUI failed: {error}"),
                    },
                    None => "❌ TUI handler is not configured.".to_string(),
                };
                let _ = self.channel.send(Message::text(Role::Assistant, &reply, None)).await;
            }
            #[cfg(feature = "memory")]
            SlashCommandResult::Dream => {
                let reply = self.handle_manual_dream_command();
                let _ = self.channel.send(Message::text(Role::Assistant, &reply, None)).await;
            }
            SlashCommandResult::Skill { name } => {
                let reply = self.run_skill_command(&name).await?;
                let _ = self.channel.send(Message::text(Role::Assistant, &reply, None)).await;
            }
            SlashCommandResult::Task { description } => {
                let reply = self.handle_task_command(&description)?;
                let _ = self.channel.send(Message::text(Role::Assistant, &reply, None)).await;
            }
            SlashCommandResult::AutonomyCommand { command } => {
                let reply = self.handle_autonomy_command(&command)?;
                let _ = self.channel.send(Message::text(Role::Assistant, &reply, None)).await;
            }
            SlashCommandResult::Skills => {
                let reply = self.handle_skills_command()?;
                let _ = self.channel.send(Message::text(Role::Assistant, &reply, None)).await;
            }
            SlashCommandResult::Unresolved => {
                let reply = self.handle_unresolved_command()?;
                let _ = self.channel.send(Message::text(Role::Assistant, &reply, None)).await;
            }
            SlashCommandResult::UpdateSystemPrompt {
                reply,
                system_prompt,
                reset_history,
            } => {
                self.apply_system_prompt_update(system_prompt, reset_history);
                if !reply.trim().is_empty() {
                    let _ = self.channel.send(Message::text(Role::Assistant, &reply, None)).await;
                }
            }
        }

        Ok(())
    }
}
