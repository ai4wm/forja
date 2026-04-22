use super::combination::build_combination_prompt;
use super::execution::{
    DebateCallContext, DebateMessageCallback, execute_agent_call, log_debate_timeout,
};
use super::mutation::build_mutation_prompt;
use super::types::{DebateMessage, DebatePhase, DebateResult, DebateRunMetadata, TaskItem};
use super::{CreationRunContext, DebateAgent, DebateEngine};
use crate::audit::logger::AuditLogger;
use crate::error::{ForjaError, Result};
use crate::traits::LlmProvider;
use std::sync::Arc;

impl DebateEngine {
    pub async fn run_debate(
        &self,
        topic: &str,
        provider: &Arc<dyn LlmProvider>,
        audit_logger: Option<&AuditLogger>,
    ) -> Result<DebateResult> {
        self.run_debate_with_callback(topic, provider, audit_logger, None, None)
            .await
    }

    pub async fn run_debate_with_context(
        &self,
        topic: &str,
        provider: &Arc<dyn LlmProvider>,
        audit_logger: Option<&AuditLogger>,
        run_context: Option<CreationRunContext>,
    ) -> Result<DebateResult> {
        self.run_debate_with_callback(topic, provider, audit_logger, None, run_context)
            .await
    }

    pub(crate) async fn run_debate_with_callback(
        &self,
        topic: &str,
        provider: &Arc<dyn LlmProvider>,
        audit_logger: Option<&AuditLogger>,
        mut on_message: Option<&mut DebateMessageCallback>,
        run_context: Option<CreationRunContext>,
    ) -> Result<DebateResult> {
        let active_agents = self.select_active_agents(topic);
        let mut transcript = Vec::new();
        let mut total_tokens = 0;
        let mut call_index = 0;
        let bounded_context_chars = run_context
            .as_ref()
            .map_or(2_000, |context| context.max_prompt_context_chars);

        for round in 1..=self.config.diverge_rounds {
            for agent in &active_agents {
                let prompt = build_diverge_prompt(agent, topic, &transcript, bounded_context_chars);
                let call_context = DebateCallContext {
                    phase: DebatePhase::Diverge,
                    round,
                    should_delay: call_index > 0,
                };
                call_index += 1;
                let message = execute_agent_call(
                    provider,
                    audit_logger,
                    agent,
                    call_context,
                    prompt,
                    &mut on_message,
                    run_context.as_ref(),
                )
                .await?;
                total_tokens += message.tokens;
                transcript.push(message);
            }
        }

        let diverge_output =
            transcript_for_phase(&transcript, DebatePhase::Diverge, bounded_context_chars);
        for round in 1..=self.config.conflict_rounds {
            for agent in &active_agents {
                let prompt = build_conflict_prompt(
                    agent,
                    &diverge_output,
                    &transcript,
                    round,
                    bounded_context_chars,
                );
                let call_context = DebateCallContext {
                    phase: DebatePhase::Conflict,
                    round,
                    should_delay: call_index > 0,
                };
                call_index += 1;
                let message = execute_agent_call(
                    provider,
                    audit_logger,
                    agent,
                    call_context,
                    prompt,
                    &mut on_message,
                    run_context.as_ref(),
                )
                .await?;
                total_tokens += message.tokens;
                transcript.push(message);
            }
        }

        let conflict_output =
            transcript_for_phase(&transcript, DebatePhase::Conflict, bounded_context_chars);
        for round in 1..=self.config.combination_rounds {
            for agent in &active_agents {
                let prompt = build_combination_prompt(
                    agent,
                    &diverge_output,
                    &conflict_output,
                    &transcript,
                    round,
                    bounded_context_chars,
                );
                let call_context = DebateCallContext {
                    phase: DebatePhase::Combination,
                    round,
                    should_delay: call_index > 0,
                };
                call_index += 1;
                let message = execute_agent_call(
                    provider,
                    audit_logger,
                    agent,
                    call_context,
                    prompt,
                    &mut on_message,
                    run_context.as_ref(),
                )
                .await?;
                total_tokens += message.tokens;
                transcript.push(message);
            }
        }

        let combination_output =
            transcript_for_phase(&transcript, DebatePhase::Combination, bounded_context_chars);
        for round in 1..=self.config.mutation_rounds {
            for agent in &active_agents {
                let prompt = build_mutation_prompt(
                    agent,
                    &combination_output,
                    &transcript,
                    round,
                    bounded_context_chars,
                );
                let call_context = DebateCallContext {
                    phase: DebatePhase::Mutation,
                    round,
                    should_delay: call_index > 0,
                };
                call_index += 1;
                let message = execute_agent_call(
                    provider,
                    audit_logger,
                    agent,
                    call_context,
                    prompt,
                    &mut on_message,
                    run_context.as_ref(),
                )
                .await?;
                total_tokens += message.tokens;
                transcript.push(message);
            }
        }

        let synthesizer = select_synthesizer(&active_agents)?;
        for round in 1..=self.config.converge_rounds {
            let prompt = build_converge_prompt(synthesizer, &transcript, bounded_context_chars);
            let call_context = DebateCallContext {
                phase: DebatePhase::Converge,
                round,
                should_delay: call_index > 0,
            };
            call_index += 1;
            let message = execute_agent_call(
                provider,
                audit_logger,
                synthesizer,
                call_context,
                prompt,
                &mut on_message,
                run_context.as_ref(),
            )
            .await?;
            total_tokens += message.tokens;
            transcript.push(message);
        }

        let final_content = transcript
            .last()
            .map(|message| message.content.clone())
            .unwrap_or_default();
        let (summary, task_list) = parse_final_output(&final_content, synthesizer);
        let metadata = DebateRunMetadata {
            active_agent_count: active_agents.len(),
            diverge_rounds: self.config.diverge_rounds,
            conflict_rounds: self.config.conflict_rounds,
            combination_rounds: self.config.combination_rounds,
            mutation_rounds: self.config.mutation_rounds,
            converge_rounds: self.config.converge_rounds,
        };

        Ok(DebateResult {
            summary,
            task_list,
            transcript,
            total_tokens,
            total_rounds: self.config.diverge_rounds
                + self.config.conflict_rounds
                + self.config.combination_rounds
                + self.config.mutation_rounds
                + self.config.converge_rounds,
            active_agent_count: active_agents.len(),
            metadata,
        })
    }

    fn select_active_agents(&self, topic: &str) -> Vec<DebateAgent> {
        let target_count = self.target_agent_count(topic);
        self.agents.iter().take(target_count).cloned().collect()
    }

    fn target_agent_count(&self, topic: &str) -> usize {
        let configured_max = self.config.max_agents.min(self.agents.len()).max(1);
        let configured_min = self.config.min_agents.min(configured_max).max(1);
        if !self.config.auto_team_sizing || configured_min >= configured_max {
            return configured_max;
        }

        let lower = topic.to_lowercase();
        let word_count = lower.split_whitespace().count();
        let has_complex_keyword = [
            "architecture",
            "integration",
            "security",
            "migration",
            "runtime",
            "budget",
            "retry",
            "discord",
            "tauri",
            "creation",
        ]
        .iter()
        .any(|keyword| lower.contains(keyword));

        if word_count <= 5 && !has_complex_keyword {
            configured_min
        } else if word_count <= 10 && !has_complex_keyword {
            (configured_min + 1).min(configured_max)
        } else {
            configured_max
        }
    }
}

fn build_diverge_prompt(
    agent: &DebateAgent,
    topic: &str,
    transcript: &[DebateMessage],
    bounded_context_chars: usize,
) -> String {
    let previous = transcript_for_phase(transcript, DebatePhase::Diverge, bounded_context_chars);
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
    bounded_context_chars: usize,
) -> String {
    let previous_conflict =
        transcript_for_phase(transcript, DebatePhase::Conflict, bounded_context_chars);
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

fn build_converge_prompt(
    agent: &DebateAgent,
    transcript: &[DebateMessage],
    bounded_context_chars: usize,
) -> String {
    let full_transcript = truncate_chars(
        &transcript
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
            .join("\n"),
        bounded_context_chars,
    );

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

fn transcript_for_phase(
    transcript: &[DebateMessage],
    phase: DebatePhase,
    bounded_context_chars: usize,
) -> String {
    let rendered = transcript
        .iter()
        .filter(|message| message.phase == phase)
        .map(|message| format!("{}: {}", message.role, message.content))
        .collect::<Vec<_>>()
        .join("\n");
    truncate_chars(&rendered, bounded_context_chars)
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

fn parse_final_output(content: &str, synthesizer: &DebateAgent) -> (String, Vec<TaskItem>) {
    let lines = content
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>();
    let summary = lines.iter().take(3).copied().collect::<Vec<_>>().join(" ");
    let mut task_list = lines
        .iter()
        .skip(3)
        .filter_map(|line| parse_task_line(line))
        .collect::<Vec<_>>();

    if task_list.is_empty() {
        task_list.push(TaskItem {
            name: "Review creation output manually".to_string(),
            assigned_role: synthesizer.role.clone(),
            estimated_hours: 1.0,
            priority: 1,
        });
    }

    (summary, task_list)
}

fn parse_task_line(line: &str) -> Option<TaskItem> {
    let trimmed = line.strip_prefix('-')?.trim();
    let parts = trimmed.split('|').map(str::trim).collect::<Vec<_>>();
    if parts.len() != 4 {
        return None;
    }

    let name = truncate_chars(parts[0], 120);
    let assigned_role = truncate_chars(parts[1], 64);
    let estimated_hours = parts[2].parse::<f32>().ok()?;
    let priority = parts[3].parse::<u8>().ok()?;
    if name.is_empty()
        || assigned_role.is_empty()
        || !estimated_hours.is_finite()
        || estimated_hours <= 0.0
        || estimated_hours > 80.0
        || !(1..=5).contains(&priority)
    {
        return None;
    }

    Some(TaskItem {
        name,
        assigned_role,
        estimated_hours,
        priority,
    })
}

fn truncate_chars(value: &str, max_chars: usize) -> String {
    if value.chars().count() <= max_chars {
        return value.to_string();
    }
    let mut truncated = value
        .chars()
        .take(max_chars.saturating_sub(1))
        .collect::<String>();
    truncated.push('…');
    truncated
}

#[allow(dead_code)]
fn _keep_timeout_logger_used(
    audit_logger: Option<&AuditLogger>,
    agent: &DebateAgent,
    call_context: DebateCallContext,
) {
    log_debate_timeout(audit_logger, agent, call_context);
}
