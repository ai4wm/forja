use crate::bootstrap::{self, BootstrapPaths};
use crate::config::ForjaConfig;
use crate::local_models::{download_hugging_face_model, parse_hf_repo};
use crate::provider_registry::ProviderRegistry;
use crate::runtime::prompt::{build_system_prompt, load_image_base64};
use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
use forja_core::engine::{SlashCommandResult, SlashHandler};
use forja_core::mode::{
    detect_image_path, parse_image_command, parse_natural_language_command,
    parse_screenshot_command, parse_slash_command, ExecMode, ModeState, SlashCommand,
};
use forja_core::skill::SkillRegistry;
use forja_core::traits::{Channel, LlmProvider, VoiceChannelStatus};
use forja_tools::{ScreenCaptureBackend, VisionAnalyzer};
use std::io::Write;
use std::sync::{Arc, Mutex};

pub(crate) type StateChangeConfirmer = Arc<dyn Fn(&str) -> bool + Send + Sync>;

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
    pub(crate) state_change_confirmer: Option<StateChangeConfirmer>,
    pub(crate) skill_registry: Arc<SkillRegistry>,
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
    let state_change_confirmer = deps.state_change_confirmer;
    let skill_registry = deps.skill_registry;

    Arc::new(
        move |text: &str, provider: &mut Arc<dyn LlmProvider>, mode_state: &mut ModeState| {
            let mut effective_text = text.trim().to_string();

            if !effective_text.starts_with('/')
                && let Some(mapped) = parse_natural_language_command(&effective_text)
            {
                let confirmed = match confirm_state_change(
                    channel_for_slash.as_ref(),
                    state_change_confirmer.as_ref(),
                    &mapped.confirmation_prompt(),
                ) {
                    Some(confirmed) => confirmed,
                    None => return None,
                };

                if !confirmed {
                    return Some(SlashCommandResult::Reply("Canceled.".to_string()));
                }

                effective_text = mapped.to_slash_command();
            }

            if !effective_text.starts_with('/')
                && let Ok(Some(skill)) = skill_registry.match_trigger(&effective_text)
            {
                return Some(SlashCommandResult::Skill { name: skill.name });
            }

            if vision_enabled_for_slash {
                if let Some(prompt) = parse_screenshot_command(&effective_text) {
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
                        user_text: effective_text.clone(),
                        reply: match result {
                            Ok(reply) => reply,
                            Err(error) => format!("❌ Vision analysis failed: {error}"),
                        },
                    });
                }

                if let Some((path, prompt)) = parse_image_command(&effective_text) {
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
                        user_text: effective_text.clone(),
                        reply: match result {
                            Ok(reply) => reply,
                            Err(error) => format!("❌ Vision analysis failed: {error}"),
                        },
                    });
                }

                if let Some((path, prompt)) = detect_image_path(&effective_text) {
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
                                user_text: effective_text.clone(),
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

            if let Some(command) = parse_slash_command(&effective_text) {
                match command {
                    SlashCommand::Mode(mode) => {
                        mode_state.update_exec_mode(mode);
                        if let Ok(mut shared_mode) = exec_mode_handle_for_slash.lock() {
                            *shared_mode = mode;
                        }
                        return Some(SlashCommandResult::Reply(format!(
                            "Mode updated: {}",
                            mode.as_str()
                        )));
                    }
                    SlashCommand::Think(level) => {
                        mode_state.update_think_level(level);
                        return Some(SlashCommandResult::Reply(format!(
                            "Think updated: {}",
                            level.as_str()
                        )));
                    }
                    SlashCommand::Role(role) => {
                        mode_state.update_role(role);
                        return Some(SlashCommandResult::Reply(format!(
                            "Role updated: {}",
                            role.as_str()
                        )));
                    }
                }
            }

            if effective_text == "/debate" {
                return Some(SlashCommandResult::Reply("Usage: /debate <topic>".to_string()));
            }

            if let Some(topic) = effective_text.strip_prefix("/debate ") {
                let topic = topic.trim();
                if topic.is_empty() {
                    return Some(SlashCommandResult::Reply("Usage: /debate <topic>".to_string()));
                }

                return Some(SlashCommandResult::Debate {
                    topic: topic.to_string(),
                });
            }

            if effective_text == "/dashboard" {
                return Some(SlashCommandResult::Dashboard);
            }

            if effective_text == "/tui" {
                return Some(SlashCommandResult::Tui);
            }

            if effective_text == "/dream" {
                return Some(SlashCommandResult::Dream);
            }

            if effective_text == "/skills" {
                return Some(SlashCommandResult::Skills);
            }

            if effective_text == "/notify" {
                return Some(SlashCommandResult::Reply(
                    "Usage: /notify <on|off|status>".to_string(),
                ));
            }

            if let Some(command) = effective_text.strip_prefix("/notify ") {
                let command = command.trim().to_lowercase();
                let reply = match command.as_str() {
                    "on" => {
                        let state = tokio::task::block_in_place(|| {
                            tokio::runtime::Handle::current().block_on(async {
                                channel_for_slash
                                    .set_notifications_enabled(true)
                                    .await
                                    .unwrap_or_default()
                            })
                        });
                        format!("Notifications enabled (min level: {}).", state.min_level.as_str())
                    }
                    "off" => {
                        let _ = tokio::task::block_in_place(|| {
                            tokio::runtime::Handle::current().block_on(async {
                                channel_for_slash
                                    .set_notifications_enabled(false)
                                    .await
                                    .unwrap_or_default()
                            })
                        });
                        "Notifications disabled.".to_string()
                    }
                    "status" => match channel_for_slash.notification_state() {
                        Some(state) => format!(
                            "Notifications: {} (min level: {})",
                            if state.enabled { "on" } else { "off" },
                            state.min_level.as_str()
                        ),
                        None => "Notification system unavailable.".to_string(),
                    },
                    _ => "Usage: /notify <on|off|status>".to_string(),
                };
                return Some(SlashCommandResult::Reply(reply));
            }

            if effective_text == "/voice" {
                return Some(SlashCommandResult::Reply(
                    "Usage: /voice <on|off|status>".to_string(),
                ));
            }

            if let Some(command) = effective_text.strip_prefix("/voice ") {
                let command = command.trim().to_lowercase();
                let reply = match command.as_str() {
                    "on" => {
                        let status = tokio::task::block_in_place(|| {
                            tokio::runtime::Handle::current().block_on(async {
                                channel_for_slash
                                    .set_voice_enabled(true)
                                    .await
                                    .unwrap_or(VoiceChannelStatus::Unavailable)
                            })
                        });
                        format_voice_status(status, true)
                    }
                    "off" => {
                        let status = tokio::task::block_in_place(|| {
                            tokio::runtime::Handle::current().block_on(async {
                                channel_for_slash
                                    .set_voice_enabled(false)
                                    .await
                                    .unwrap_or(VoiceChannelStatus::Unavailable)
                            })
                        });
                        format_voice_status(status, false)
                    }
                    "status" => match channel_for_slash.voice_status() {
                        Some(status) => format!("Voice status: {}", voice_status_label(status)),
                        None => "Voice channel unavailable.".to_string(),
                    },
                    _ => "Usage: /voice <on|off|status>".to_string(),
                };
                return Some(SlashCommandResult::Reply(reply));
            }

            if effective_text == "/skill" {
                return Some(SlashCommandResult::Reply(
                    "Usage: /skill <name>".to_string(),
                ));
            }

            if let Some(name) = effective_text.strip_prefix("/skill ") {
                let name = name.trim();
                if name.is_empty() {
                    return Some(SlashCommandResult::Reply(
                        "Usage: /skill <name>".to_string(),
                    ));
                }

                if name.eq_ignore_ascii_case("list") {
                    return Some(SlashCommandResult::Skills);
                }

                return Some(SlashCommandResult::Skill {
                    name: name.to_string(),
                });
            }

            if effective_text == "/unresolved" {
                return Some(SlashCommandResult::Unresolved);
            }

            if let Some(description) = effective_text.strip_prefix("/task ") {
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

            if let Some(command) = effective_text.strip_prefix("/autonomy ") {
                let command = command.trim();
                if command.is_empty() {
                    return Some(SlashCommandResult::Reply(
                        "Usage: /autonomy <start|stop|status>".to_string(),
                    ));
                }

                return Some(SlashCommandResult::AutonomyCommand {
                    command: command.to_string(),
                });
            }

            if effective_text == "/models" {
                let registry = registry.lock().unwrap();
                return Some(SlashCommandResult::Reply(
                    registry.list_for_config(&cfg_for_handler),
                ));
            }

            if effective_text == "/model" {
                let registry = registry.lock().unwrap();
                let entry = registry.active();
                return Some(SlashCommandResult::Reply(format!(
                    "Current model: **{}** ({}/{})",
                    entry.label, entry.provider, entry.model_id
                )));
            }

            if let Some(target) = effective_text.strip_prefix("/model ") {
                if let Some(spec) = target.trim().strip_prefix("fetch ") {
                    let mut args = spec.split_whitespace();
                    let repo_input = args.next().unwrap_or_default();
                    let filename = args.next();
                    let parsed = match parse_hf_repo(repo_input, filename) {
                        Ok(parsed) => parsed,
                        Err(error) => return Some(SlashCommandResult::Reply(error)),
                    };

                    println!("[Model] Downloading {}...", parsed.repo_id);
                    let mut last_progress = 0u64;
                    let result = tokio::task::block_in_place(|| {
                        tokio::runtime::Handle::current().block_on(async {
                            download_hugging_face_model(parsed, |downloaded, total| {
                                if let Some(total) = total {
                                    let percent = if total == 0 {
                                        0
                                    } else {
                                        downloaded.saturating_mul(100) / total
                                    };
                                    if percent >= last_progress + 10 || percent == 100 {
                                        println!("[Model] Download progress: {percent}%");
                                        last_progress = percent;
                                    }
                                }
                            })
                            .await
                        })
                    });

                    return Some(SlashCommandResult::Reply(match result {
                        Ok(model) => {
                            let mut registry = registry.lock().unwrap();
                            registry.refresh(&cfg_for_handler);
                            format!(
                                "✅ Downloaded local model: {} (`{}`)",
                                model.display_name, model.model_id
                            )
                        }
                        Err(error) => format!("❌ Model download failed: {error}"),
                    }));
                }

                if let Some(spec) = target.trim().strip_prefix("bootstrap ") {
                    let mut args = spec.split_whitespace();
                    let repo_input = args.next().unwrap_or_default();
                    let filename = args.next();
                    let parsed = match parse_hf_repo(repo_input, filename) {
                        Ok(parsed) => parsed,
                        Err(error) => return Some(SlashCommandResult::Reply(error)),
                    };

                    println!("[Model] Bootstrapping {}...", parsed.repo_id);
                    let mut last_progress = 0u64;
                    let result = tokio::task::block_in_place(|| {
                        tokio::runtime::Handle::current().block_on(async {
                            download_hugging_face_model(parsed, |downloaded, total| {
                                if let Some(total) = total {
                                    let percent = if total == 0 {
                                        0
                                    } else {
                                        downloaded.saturating_mul(100) / total
                                    };
                                    if percent >= last_progress + 10 || percent == 100 {
                                        println!("[Model] Download progress: {percent}%");
                                        last_progress = percent;
                                    }
                                }
                            })
                            .await
                        })
                    });

                    let downloaded_model = match result {
                        Ok(model) => model,
                        Err(error) => {
                            return Some(SlashCommandResult::Reply(format!(
                                "❌ Model bootstrap failed: {error}"
                            )))
                        }
                    };

                    let mut registry = registry.lock().unwrap();
                    registry.refresh(&cfg_for_handler);
                    let Some(idx) = registry.resolve(&downloaded_model.model_id, &cfg_for_handler) else {
                        return Some(SlashCommandResult::Reply(format!(
                            "✅ Downloaded `{}` but could not resolve it in /models yet.",
                            downloaded_model.model_id
                        )));
                    };
                    return Some(match registry.switch_to(idx, &cfg_for_handler) {
                        Err(error) => SlashCommandResult::Reply(format!(
                            "❌ Downloaded `{}` but could not activate it: {error}",
                            downloaded_model.model_id
                        )),
                        Ok(new_config) => match forja_llm::LlmClient::new(new_config) {
                            Err(error) => SlashCommandResult::Reply(format!(
                                "❌ Downloaded `{}` but failed to create LlmClient: {error}",
                                downloaded_model.model_id
                            )),
                            Ok(client) => {
                                let entry = registry.active();
                                *provider = Arc::new(client);
                                SlashCommandResult::Reply(format!(
                                    "✅ Bootstrapped and switched to **{}** ({}/{})",
                                    entry.label, entry.provider, entry.model_id
                                ))
                            }
                        },
                    });
                }

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

fn confirm_state_change(
    channel: &dyn Channel,
    injected: Option<&StateChangeConfirmer>,
    prompt: &str,
) -> Option<bool> {
    if let Some(confirmer) = injected {
        return Some(confirmer(prompt));
    }

    if !channel.is_cli_source() {
        return None;
    }

    print!("{prompt} (y/n) > ");
    std::io::stdout().flush().ok()?;
    let mut line = String::new();
    std::io::stdin().read_line(&mut line).ok()?;
    let normalized = line.trim().to_lowercase();
    Some(matches!(normalized.as_str(), "y" | "yes" | "예" | "ㅇ"))
}

fn format_voice_status(status: VoiceChannelStatus, requested_enabled: bool) -> String {
    match status {
        VoiceChannelStatus::Disabled if !requested_enabled => {
            "Voice channel disabled.".to_string()
        }
        VoiceChannelStatus::Listening if requested_enabled => {
            "Voice channel enabled and listening.".to_string()
        }
        VoiceChannelStatus::Speaking => "Voice channel enabled and speaking.".to_string(),
        VoiceChannelStatus::Unavailable => {
            "Voice channel unavailable on this system.".to_string()
        }
        _ => format!("Voice status: {}", voice_status_label(status)),
    }
}

fn voice_status_label(status: VoiceChannelStatus) -> &'static str {
    match status {
        VoiceChannelStatus::Disabled => "disabled",
        VoiceChannelStatus::Listening => "listening",
        VoiceChannelStatus::Speaking => "speaking",
        VoiceChannelStatus::Unavailable => "unavailable",
    }
}

#[cfg(test)]
mod tests;
