use crate::bootstrap;
use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
use forja_core::error::{ForjaError, Result};
use forja_core::prompt::join_prompt_sections;
use forja_core::traits::LlmProvider;
use forja_core::{Content, Message, Role};
use std::path::Path;
use std::sync::Arc;

fn load_project_prompt() -> Option<(String, String)> {
    let candidates = ["AGENTS.md", "FORJA.md", "CLAUDE.md"];

    for file in candidates {
        if let Ok(content) = std::fs::read_to_string(file)
            && !content.trim().is_empty()
        {
            return Some((file.to_string(), content.trim().to_string()));
        }
    }

    None
}

pub(crate) fn build_system_prompt(
    bootstrap_paths: &bootstrap::BootstrapPaths,
) -> std::io::Result<(String, Option<String>)> {
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    let bootstrap_prompt = bootstrap::compose_system_prompt_prefix(bootstrap_paths)?;
    let project_prompt = load_project_prompt();
    let loaded_project_file = project_prompt
        .as_ref()
        .map(|(file_name, _)| file_name.clone());
    let mut sections = Vec::new();
    sections.push(bootstrap_prompt);
    if let Some((_, project_content)) = project_prompt {
        sections.push(project_content);
    }
    let mut combined_prompt = join_prompt_sections(sections, "\n\n---\n\n");

    if !combined_prompt.is_empty() {
        combined_prompt.push_str(&format!(
            "\n\nToday's date is {today}. This date is correct. If a search result has the same date, treat it as current."
        ));
    }

    Ok((combined_prompt, loaded_project_file))
}

pub(crate) fn build_tool_prompt(
    shell_enabled: bool,
    input_enabled: bool,
    browser_enabled: bool,
    vision_enabled: bool,
) -> String {
    let mut sections = Vec::new();

    if shell_enabled {
        sections.push(
            "You have access to a shell tool that can execute OS commands.\n\
When the user asks you to perform a system task (open app,\n\
manage files, check system info, etc.), use the shell tool.\n\
For Windows, use PowerShell commands.\n\
For macOS/Linux, use bash commands.\n\
Always prefer safe, non-destructive commands.\n\
Example: user says 'open notepad' -> shell: Start-Process notepad\n\
Example: user says 'what time is it' -> shell: Get-Date\n\
Example: user says 'list files' -> shell: Get-ChildItem"
                .to_string(),
        );
    }

    if input_enabled {
        sections.push(
            "Tool: input\n\
Actions: type_text, key_press, hotkey, mouse_move, mouse_click, mouse_double_click, mouse_drag, scroll\n\
Example: {\"tool\":\"input\",\"action\":\"type_text\",\"text\":\"hello\"}\n\
Example: {\"tool\":\"input\",\"action\":\"hotkey\",\"keys\":[\"ctrl\",\"s\"]}\n\
Example: {\"tool\":\"input\",\"action\":\"mouse_click\",\"button\":\"left\",\"x\":500,\"y\":300}\n\
Example: {\"tool\":\"input\",\"action\":\"scroll\",\"direction\":\"down\",\"amount\":3}"
                .to_string(),
        );
    }

    if browser_enabled {
        sections.push(
            "Tool: browser\n\
Actions: open, goto, scroll, click, type_text, read_text, read_page, screenshot, evaluate, tab_list, tab_switch, tab_close, back, forward\n\
Example: {\"tool\":\"browser\",\"action\":\"open\",\"url\":\"https://google.com\"}\n\
Example: {\"tool\":\"browser\",\"action\":\"click\",\"selector\":\"button.submit\"}\n\
Example: {\"tool\":\"browser\",\"action\":\"type_text\",\"selector\":\"input#search\",\"text\":\"forja rust\"}\n\
Example: {\"tool\":\"browser\",\"action\":\"scroll\",\"direction\":\"down\",\"amount\":500}\n\
Example: {\"tool\":\"browser\",\"action\":\"read_text\",\"selector\":\"h1\"}\n\
Example: {\"tool\":\"browser\",\"action\":\"screenshot\"}"
                .to_string(),
        );
    }

    if vision_enabled {
        sections.push(
            "Tool: vision\n\
Actions: capture_screen, capture_region, analyze, analyze_region, find_element, ocr\n\
Example: {\"tool\":\"vision\",\"action\":\"analyze\",\"prompt\":\"What is on the screen?\"}\n\
Example: {\"tool\":\"vision\",\"action\":\"find_element\",\"description\":\"red login button\"}\n\
Example: {\"tool\":\"vision\",\"action\":\"capture_region\",\"x\":100,\"y\":200,\"width\":500,\"height\":300}\n\
Example: {\"tool\":\"vision\",\"action\":\"ocr\",\"x\":0,\"y\":0,\"width\":1920,\"height\":1080}\n\
Note: Chain find_element result with input tool mouse_click to click visual elements."
                .to_string(),
        );
    }

    join_prompt_sections(sections, "\n\n")
}

pub(crate) fn load_image_base64(path: &Path) -> std::io::Result<String> {
    let bytes = std::fs::read(path)?;
    Ok(BASE64_STANDARD.encode(bytes))
}

pub(crate) fn auto_summarize_enabled() -> bool {
    !matches!(
        std::env::var("FORJA_AUTO_SUMMARIZE"),
        Ok(value) if value.eq_ignore_ascii_case("false")
    )
}

pub(crate) async fn summarize_memory_block(
    provider: Arc<dyn LlmProvider>,
    block: String,
) -> Result<String> {
    let response = provider
        .chat(
            &[
                Message::text(
                    Role::System,
                    "Summarize one daily memory.md block into at most three plain-text lines. Return only the summary lines.",
                    None,
                ),
                Message::text(
                    Role::User,
                    format!(
                        "Summarize the following daily memory.md records in max 3 lines.\n\
Keep only important preferences, decisions, and ongoing work. Answer in plain text only.\n\
\n{block}"
                    ),
                    None,
                ),
            ],
            None,
        )
        .await?;

    match response.content {
        Content::Text { text, .. } => Ok(text),
        _ => Err(ForjaError::LlmError(
            "memory summary response was not text".to_string(),
        )),
    }
}
