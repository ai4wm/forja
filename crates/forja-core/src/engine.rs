use crate::audit::logger::AuditLogger;
use crate::autonomy::loop_runner::AutonomousLoop;
use crate::budget::{manager::BudgetManager, BudgetMode};
use crate::creation::DebateEngine;
use crate::context::token_counter::{count_message_tokens, count_messages_tokens};
use crate::context::SummaryCallback;
use crate::error::{ForjaError, Result};
use crate::emotion::EmotionEngine;
use crate::gateway::Envelope;
use crate::heartbeat::scheduler::HeartbeatScheduler;
use crate::knowledge::KnowledgeManager;
use crate::mode::ModeState;
use crate::prompt::assemble_system_prompt;
use crate::ralf::executor::ralf_execute;
use crate::ralf::{RalfConfig, RalfState};
use crate::serendipity::SerendipityEngine;
use crate::traits::{Channel, LlmProvider, Tool};
use crate::types::{Content, Message, Role, ToolDefinition};
use chrono::{DateTime, Local};
use std::collections::HashMap;
use std::sync::Arc;
mod audit;
mod autonomy;
mod budget;
mod creation;
mod dashboard;
mod emotion;
mod context;
mod heartbeat;
mod knowledge;
mod mode;
mod serendipity;
#[cfg(feature = "memory")]
mod memory;

#[cfg(feature = "memory")]
use crate::traits::MemoryStore;
use self::context::EngineContextDefaults;

const MAX_TOOL_DEPTH: usize = 10;

pub enum SlashCommandResult {
    Reply(String),
    ReplyAndSave { user_text: String, reply: String },
    Debate { topic: String },
    Dashboard,
    Task { description: String },
    Skills,
    Unresolved,
    UpdateSystemPrompt {
        reply: String,
        system_prompt: Option<String>,
        reset_history: bool,
    },
}

/// Slash command callback type for /models and /model.
pub type SlashHandler = Arc<dyn Fn(&str, &mut Arc<dyn LlmProvider>, &mut ModeState) -> Option<SlashCommandResult> + Send + Sync>;
pub type DashboardHandler = Arc<dyn Fn() -> Result<String> + Send + Sync>;

/// Core Forja engine. Coordinates channels, LLM providers, and tools,
/// and drives the main event loop plus recursive tool evaluation.
pub struct Engine {
    provider: Arc<dyn LlmProvider>,
    #[cfg_attr(not(feature = "runtime"), allow(dead_code))]
    channel: Arc<dyn Channel>,
    tools: HashMap<String, Arc<dyn Tool>>,
    conversation_history: Vec<Message>,
    total_tokens: usize,
    max_context_tokens: usize,
    context_model: String,
    context_warning_emitted: bool,
    context_summary_callback: Option<SummaryCallback>,
    budget_manager: Option<Arc<BudgetManager>>,
    budget_mode: BudgetMode,
    current_agent_id: String,
    creation_engine: Option<DebateEngine>,
    autonomy: Option<AutonomousLoop>,
    audit_logger: Option<Arc<AuditLogger>>,
    heartbeat_scheduler: Option<HeartbeatScheduler>,
    heartbeat_sender: tokio::sync::mpsc::Sender<Envelope>,
    heartbeat_receiver: Option<tokio::sync::mpsc::Receiver<Envelope>>,
    ralf_config: RalfConfig,
    system_prompt: Option<String>,
    tool_prompt: Option<String>,
    assistant_name: String,
    user_title: String,
    slash_handler: Option<SlashHandler>,
    dashboard_handler: Option<DashboardHandler>,
    mode_state: ModeState,
    emotion: Option<EmotionEngine>,
    turn_tone_context: Option<String>,
    turn_relationship_context: Option<String>,
    knowledge: Option<Arc<KnowledgeManager>>,
    turn_knowledge_context: Option<String>,
    serendipity: Option<SerendipityEngine>,
    turn_count: u32,
    last_serendipity_triggered_at: Option<DateTime<Local>>,
    #[cfg(feature = "memory")]
    memory: Option<Arc<dyn MemoryStore>>,
    #[cfg(feature = "memory")]
    turn_memory_context: Option<String>,
}

impl Engine {
    pub fn new(provider: Arc<dyn LlmProvider>, channel: Arc<dyn Channel>) -> Self {
        let context_defaults = EngineContextDefaults::default();
        let (heartbeat_sender, heartbeat_receiver) = tokio::sync::mpsc::channel(32);

        Self {
            provider,
            channel,
            tools: HashMap::new(),
            conversation_history: Vec::new(),
            total_tokens: 0,
            max_context_tokens: context_defaults.max_context_tokens,
            context_model: context_defaults.context_model,
            context_warning_emitted: false,
            context_summary_callback: None,
            budget_manager: None,
            budget_mode: BudgetMode::Monitor,
            current_agent_id: "default".to_string(),
            creation_engine: None,
            autonomy: None,
            audit_logger: None,
            heartbeat_scheduler: None,
            heartbeat_sender,
            heartbeat_receiver: Some(heartbeat_receiver),
            ralf_config: RalfConfig::default(),
            system_prompt: None,
            tool_prompt: None,
            assistant_name: String::new(),
            user_title: String::new(),
            slash_handler: None,
            dashboard_handler: None,
            mode_state: ModeState::default(),
            emotion: None,
            turn_tone_context: None,
            turn_relationship_context: None,
            knowledge: None,
            turn_knowledge_context: None,
            serendipity: None,
            turn_count: 0,
            last_serendipity_triggered_at: None,
            #[cfg(feature = "memory")]
            memory: None,
            #[cfg(feature = "memory")]
            turn_memory_context: None,
        }
    }

    /// Sets a custom system prompt. History injection happens at request time.
    pub fn with_system_prompt(mut self, prompt: String) -> Self {
        self.system_prompt = Some(prompt);
        self
    }

    /// Optional memory store integration.
    #[cfg(feature = "memory")]
    pub fn with_memory(mut self, memory: Arc<dyn MemoryStore>) -> Self {
        self.memory = Some(memory);
        self
    }

    /// Registers the slash command handler.
    pub fn with_slash_handler(mut self, handler: SlashHandler) -> Self {
        self.slash_handler = Some(handler);
        self
    }

    /// Registers a tool with the engine.
    pub fn register_tool(&mut self, tool: Arc<dyn Tool>) {
        self.tools.insert(tool.name().to_string(), tool);
    }

    /// Swaps the LLM provider at runtime, for example after /model.
    pub fn swap_provider(&mut self, new_provider: Arc<dyn LlmProvider>) {
        self.provider = new_provider;
    }

    /// Returns Some for slash-command-like input and None otherwise.
    pub fn slash_response(&self, text: &str) -> Option<&'static str> {
        // Detection only. Actual handling is delegated to the caller.
        if text.trim_start().starts_with('/') { Some("") } else { None }
    }

    /// Adds a message to conversation history and updates the tracked token total.
    fn push_message(&mut self, msg: Message) {
        self.total_tokens = self
            .total_tokens
            .saturating_add(count_message_tokens(&msg, &self.context_model));
        self.conversation_history.push(msg);
    }

    fn request_messages(&self) -> Vec<Message> {
        let mut messages = Vec::new();
        let prompt = assemble_system_prompt(
            &self.mode_state,
            &self.assistant_name,
            &self.user_title,
            self.system_prompt.as_deref().unwrap_or_default(),
            "",
            self.tool_prompt.as_deref().unwrap_or_default(),
            self.turn_tone_context.as_deref().unwrap_or_default(),
            self.turn_relationship_context.as_deref().unwrap_or_default(),
            self.turn_knowledge_context.as_deref().unwrap_or_default(),
            #[cfg(feature = "memory")]
            self.turn_memory_context.as_deref().unwrap_or_default(),
            #[cfg(not(feature = "memory"))]
            "",
        );
        if !prompt.trim().is_empty() {
            messages.push(Message::text(Role::System, prompt, None));
        }

        messages.extend(self.conversation_history.clone());
        messages
    }

    #[cfg(feature = "runtime")]
    fn apply_system_prompt_update(
        &mut self,
        next_system_prompt: Option<String>,
        reset_history: bool,
    ) {
        self.system_prompt = next_system_prompt.clone();

        if reset_history {
            self.clear_conversation_history();
        }
        if let Some(system_prompt) = next_system_prompt {
            self.system_prompt = Some(system_prompt);
        }
    }

    pub fn shutdown(&mut self) {
        self.stop_heartbeat_runtime();
        self.autonomy = None;
        self.clear_conversation_history();
        self.clear_turn_knowledge_context();
        self.clear_turn_emotion_context();
        #[cfg(feature = "memory")]
        self.clear_turn_memory_context();
        self.turn_count = 0;
        self.last_serendipity_triggered_at = None;

        let (heartbeat_sender, heartbeat_receiver) = tokio::sync::mpsc::channel(32);
        self.heartbeat_sender = heartbeat_sender;
        self.heartbeat_receiver = Some(heartbeat_receiver);
    }

    /// Evaluates a single turn. Tool calls are executed and then re-interpreted recursively.
    #[async_recursion::async_recursion]
    pub async fn handle_step(&mut self, depth: usize) -> Result<Message> {
        if depth >= MAX_TOOL_DEPTH {
            return Err(ForjaError::MaxDepthExceeded(MAX_TOOL_DEPTH));
        }

        let tool_defs: Vec<ToolDefinition> = self.tools.values()
            .map(|t| t.definition())
            .collect();

        self.compress_context().await?;
        self.check_current_agent_budget()?;
        let request_messages = self.request_messages();
        let request_token_count = count_messages_tokens(&request_messages, &self.context_model);
        self.log_llm_call("chat", request_token_count);

        let provider = self.provider.clone();
        let tool_defs_for_retry = tool_defs.clone();
        let mut ralf_state = RalfState::default();
        let response_msg = ralf_execute(
            "llm_call",
            &self.ralf_config,
            &mut ralf_state,
            self.audit_logger.as_deref(),
            move || {
                let provider = provider.clone();
                let request_messages = request_messages.clone();
                let tool_defs = tool_defs_for_retry.clone();
                async move {
                    let tools = if tool_defs.is_empty() {
                        None
                    } else {
                        Some(tool_defs.as_slice())
                    };
                    provider.chat(&request_messages, tools).await
                }
            },
        )
        .await?;
        self.record_current_agent_usage(count_message_tokens(&response_msg, &self.context_model))?;

        match &response_msg.content {
            Content::ToolCall {
                call_id,
                tool_name,
                arguments,
                reasoning_content: _,
                thought_signature: _,
            } => {
                // Store the tool call request in history first.
                self.push_message(response_msg.clone());

                let result = if let Some(tool) = self.tools.get(tool_name).cloned() {
                    self.log_tool_call(tool_name, arguments);
                    let arguments = arguments.clone();
                    let mut ralf_state = RalfState::default();
                    let result = ralf_execute(
                        "tool_call",
                        &self.ralf_config,
                        &mut ralf_state,
                        self.audit_logger.as_deref(),
                        move || {
                            let tool = tool.clone();
                            let arguments = arguments.clone();
                            async move { tool.execute(arguments).await }
                        },
                    )
                    .await?;
                    self.log_tool_result(call_id, &result);
                    result
                } else {
                    let result = serde_json::json!({
                        "error": format!("Unknown tool requested: {}", tool_name)
                    });
                    self.log_tool_call(tool_name, arguments);
                    self.log_tool_result(call_id, &result);
                    result
                };

                let result_msg = Message::tool_result(call_id, result);
                self.push_message(result_msg);

                // Recurse so the model can interpret the tool result.
                self.handle_step(depth + 1).await
            }
            _ => {
                // End the turn for non-tool responses.
                self.push_message(response_msg.clone());
                Ok(response_msg)
            }
        }
    }

    /// Main event loop, available with the `runtime` feature.
    #[cfg(feature = "runtime")]
    pub async fn run<F>(&mut self, shutdown: F) -> Result<()>
    where
        F: std::future::Future<Output = ()> + Send,
    {
        tokio::pin!(shutdown);
        self.start_heartbeat_runtime()?;

        loop {
            let channel = self.channel.clone();
            tokio::select! {
                // Exit on shutdown signal.
                _ = &mut shutdown => {
                    break;
                }
                heartbeat = async {
                    match self.heartbeat_receiver.as_mut() {
                        Some(receiver) => receiver.recv().await,
                        None => None,
                    }
                } => {
                    if heartbeat.is_some() {
                        self.handle_autonomy_tick().await?;
                    }
                }
                // Wait for incoming messages from the channel.
                result = channel.receive() => {
                    let user_msg = result?;
                    self.push_message(user_msg.clone());
                    self.begin_user_turn();
                    self.refresh_turn_role(&user_msg);
                    let pre_spinner = start_pre_spinner();
                    self.refresh_turn_emotion_context().await;
                    self.refresh_turn_knowledge_context(&user_msg).await;

                    #[cfg(feature = "memory")]
                    self.refresh_turn_memory_context(&user_msg).await;
                    pre_spinner.finish_and_clear();

                    // Run one evaluation step. Tool definitions are collected inside handle_step.
                    let response = self.handle_step(0).await?;
                    let response = self.maybe_append_serendipity_to_message(response).await;

                    // Send the final output back to the channel.
                    self.channel.send(response.clone()).await?;

                    #[cfg(feature = "memory")]
                    {
                        let assistant_text = match &response.content {
                            Content::Text { text, .. } => Some(text.as_str()),
                            _ => None,
                        };
                        self.save_turn_memory_entries(&user_msg, assistant_text).await;
                        self.clear_turn_memory_context();
                        self.check_and_flush_context().await?;
                    }

                    self.clear_turn_knowledge_context();
                    self.clear_turn_emotion_context();
                }
            }
        }

        self.shutdown();
        Ok(())
    }

    /// Streaming main loop. Falls back to chat() if streaming fails.
    #[cfg(feature = "runtime")]
    pub async fn run_streaming<F>(&mut self, shutdown: F) -> Result<()>
    where
        F: std::future::Future<Output = ()> + Send,
    {
        tokio::pin!(shutdown);
        self.start_heartbeat_runtime()?;

        loop {
            let channel = self.channel.clone();
            tokio::select! {
                _ = &mut shutdown => { break; }
                heartbeat = async {
                    match self.heartbeat_receiver.as_mut() {
                        Some(receiver) => receiver.recv().await,
                        None => None,
                    }
                } => {
                    if heartbeat.is_some() {
                        self.handle_autonomy_tick().await?;
                    }
                }
                result = channel.receive() => {
                    let user_msg = result?;

                    // Intercept slash commands.
                    let slash_reply = if let Content::Text { text, .. } = &user_msg.content {
                        if let Some(handler) = &self.slash_handler.clone() {
                            handler(text, &mut self.provider, &mut self.mode_state)
                        } else {
                            None
                        }
                    } else {
                        None
                    };

                    if let Some(slash_result) = slash_reply {
                        match slash_result {
                            SlashCommandResult::Reply(reply) => {
                                let reply_msg = Message::text(Role::Assistant, &reply, None);
                                let _ = self.channel.send(reply_msg).await;
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
                                    result.task_list
                                        .iter()
                                        .map(|task| format!(
                                            "- {} | {} | {}h | P{}",
                                            task.name,
                                            task.assigned_role,
                                            task.estimated_hours,
                                            task.priority
                                        ))
                                        .collect::<Vec<_>>()
                                        .join("\n")
                                );
                                let reply_msg = Message::text(Role::Assistant, &final_reply, None);
                                let _ = self.channel.send(reply_msg.clone()).await;
                                self.push_message(user_msg.clone());
                                self.push_message(reply_msg.clone());
                                #[cfg(feature = "memory")]
                                self.save_turn_memory_entries(&user_msg, Some(&final_reply)).await;
                            }
                            SlashCommandResult::Dashboard => {
                                let reply = match &self.dashboard_handler {
                                    Some(handler) => match handler() {
                                        Ok(url) => format!("[Dashboard] {url} opened"),
                                        Err(error) => format!("❌ Dashboard failed: {error}"),
                                    },
                                    None => "❌ Dashboard handler is not configured.".to_string(),
                                };
                                let reply_msg = Message::text(Role::Assistant, &reply, None);
                                let _ = self.channel.send(reply_msg).await;
                            }
                            SlashCommandResult::Task { description } => {
                                let reply = self.handle_task_command(&description)?;
                                let reply_msg = Message::text(Role::Assistant, &reply, None);
                                let _ = self.channel.send(reply_msg).await;
                            }
                            SlashCommandResult::Skills => {
                                let reply = self.handle_skills_command()?;
                                let reply_msg = Message::text(Role::Assistant, &reply, None);
                                let _ = self.channel.send(reply_msg).await;
                            }
                            SlashCommandResult::Unresolved => {
                                let reply = self.handle_unresolved_command()?;
                                let reply_msg = Message::text(Role::Assistant, &reply, None);
                                let _ = self.channel.send(reply_msg).await;
                            }
                            SlashCommandResult::UpdateSystemPrompt { reply, system_prompt, reset_history } => {
                                self.apply_system_prompt_update(system_prompt, reset_history);
                                if !reply.trim().is_empty() {
                                    let reply_msg = Message::text(Role::Assistant, &reply, None);
                                    let _ = self.channel.send(reply_msg).await;
                                }
                            }
                        }
                        continue;
                    }

                    self.push_message(user_msg.clone());
                    self.begin_user_turn();
                    self.refresh_turn_role(&user_msg);
                    let pre_spinner = start_pre_spinner();
                    self.refresh_turn_emotion_context().await;
                    self.refresh_turn_knowledge_context(&user_msg).await;

                    #[cfg(feature = "memory")]
                    self.refresh_turn_memory_context(&user_msg).await;
                    pre_spinner.finish_and_clear();

                    let mut response_result = self.execute_streaming_turn_once().await;
                    let should_retry_with_emergency = response_result
                        .as_ref()
                        .err()
                        .map(|error| {
                            let err_str = error.to_string().to_lowercase();
                            err_str.contains("token")
                                || err_str.contains("limit")
                                || err_str.contains("exceeded")
                                || err_str.contains("context")
                        })
                        .unwrap_or(false);

                    if should_retry_with_emergency {
                        response_result = if let Err(error) = self.emergency_compress_context().await {
                            Err(error)
                        } else {
                            self.execute_streaming_turn_once().await
                        };
                    }

                    let final_assistant_text = match response_result {
                        Ok(text_opt) => text_opt,
                        Err(e) => {
                            let err_text = format!("⚠️ Error: {}", e);
                            eprintln!("[Engine Error] {}", err_text);
                            self.log_engine_error("run_streaming", &e.to_string());

                            // Send the error text back through the active channel.
                            let _ = self.channel.send(crate::types::Message::text(crate::types::Role::Assistant, err_text, None)).await;
                            None
                        }
                    };

                    #[cfg(feature = "memory")]
                    {
                        self.save_turn_memory_entries(&user_msg, final_assistant_text.as_deref()).await;
                        self.clear_turn_memory_context();
                        self.check_and_flush_context().await?;
                    }

                    self.clear_turn_knowledge_context();
                    self.clear_turn_emotion_context();
                }
            }
        }

        self.shutdown();
        Ok(())
    }

    #[cfg(feature = "runtime")]
    async fn execute_streaming_turn_once(&mut self) -> Result<Option<String>> {
        self.compress_context().await?;
        self.check_current_agent_budget()?;
        let request_token_count = count_messages_tokens(&self.request_messages(), &self.context_model);
        self.log_llm_call("stream", request_token_count);

        let mut ralf_state = RalfState::default();
        let streaming_result = ralf_execute(
            "llm_stream",
            &self.ralf_config,
            &mut ralf_state,
            self.audit_logger.as_deref(),
            || self.stream_step_with_tools(),
        )
        .await
        .unwrap_or(None);

        match streaming_result {
            Some(text) => {
                let streamed_text = text;
                let text = self
                    .maybe_append_serendipity_to_text(streamed_text.clone())
                    .await;
                let response_msg = crate::types::Message::text(
                    crate::types::Role::Assistant,
                    &text,
                    None,
                );
                self.record_current_agent_usage(count_message_tokens(&response_msg, &self.context_model))?;
                self.push_message(response_msg.clone());

                if self.channel.is_cli_source() {
                    if let Some(suffix) = text.strip_prefix(&streamed_text)
                        && !suffix.is_empty() {
                            print!("{suffix}");
                            std::io::Write::flush(&mut std::io::stdout()).ok();
                        }
                    let _ = tokio::task::spawn_blocking(|| {
                        use std::io::Write;
                        println!();
                        print!("> ");
                        std::io::stdout().flush().ok();
                    })
                    .await;
                } else {
                    self.channel.send(response_msg).await?;
                }

                Ok(Some(text))
            }
            None => {
                use indicatif::{ProgressBar, ProgressStyle};
                use std::time::Duration;

                let spinner = ProgressBar::new_spinner();
                spinner.set_style(
                    ProgressStyle::default_spinner()
                        .tick_strings(&["⠋","⠙","⠹","⠸","⠼","⠴","⠦","⠧","⠇","⠏","✓"])
                        .template("{spinner:.cyan} {msg}")
                        .unwrap()
                );
                spinner.set_message("Thinking...");
                spinner.enable_steady_tick(Duration::from_millis(80));

                let final_msg = self.handle_step(0).await?;
                let final_msg = self.maybe_append_serendipity_to_message(final_msg).await;
                spinner.finish_and_clear();
                self.channel.send(final_msg.clone()).await?;

                Ok(if let Content::Text { text, .. } = &final_msg.content {
                    Some(text.clone())
                } else {
                    None
                })
            }
        }
    }

    /// Streams tokens progressively, including tool definitions in the request.
    #[cfg(feature = "runtime")]
    async fn stream_step_with_tools(&self) -> Result<Option<String>> {
        use tokio_stream::StreamExt;
        use indicatif::{ProgressBar, ProgressStyle};
        use std::time::Duration;

        let tool_defs: Vec<ToolDefinition> = self.tools.values()
            .map(|t| t.definition())
            .collect();
        let tools = if tool_defs.is_empty() { None } else { Some(tool_defs.as_slice()) };

        // Start spinner.
        let spinner = ProgressBar::new_spinner();
        spinner.set_style(
            ProgressStyle::default_spinner()
                .tick_strings(&["⠋","⠙","⠹","⠸","⠼","⠴","⠦","⠧","⠇","⠏","✓"])
                .template("{spinner:.cyan} {msg}")
                .unwrap()
        );
        spinner.set_message("Thinking...");
        spinner.enable_steady_tick(Duration::from_millis(80));

        // Attempt streaming with tool definitions included.
        let request_messages = self.request_messages();
        let mut stream = match self.provider.stream(&request_messages, tools).await {
            Ok(s) => s,
            Err(_) => {
                spinner.finish_and_clear();
                return Ok(None); // Fallback when streaming is unsupported.
            }
        };

        let mut full_text = String::new();
        let mut first_token = true;

        while let Some(chunk) = stream.next().await {
            match chunk {
                Ok(token) => {
                    // Ignore empty tokens.
                    if token.is_empty() { continue; }

                    // Stop streaming and fall back if the first chunk looks like a tool call payload.
                    if first_token && (token.trim_start().starts_with("{\"") || token.contains("tool_call")) {
                        spinner.finish_and_clear();
                        return Ok(None);
                    }
                    
                    if first_token {
                        if self.channel.is_cli_source() {
                            spinner.finish_and_clear(); // CLI starts printing immediately, so remove spinner.
                        }
                        self.channel.cancel_typing().await; // Stop typing indicators on channels like Telegram.
                        first_token = false;
                    }

                    // Print tokens immediately for CLI only.
                    if self.channel.is_cli_source() {
                        print!("{}", token);
                        std::io::Write::flush(&mut std::io::stdout()).ok();
                    }
                    full_text.push_str(&token);
                }
                Err(_) => break,
            }
        }

        if full_text.is_empty() {
            spinner.finish_and_clear();
            Ok(None)
        } else {
            spinner.finish_and_clear(); // Final cleanup in case the spinner is still visible.
            if self.channel.is_cli_source() {
                println!(); // Newline after streaming completes.
            }
            Ok(Some(full_text))
        }
    }
}

#[cfg(feature = "runtime")]
fn start_pre_spinner() -> indicatif::ProgressBar {
    use indicatif::{ProgressBar, ProgressStyle};
    use std::time::Duration;

    let spinner = ProgressBar::new_spinner();
    spinner.set_style(
        ProgressStyle::default_spinner()
            .tick_strings(&["⠋","⠙","⠹","⠸","⠼","⠴","⠦","⠧","⠇","⠏","✓"])
            .template("{spinner:.cyan} {msg}")
            .unwrap()
    );
    spinner.set_message("Thinking...");
    spinner.enable_steady_tick(Duration::from_millis(80));
    spinner
}

