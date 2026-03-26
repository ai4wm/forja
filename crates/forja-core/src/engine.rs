use crate::error::{ForjaError, Result};
use crate::emotion::EmotionEngine;
use crate::knowledge::KnowledgeManager;
use crate::mode::ModeState;
use crate::prompt::assemble_system_prompt;
use crate::serendipity::SerendipityEngine;
use crate::traits::{Channel, LlmProvider, Tool};
use crate::types::{Content, Message, Role, ToolDefinition};
use chrono::{DateTime, Local};
use std::collections::HashMap;
use std::sync::Arc;
mod emotion;
mod knowledge;
mod mode;
mod serendipity;
#[cfg(feature = "memory")]
mod memory;

#[cfg(feature = "memory")]
use crate::traits::MemoryStore;

const MAX_TOOL_DEPTH: usize = 10;

pub enum SlashCommandResult {
    Reply(String),
    ReplyAndSave { user_text: String, reply: String },
    UpdateSystemPrompt {
        reply: String,
        system_prompt: Option<String>,
        reset_history: bool,
    },
}

/// Slash command callback type for /models and /model.
pub type SlashHandler = Arc<dyn Fn(&str, &mut Arc<dyn LlmProvider>, &mut ModeState) -> Option<SlashCommandResult> + Send + Sync>;

/// Core Forja engine. Coordinates channels, LLM providers, and tools,
/// and drives the main event loop plus recursive tool evaluation.
pub struct Engine {
    provider: Arc<dyn LlmProvider>,
    #[cfg_attr(not(feature = "runtime"), allow(dead_code))]
    channel: Arc<dyn Channel>,
    tools: HashMap<String, Arc<dyn Tool>>,
    conversation_history: Vec<Message>,
    max_history: usize,
    system_prompt: Option<String>,
    tool_prompt: Option<String>,
    assistant_name: String,
    user_title: String,
    slash_handler: Option<SlashHandler>,
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
        Self {
            provider,
            channel,
            tools: HashMap::new(),
            conversation_history: Vec::new(),
            max_history: 100,
            system_prompt: None,
            tool_prompt: None,
            assistant_name: "Forja".to_string(),
            user_title: "User".to_string(),
            slash_handler: None,
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

    /// Adds a message to conversation history and compacts it when it exceeds the window.
    fn push_message(&mut self, msg: Message) {
        self.conversation_history.push(msg);
        while self.conversation_history.len() > self.max_history {
            if let Some(pos) = self.conversation_history.iter().position(|m| m.role != Role::System) {
                self.conversation_history.remove(pos);
            } else {
                break;
            }
        }
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
            self.conversation_history.clear();
        }
        if let Some(system_prompt) = next_system_prompt {
            self.system_prompt = Some(system_prompt);
        }
    }

    /// Evaluates a single turn. Tool calls are executed and then re-interpreted recursively.
    #[async_recursion::async_recursion]
    pub async fn handle_step(&mut self, depth: usize) -> Result<Message> {
        if depth >= MAX_TOOL_DEPTH {
            return Err(ForjaError::MaxDepthExceeded(MAX_TOOL_DEPTH));
        }

        // Collect tool definitions from all registered tools.
        let tool_defs: Vec<ToolDefinition> = self.tools.values()
            .map(|t| t.definition())
            .collect();
        let tools = if tool_defs.is_empty() { None } else { Some(tool_defs.as_slice()) };

        let request_messages = self.request_messages();
        let response_msg = self.provider.chat(&request_messages, tools).await?;

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

                let result = if let Some(tool) = self.tools.get(tool_name) {
                    tool.execute(arguments.clone()).await?
                } else {
                    serde_json::json!({
                        "error": format!("Unknown tool requested: {}", tool_name)
                    })
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

        loop {
            tokio::select! {
                // Exit on shutdown signal.
                _ = &mut shutdown => {
                    break;
                }
                // Wait for incoming messages from the channel.
                result = self.channel.receive() => {
                    let user_msg = result?;
                    
                    #[cfg(feature = "memory")]
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

        Ok(())
    }

    /// Streaming main loop. Falls back to chat() if streaming fails.
    #[cfg(feature = "runtime")]
    pub async fn run_streaming<F>(&mut self, shutdown: F) -> Result<()>
    where
        F: std::future::Future<Output = ()> + Send,
    {
        tokio::pin!(shutdown);

        loop {
            tokio::select! {
                _ = &mut shutdown => { break; }
                result = self.channel.receive() => {
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
                            SlashCommandResult::UpdateSystemPrompt { reply, system_prompt, reset_history } => {
                                self.apply_system_prompt_update(system_prompt, reset_history);
                                let reply_msg = Message::text(Role::Assistant, &reply, None);
                                let _ = self.channel.send(reply_msg).await;
                            }
                        }
                        continue;
                    }

                    #[cfg(feature = "memory")]
                    self.push_message(user_msg.clone());
                    self.begin_user_turn();
                    self.refresh_turn_role(&user_msg);
                    let pre_spinner = start_pre_spinner();
                    self.refresh_turn_emotion_context().await;
                    self.refresh_turn_knowledge_context(&user_msg).await;

                    #[cfg(feature = "memory")]
                    self.refresh_turn_memory_context(&user_msg).await;
                    pre_spinner.finish_and_clear();

                    // Catch errors across streaming and fallback handling.
                    let response_result = async {
                        // Try a streaming LLM call first.
                        let streaming_result = self.stream_step_with_tools().await
                            .unwrap_or(None);

                        match streaming_result {
                            Some(text) => {
                                let streamed_text = text;
                                let text = self
                                    .maybe_append_serendipity_to_text(streamed_text.clone())
                                    .await;
                                // Streaming text path succeeded.
                                let response_msg = crate::types::Message::text(
                                    crate::types::Role::Assistant, &text, None
                                );
                                self.push_message(response_msg.clone());
                                
                                if self.channel.is_cli_source() {
                                    if let Some(suffix) = text.strip_prefix(&streamed_text)
                                        && !suffix.is_empty() {
                                            print!("{suffix}");
                                            std::io::Write::flush(&mut std::io::stdout()).ok();
                                        }
                                    // CLI already showed streamed text, restore the prompt only.
                                    let _ = tokio::task::spawn_blocking(|| {
                                        use std::io::Write;
                                        println!();
                                        print!("> ");
                                        std::io::stdout().flush().ok();
                                    }).await;
                                } else {
                                    // Non-CLI channels receive the final message through send().
                                    self.channel.send(response_msg).await?;
                                }
                                
                                Ok::<Option<String>, crate::error::ForjaError>(Some(text))
                            }
                            None => {
                                use indicatif::{ProgressBar, ProgressStyle};
                                use std::time::Duration;

                                // Start spinner when streaming is unavailable, e.g. tool-call path.
                                let spinner = ProgressBar::new_spinner();
                                spinner.set_style(
                                    ProgressStyle::default_spinner()
                                        .tick_strings(&["⠋","⠙","⠹","⠸","⠼","⠴","⠦","⠧","⠇","⠏","✓"])
                                        .template("{spinner:.cyan} {msg}")
                                        .unwrap()
                                );
                                spinner.set_message("Thinking...");
                                spinner.enable_steady_tick(Duration::from_millis(80));

                                // Fallback to the non-streaming chat path.
                                let final_msg = self.handle_step(0).await?;
                                let final_msg = self.maybe_append_serendipity_to_message(final_msg).await;
                                
                                // Stop spinner after the response arrives.
                                spinner.finish_and_clear();

                                self.channel.send(final_msg.clone()).await?;
                                
                                Ok::<Option<String>, crate::error::ForjaError>(
                                    if let Content::Text { text, .. } = &final_msg.content {
                                        Some(text.clone())
                                    } else {
                                        None
                                    }
                                )
                            }
                        }
                    }.await;

                    let final_assistant_text = match response_result {
                        Ok(text_opt) => text_opt,
                        Err(e) => {
                            let err_text = format!("⚠️ Error: {}", e);
                            eprintln!("[Engine Error] {}", err_text);
                            
                            // Reset history if the error looks like a token/context overflow.
                            let err_str = e.to_string().to_lowercase();
                            if err_str.contains("token") || err_str.contains("limit") || err_str.contains("exceeded") || err_str.contains("context") {
                                self.conversation_history.clear();
                            }
                            
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

        Ok(())
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
            .tick_strings(&["","","","","","","","","","",""])
            .template("{spinner:.cyan} {msg}")
            .unwrap()
    );
    spinner.set_message("Processing...");
    spinner.enable_steady_tick(Duration::from_millis(80));
    spinner
}

