use crate::bootstrap::{self, BootstrapPaths};
use crate::config::ForjaConfig;
use crate::provider_registry::ProviderRegistry;
use crate::runtime::prompt::{
    build_system_prompt, exec_mode_label, load_image_base64, role_label, think_level_label,
};
use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
use forja_core::engine::{SlashCommandResult, SlashHandler};
use forja_core::mode::{
    detect_image_path, parse_image_command, parse_screenshot_command, parse_slash_command, ExecMode,
    ModeState, SlashCommand,
};
use forja_core::traits::{Channel, LlmProvider};
use forja_tools::{ScreenCaptureBackend, VisionAnalyzer};
use std::sync::{Arc, Mutex};

pub(crate) struct SlashHandlerDeps {
    pub(crate) cfg_for_handler: ForjaConfig,
    pub(crate) registry: ProviderRegistry,
    pub(crate) channel: Arc<dyn Channel>,
    pub(crate) bootstrap_paths: BootstrapPaths,
    pub(crate) interactive_identity_supported: bool,
    pub(crate) exec_mode_handle: Arc<Mutex<ExecMode>>,
    pub(crate) vision_enabled: bool,
    pub(crate) capture_backend: Arc<dyn ScreenCaptureBackend>,
    pub(crate) vision_analyzer: Arc<dyn VisionAnalyzer>,
}

pub(crate) fn build_slash_handler(deps: SlashHandlerDeps) -> SlashHandler {
    let registry = Mutex::new(deps.registry);
    let cfg_for_handler = deps.cfg_for_handler;
    let channel_for_slash = deps.channel;
    let bootstrap_paths_for_slash = deps.bootstrap_paths;
    let interactive_identity_supported = deps.interactive_identity_supported;
    let exec_mode_handle_for_slash = deps.exec_mode_handle;
    let vision_enabled_for_slash = deps.vision_enabled;
    let capture_backend_for_slash = deps.capture_backend;
    let vision_analyzer_for_slash = deps.vision_analyzer;

    Arc::new(
        move |text: &str, provider: &mut Arc<dyn LlmProvider>, mode_state: &mut ModeState| {
            let text = text.trim();

            if vision_enabled_for_slash {
                if let Some(prompt) = parse_screenshot_command(text) {
                    println!("[Vision] Captured the screen. Analyzing...");
                    let prompt = if prompt.trim().is_empty() {
                        "Describe what you see on screen.".to_string()
                    } else {
                        prompt
                    };
                    let result = tokio::task::block_in_place(|| {
                        tokio::runtime::Handle::current().block_on(async {
                            let capture = capture_backend_for_slash.capture_full().await?;
                            let image_base64 = BASE64_STANDARD.encode(capture);
                            vision_analyzer_for_slash
                                .analyze_image(&image_base64, &prompt)
                                .await
                        })
                    });

                    return Some(SlashCommandResult::ReplyAndSave {
                        user_text: text.to_string(),
                        reply: match result {
                            Ok(reply) => reply,
                            Err(error) => format!("❌ Vision analysis failed: {error}"),
                        },
                    });
                }

                if let Some((path, prompt)) = parse_image_command(text) {
                    let prompt = if prompt.trim().is_empty() {
                        "Describe what you see in this image.".to_string()
                    } else {
                        prompt
                    };
                    let image_base64 = match load_image_base64(&path) {
                        Ok(image_base64) => image_base64,
                        Err(error) => {
                            return Some(SlashCommandResult::Reply(format!(
                                "❌ Could not read the image file: {error}"
                            )))
                        }
                    };
                    let result = tokio::task::block_in_place(|| {
                        tokio::runtime::Handle::current().block_on(async {
                            vision_analyzer_for_slash
                                .analyze_image(&image_base64, &prompt)
                                .await
                        })
                    });

                    return Some(SlashCommandResult::ReplyAndSave {
                        user_text: text.to_string(),
                        reply: match result {
                            Ok(reply) => reply,
                            Err(error) => format!("❌ Vision analysis failed: {error}"),
                        },
                    });
                }

                if let Some((path, prompt)) = detect_image_path(text) {
                    match load_image_base64(&path) {
                        Ok(image_base64) => {
                            let prompt = if prompt.trim().is_empty() {
                                "Describe what you see in this image.".to_string()
                            } else {
                                prompt
                            };
                            let result = tokio::task::block_in_place(|| {
                                tokio::runtime::Handle::current().block_on(async {
                                    vision_analyzer_for_slash
                                        .analyze_image(&image_base64, &prompt)
                                        .await
                                })
                            });

                            return Some(SlashCommandResult::ReplyAndSave {
                                user_text: text.to_string(),
                                reply: match result {
                                    Ok(reply) => reply,
                                    Err(error) => format!("❌ Vision analysis failed: {error}"),
                                },
                            });
                        }
                        Err(error) => {
                            eprintln!("[Vision] failed to load image '{}': {error}", path.display());
                        }
                    }
                }
            }

            if let Some(command) = parse_slash_command(text) {
                match command {
                    SlashCommand::Mode(mode) => {
                        mode_state.update_exec_mode(mode);
                        if let Ok(mut shared_mode) = exec_mode_handle_for_slash.lock() {
                            *shared_mode = mode;
                        }
                        return Some(SlashCommandResult::Reply(format!(
                            "Mode updated: {}",
                            exec_mode_label(mode)
                        )));
                    }
                    SlashCommand::Think(level) => {
                        mode_state.update_think_level(level);
                        return Some(SlashCommandResult::Reply(format!(
                            "Think updated: {}",
                            think_level_label(level)
                        )));
                    }
                    SlashCommand::Role(role) => {
                        mode_state.update_role(role);
                        return Some(SlashCommandResult::Reply(format!(
                            "Role updated: {}",
                            role_label(role)
                        )));
                    }
                }
            }

            if text == "/debate" {
                return Some(SlashCommandResult::Reply("Usage: /debate <topic>".to_string()));
            }

            if let Some(topic) = text.strip_prefix("/debate ") {
                let topic = topic.trim();
                if topic.is_empty() {
                    return Some(SlashCommandResult::Reply("Usage: /debate <topic>".to_string()));
                }

                return Some(SlashCommandResult::Debate {
                    topic: topic.to_string(),
                });
            }

            if text == "/dashboard" {
                return Some(SlashCommandResult::Dashboard);
            }

            if text == "/skills" {
                return Some(SlashCommandResult::Skills);
            }

            if text == "/unresolved" {
                return Some(SlashCommandResult::Unresolved);
            }

            if let Some(description) = text.strip_prefix("/task ") {
                let description = description.trim();
                if description.is_empty() {
                    return Some(SlashCommandResult::Reply(
                        "Usage: /task <description>".to_string(),
                    ));
                }

                return Some(SlashCommandResult::Task {
                    description: description.to_string(),
                });
            }

            if text == "/models" {
                let registry = registry.lock().unwrap();
                return Some(SlashCommandResult::Reply(
                    registry.list_for_config(&cfg_for_handler),
                ));
            }

            if text == "/model" {
                let registry = registry.lock().unwrap();
                let entry = registry.active();
                return Some(SlashCommandResult::Reply(format!(
                    "Current model: **{}** ({}/{})",
                    entry.label, entry.provider, entry.model_id
                )));
            }

            if let Some(target) = text.strip_prefix("/model ") {
                let mut registry = registry.lock().unwrap();
                match registry.resolve(target, &cfg_for_handler) {
                    None => {
                        return Some(SlashCommandResult::Reply(format!(
                            "❌ Could not find model '{}'. Check `/models` for the list.",
                            target
                        )))
                    }
                    Some(idx) => match registry.switch_to(idx, &cfg_for_handler) {
                        Err(error) => {
                            return Some(SlashCommandResult::Reply(format!(
                                "❌ Switch failed: {error}"
                            )))
                        }
                        Ok(new_config) => match forja_llm::LlmClient::new(new_config) {
                            Err(error) => {
                                return Some(SlashCommandResult::Reply(format!(
                                    "❌ Failed to create LlmClient: {error}"
                                )))
                            }
                            Ok(client) => {
                                let entry = registry.active();
                                *provider = Arc::new(client);
                                return Some(SlashCommandResult::Reply(format!(
                                    "✅ Switched model: **{}** ({}/{})",
                                    entry.label, entry.provider, entry.model_id
                                )));
                            }
                        },
                    },
                }
            }

            if text == "/identity" {
                if !interactive_identity_supported || !channel_for_slash.is_cli_source() {
                    return Some(SlashCommandResult::Reply(
                        "This command is only supported in CLI-only mode.".to_string(),
                    ));
                }

                let outcome = match bootstrap::reset_bootstrap(&bootstrap_paths_for_slash) {
                    Ok(outcome) => outcome,
                    Err(error) => {
                        return Some(SlashCommandResult::Reply(format!(
                            "❌ Identity reset failed: {error}"
                        )))
                    }
                };

                let system_prompt = match build_system_prompt(&bootstrap_paths_for_slash) {
                    Ok((system_prompt, _)) => system_prompt,
                    Err(error) => {
                        return Some(SlashCommandResult::Reply(format!(
                            "❌ Failed to rebuild the system prompt: {error}"
                        )))
                    }
                };

                return Some(SlashCommandResult::UpdateSystemPrompt {
                    reply: outcome.greeting.unwrap_or_default(),
                    system_prompt: Some(system_prompt),
                    reset_history: true,
                });
            }

            None
        },
    )
}

#[cfg(test)]
mod tests;
