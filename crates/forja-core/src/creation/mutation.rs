use super::types::{DebateMessage, DebatePhase};
use super::DebateAgent;

const MUTATION_OPERATORS: &[&str] = &[
    "inversion",
    "amplification",
    "reduction",
    "elimination",
    "failure-to-advantage",
];

pub(crate) fn build_mutation_prompt(
    agent: &DebateAgent,
    combination_output: &str,
    transcript: &[DebateMessage],
    round: usize,
    bounded_context_chars: usize,
) -> String {
    let operator = MUTATION_OPERATORS[(round + agent.id.len()) % MUTATION_OPERATORS.len()];
    let previous = bounded_transcript_for_phase(
        transcript,
        DebatePhase::Mutation,
        bounded_context_chars,
    );
    format!(
        "You are {role}. Your framework: {framework}\n\
Phase: MUTATION.\n\
Apply the `{operator}` transformation to the current combined proposal.\n\
Turn a weakness into a usable advantage or reduce unnecessary complexity.\n\
Combination context: {combination_output}\n\
Previous mutation discussion: {previous}\n\
Your mutated proposal:",
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
