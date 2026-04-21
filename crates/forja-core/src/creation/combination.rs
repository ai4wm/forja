use super::types::{DebateMessage, DebatePhase};
use super::DebateAgent;

pub(crate) fn build_combination_prompt(
    agent: &DebateAgent,
    diverge_output: &str,
    conflict_output: &str,
    transcript: &[DebateMessage],
    round: usize,
    bounded_context_chars: usize,
) -> String {
    let pattern = if round.is_multiple_of(2) { "SCAMPER" } else { "TRIZ" };
    let previous = bounded_transcript_for_phase(
        transcript,
        DebatePhase::Combination,
        bounded_context_chars,
    );
    format!(
        "You are {role}. Your framework: {framework}\n\
Phase: COMBINATION.\n\
Use {pattern} to fuse at least two prior ideas into a stronger combined proposal.\n\
Keep the output concrete and implementation-aware.\n\
Divergence context: {diverge_output}\n\
Conflict context: {conflict_output}\n\
Previous combination discussion: {previous}\n\
Your combined proposal:",
        role = agent.role,
        framework = agent.framework,
    )
}

fn bounded_transcript_for_phase(
    transcript: &[DebateMessage],
    phase: DebatePhase,
    bounded_context_chars: usize,
) -> String {
    let text = transcript
        .iter()
        .filter(|message| message.phase == phase)
        .map(|message| format!("{}: {}", message.role, message.content))
        .collect::<Vec<_>>()
        .join("\n");
    truncate_chars(&text, bounded_context_chars)
}

fn truncate_chars(value: &str, max_chars: usize) -> String {
    if value.chars().count() <= max_chars {
        return value.to_string();
    }

    let mut truncated = value.chars().take(max_chars.saturating_sub(1)).collect::<String>();
    truncated.push('…');
    truncated
}
