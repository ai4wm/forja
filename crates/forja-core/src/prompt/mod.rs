pub mod analyst;
pub mod assistant;
pub mod base;
pub mod coder;
pub mod loader;
pub mod think;
pub mod writer;

use crate::mode::{ModeState, Role};

#[allow(clippy::too_many_arguments)]
pub fn assemble_system_prompt(
    prompt_loader: &loader::PromptLoader,
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
    let mut sections = vec![prompt_loader.load_base(assistant_name, user_title)];

    let think = prompt_loader.load_think(match mode_state.think_level {
        crate::mode::ThinkLevel::Min => "min",
        crate::mode::ThinkLevel::Mid => "mid",
        crate::mode::ThinkLevel::Max => "max",
    });
    if !think.is_empty() {
        sections.push(think);
    }

    let role_prompt = match mode_state.effective_role() {
        Role::Coder => prompt_loader.load_role("coder"),
        Role::Writer => prompt_loader.load_role("writer"),
        Role::Assistant => prompt_loader.load_role("assistant"),
        Role::Analyst => prompt_loader.load_role("analyst"),
        Role::Auto | Role::Default => String::new(),
    };
    if !role_prompt.is_empty() {
        sections.push(role_prompt);
    }

    for section in [
        identity,
        user,
        tools,
        emotion_tone,
        relationship,
        knowledge,
        memory,
    ] {
        let trimmed = section.trim();
        if !trimmed.is_empty() {
            sections.push(trimmed.to_string());
        }
    }

    sections.join("\n\n")
}
