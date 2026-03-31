use super::types::{DebateMessage, DebatePhase, DebateResult, TaskItem};
use super::{DebateAgent, DebateEngine};
use crate::audit::logger::{AuditEvent, AuditLogger};
use crate::context::token_counter::count_tokens;
use crate::error::{ForjaError, Result};
use crate::traits::LlmProvider;
use crate::types::{Content, Message, Role};
use serde_json::json;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use tokio::time::{sleep, timeout, Duration};

type DebateMessageCallback =
    dyn FnMut(&DebateMessage) -> Pin<Box<dyn Future<Output = Result<()>> + Send>>;

#[derive(Debug, Clone, Copy)]
struct DebateCallContext {
    phase: DebatePhase,
    round: usize,
    should_delay: bool,
}

impl DebateEngine {
    pub async fn run_debate(
        &self,
        topic: &str,
        provider: &Arc<dyn LlmProvider>,
        audit_logger: Option<&AuditLogger>,
    ) -> Result<DebateResult> {
        self.run_debate_with_callback(topic, provider, audit_logger, None)
            .await
    }

    pub(crate) async fn run_debate_with_callback(
        &self,
        topic: &str,
        provider: &Arc<dyn LlmProvider>,
        audit_logger: Option<&AuditLogger>,
        mut on_message: Option<&mut DebateMessageCallback>,
    ) -> Result<DebateResult> {
        let active_agents: Vec<DebateAgent> = self
            .agents
            .iter()
            .take(self.config.max_agents)
            .cloned()
            .collect();

        let mut transcript = Vec::new();
        let mut total_tokens = 0;
        let mut call_index = 0;

        for round in 1..=self.config.diverge_rounds {
            for agent in &active_agents {
                let prompt = build_diverge_prompt(agent, topic, &transcript);
                let context = DebateCallContext {
                    phase: DebatePhase::Diverge,
                    round,
                    should_delay: call_index > 0,
                };
                call_index += 1;
                let message = call_agent(
                    provider,
                    audit_logger,
                    agent,
                    context,
                    prompt,
                    &mut on_message,
                )
                .await?;
                total_tokens += message.tokens;
                transcript.push(message);
            }
        }

        let diverge_output = transcript_for_phase(&transcript, DebatePhase::Diverge);
        for round in 1..=self.config.conflict_rounds {
            for agent in &active_agents {
                let prompt = build_conflict_prompt(agent, &diverge_output, &transcript, round);
                let context = DebateCallContext {
                    phase: DebatePhase::Conflict,
                    round,
                    should_delay: call_index > 0,
                };
                call_index += 1;
                let message = call_agent(
                    provider,
                    audit_logger,
                    agent,
                    context,
                    prompt,
                    &mut on_message,
                )
                .await?;
                total_tokens += message.tokens;
                transcript.push(message);
            }
        }

        let synthesizer = select_synthesizer(&active_agents)?;
        for round in 1..=self.config.converge_rounds {
            let prompt = build_converge_prompt(synthesizer, &transcript);
            let context = DebateCallContext {
                phase: DebatePhase::Converge,
                round,
                should_delay: call_index > 0,
            };
            call_index += 1;
            let message = call_agent(
                provider,
                audit_logger,
                synthesizer,
                context,
                prompt,
                &mut on_message,
            )
            .await?;
            total_tokens += message.tokens;
            transcript.push(message);
        }

        let final_content = transcript
            .last()
            .map(|message| message.content.clone())
            .unwrap_or_default();
        let (summary, task_list) = parse_final_output(&final_content);

        Ok(DebateResult {
            summary,
            task_list,
            transcript,
            total_tokens,
            total_rounds: self.config.diverge_rounds
                + self.config.conflict_rounds
                + self.config.converge_rounds,
        })
    }
}

async fn call_agent(
    provider: &Arc<dyn LlmProvider>,
    audit_logger: Option<&AuditLogger>,
    agent: &DebateAgent,
    context: DebateCallContext,
    prompt: String,
    on_message: &mut Option<&mut DebateMessageCallback>,
) -> Result<DebateMessage> {
    if context.should_delay {
        sleep(Duration::from_secs(2)).await;
    }

    let request_messages = [
        Message::text(
            Role::System,
            format!("You are {}. Your framework: {}", agent.role, agent.framework),
            None,
        ),
        Message::text(Role::User, prompt, None),
    ];

    let content = match timeout(
        Duration::from_secs(60),
        provider.chat(&request_messages, None),
    )
    .await
    {
        Ok(Ok(response)) => match response.content {
            Content::Text { text, .. } => text,
            _ => {
                return Err(ForjaError::LlmError(
                    "debate response was not text".to_string(),
                ))
            }
        },
        Ok(Err(error)) => return Err(error),
        Err(_) => {
            log_debate_timeout(audit_logger, agent, context);
            "[timeout] No response within 60s".to_string()
        }
    };

    let message = DebateMessage {
        agent_id: agent.id.clone(),
        role: agent.role.clone(),
        phase: context.phase,
        round: context.round,
        tokens: count_tokens(&content, "cl100k_base"),
        content,
    };

    log_debate_message(audit_logger, &message);

    if let Some(callback) = on_message.as_deref_mut() {
        callback(&message).await?;
    }

    Ok(message)
}

fn build_diverge_prompt(agent: &DebateAgent, topic: &str, transcript: &[DebateMessage]) -> String {
    let previous = transcript_for_phase(transcript, DebatePhase::Diverge);
    format!(
        "You are {role}. Your framework: {framework}\n\
Phase: DIVERGE. Rule: You MUST build on previous ideas.\n\
Say \"Yes, and...\" with no criticism or rejection.\n\
Topic: {topic}\n\
Previous ideas: {previous}\n\
Your contribution:",
        role = agent.role,
        framework = agent.framework,
    )
}

fn build_conflict_prompt(
    agent: &DebateAgent,
    diverge_output: &str,
    transcript: &[DebateMessage],
    round: usize,
) -> String {
    let previous_conflict = transcript_for_phase(transcript, DebatePhase::Conflict);
    format!(
        "You are {role}. Your framework: {framework}\n\
Phase: CONFLICT. Rule: Find falsifiable flaws in each proposal.\n\
Estimate failure probability for each flaw.\n\
If you criticize, you MUST propose an alternative.\n\
Proposals so far: {diverge_output}\n\
Round {round} discussion: {previous_conflict}\n\
Your critique and alternatives:",
        role = agent.role,
        framework = agent.framework,
    )
}

fn build_converge_prompt(agent: &DebateAgent, transcript: &[DebateMessage]) -> String {
    let full_transcript = transcript
        .iter()
        .map(|message| {
            format!(
                "[{}][R{}] {}: {}",
                message.phase.label(),
                message.round,
                message.role,
                message.content
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    format!(
        "You are {role}. Your framework: {framework}\n\
Phase: CONVERGE.\n\
Summarize the entire debate in exactly 3 sentences.\n\
Then create an executable task list in this exact format:\n\
- <task name> | <assigned role> | <estimated hours> | <priority>\n\
Debate transcript: {full_transcript}\n\
Summary and task list:",
        role = agent.role,
        framework = agent.framework,
    )
}

fn transcript_for_phase(transcript: &[DebateMessage], phase: DebatePhase) -> String {
    transcript
        .iter()
        .filter(|message| message.phase == phase)
        .map(|message| format!("{}: {}", message.role, message.content))
        .collect::<Vec<_>>()
        .join("\n")
}

fn select_synthesizer(agents: &[DebateAgent]) -> Result<&DebateAgent> {
    agents
        .iter()
        .find(|agent| {
            agent.role.eq_ignore_ascii_case("synthesis")
                || agent.role.eq_ignore_ascii_case("synthesizer")
                || agent.id.contains("synth")
        })
        .or_else(|| agents.last())
        .ok_or_else(|| ForjaError::Internal("debate engine has no agents".to_string()))
}

fn parse_final_output(content: &str) -> (String, Vec<TaskItem>) {
    let lines: Vec<&str> = content
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();

    let summary = lines.iter().take(3).copied().collect::<Vec<_>>().join(" ");
    let task_list = lines
        .iter()
        .skip(3)
        .filter_map(|line| parse_task_line(line))
        .collect();

    (summary, task_list)
}

fn parse_task_line(line: &str) -> Option<TaskItem> {
    let trimmed = line.strip_prefix('-')?.trim();
    let parts: Vec<&str> = trimmed.split('|').map(str::trim).collect();
    if parts.len() != 4 {
        return None;
    }

    let estimated_hours = parts[2].parse::<f32>().ok()?;
    let priority = parts[3].parse::<u8>().ok()?;

    Some(TaskItem {
        name: parts[0].to_string(),
        assigned_role: parts[1].to_string(),
        estimated_hours,
        priority,
    })
}

fn log_debate_message(audit_logger: Option<&AuditLogger>, message: &DebateMessage) {
    let Some(audit_logger) = audit_logger else {
        return;
    };

    let event = AuditEvent::new(
        "debate_message",
        json!({
            "role": message.role,
            "phase": message.phase.label(),
            "round": message.round,
            "content": message.content,
        }),
    )
    .with_agent_id(message.agent_id.clone())
    .with_token_count(message.tokens);
    let _ = audit_logger.log_event(event);
}

fn log_debate_timeout(
    audit_logger: Option<&AuditLogger>,
    agent: &DebateAgent,
    context: DebateCallContext,
) {
    let Some(audit_logger) = audit_logger else {
        return;
    };

    let event = AuditEvent::new(
        "debate_timeout",
        json!({
            "agent_id": agent.id.clone(),
            "phase": context.phase.label(),
            "round": context.round,
        }),
    )
    .with_agent_id(agent.id.clone());
    let _ = audit_logger.log_event(event);
}
