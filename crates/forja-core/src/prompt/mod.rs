pub mod analyst;
pub mod assistant;
pub mod base;
pub mod coder;
pub mod think;
pub mod writer;

use crate::mode::{ModeState, Role};

pub fn join_prompt_sections<I, S>(sections: I, separator: &str) -> String
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    sections
        .into_iter()
        .filter_map(|section| {
            let trimmed = section.as_ref().trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        })
        .collect::<Vec<_>>()
        .join(separator)
}

#[allow(clippy::too_many_arguments)]
pub fn assemble_system_prompt(
    mode_state: &ModeState,
    assistant_name: &str,
    user_title: &str,
    identity: &str,
    user: &str,
    tools: &str,
    emotion_tone: &str,
    relationship: &str,
    knowledge: &str,
    memory: &str,
) -> String {
    let mut sections = vec![base::base_prompt(assistant_name, user_title)];

    let think = think::think_prompt(mode_state.think_level);
    if !think.is_empty() {
        sections.push(think.to_string());
    }

    let role_prompt = match mode_state.effective_role() {
        Role::Coder => coder::CODER_PROMPT,
        Role::Writer => writer::WRITER_PROMPT,
        Role::Assistant => assistant::ASSISTANT_PROMPT,
        Role::Analyst => analyst::ANALYST_PROMPT,
        Role::Auto | Role::Default => "",
    };
    if !role_prompt.is_empty() {
        sections.push(role_prompt.to_string());
    }

    sections.extend(
        [identity, user, tools, emotion_tone, relationship, knowledge, memory]
            .into_iter()
            .map(str::to_string),
    );

    join_prompt_sections(sections, "\n\n")
}
