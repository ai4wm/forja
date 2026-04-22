use crate::audit::logger::AuditLogger;
use crate::autonomy::AutonomyExecutionRuntime;
use crate::autonomy::loop_runner::AutonomousLoop;
use crate::budget::{BudgetMode, manager::BudgetManager};
use crate::context::SummaryCallback;
use crate::creation::DebateEngine;
use crate::emotion::EmotionEngine;
use crate::error::Result;
use crate::gateway::Envelope;
use crate::heartbeat::scheduler::HeartbeatScheduler;
use crate::knowledge::KnowledgeManager;
use crate::mode::ModeState;
use crate::ralf::RalfConfig;
use crate::serendipity::SerendipityEngine;
use crate::skill::SkillRegistry;
use crate::traits::{Channel, LlmProvider, Tool};
use crate::types::Message;
use chrono::{DateTime, Local};
use std::collections::HashMap;
use std::sync::Arc;

mod audit;
mod autonomy;
mod budget;
mod context;
mod creation;
mod dashboard;
#[cfg(feature = "memory")]
mod dream;
mod emotion;
mod heartbeat;
mod knowledge;
#[cfg(feature = "memory")]
mod memory;
mod mode;
mod request;
mod serendipity;
mod skills;
mod slash_runtime;
mod state;
mod streaming;
mod tool_execution;
mod turn;

#[cfg(feature = "memory")]
pub use self::dream::DreamRuntimeConfig;
#[cfg(feature = "memory")]
use crate::traits::MemoryStore;

pub(super) const MAX_TOOL_DEPTH: usize = 10;
pub(super) const ANSI_RESET: &str = "\x1b[0m";
#[cfg_attr(not(feature = "runtime"), allow(dead_code))]
pub(super) const ANSI_CYAN: &str = "\x1b[36m";
#[cfg_attr(not(feature = "runtime"), allow(dead_code))]
pub(super) const ANSI_YELLOW: &str = "\x1b[33m";
#[cfg_attr(not(feature = "runtime"), allow(dead_code))]
pub(super) const ANSI_MAGENTA: &str = "\x1b[35m";
#[cfg_attr(not(feature = "runtime"), allow(dead_code))]
pub(super) const ANSI_GREEN: &str = "\x1b[32m";
#[cfg_attr(not(feature = "runtime"), allow(dead_code))]
pub(super) const ANSI_BLUE: &str = "\x1b[34m";

pub enum SlashCommandResult {
    Reply(String),
    ReplyAndSave {
        user_text: String,
        reply: String,
    },
    Debate {
        topic: String,
    },
    Dashboard,
    Tui,
    #[cfg(feature = "memory")]
    Dream,
    Skill {
        name: String,
    },
    Task {
        description: String,
    },
    AutonomyCommand {
        command: String,
    },
    Skills,
    Unresolved,
    UpdateSystemPrompt {
        reply: String,
        system_prompt: Option<String>,
        reset_history: bool,
    },
}

pub type SlashHandler = Arc<
    dyn Fn(&str, &mut Arc<dyn LlmProvider>, &mut ModeState) -> Option<SlashCommandResult>
        + Send
        + Sync,
>;
pub type DashboardHandler = Arc<dyn Fn() -> Result<String> + Send + Sync>;
pub type TuiHandler = Arc<dyn Fn() -> Result<String> + Send + Sync>;

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
    autonomy_runtime: Option<AutonomyExecutionRuntime>,
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
    tui_handler: Option<TuiHandler>,
    skill_registry: Option<Arc<SkillRegistry>>,
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
    #[cfg(feature = "memory")]
    dream_runtime: Option<DreamRuntimeConfig>,
    #[cfg(feature = "memory")]
    dream_state: Option<Arc<dream::DreamRuntimeState>>,
}

impl Engine {
    #[cfg(feature = "runtime")]
    pub async fn run<F>(&mut self, shutdown: F) -> Result<()>
    where
        F: std::future::Future<Output = ()> + Send,
    {
        #[cfg(feature = "memory")]
        {
            tokio::pin!(shutdown);
            self.start_heartbeat_runtime()?;
            let mut dream_interval = self.dream_interval();

            loop {
                let channel = self.channel.clone();
                tokio::select! {
                    _ = &mut shutdown => break,
                    heartbeat = self.next_heartbeat() => {
                        if heartbeat.is_some() {
                            self.handle_autonomy_tick().await?;
                        }
                    }
                    _ = Self::wait_for_dream_tick(&mut dream_interval) => {
                        self.maybe_start_idle_dream();
                    }
                    result = channel.receive() => {
                        let user_msg = result?;
                        self.note_user_activity();
                        self.process_non_streaming_turn(user_msg).await?;
                    }
                }
            }

            self.finish_runtime_shutdown().await;
            Ok(())
        }

        #[cfg(not(feature = "memory"))]
        {
            tokio::pin!(shutdown);
            self.start_heartbeat_runtime()?;

            loop {
                let channel = self.channel.clone();
                tokio::select! {
                    _ = &mut shutdown => break,
                    heartbeat = self.next_heartbeat() => {
                        if heartbeat.is_some() {
                            self.handle_autonomy_tick().await?;
                        }
                    }
                    result = channel.receive() => {
                        let user_msg = result?;
                        self.process_non_streaming_turn(user_msg).await?;
                    }
                }
            }

            self.finish_runtime_shutdown().await;
            Ok(())
        }
    }

    #[cfg(feature = "runtime")]
    pub async fn run_streaming<F>(&mut self, shutdown: F) -> Result<()>
    where
        F: std::future::Future<Output = ()> + Send,
    {
        #[cfg(feature = "memory")]
        {
            tokio::pin!(shutdown);
            self.start_heartbeat_runtime()?;
            let mut dream_interval = self.dream_interval();

            loop {
                let channel = self.channel.clone();
                tokio::select! {
                    _ = &mut shutdown => break,
                    heartbeat = self.next_heartbeat() => {
                        if heartbeat.is_some() {
                            self.handle_autonomy_tick().await?;
                        }
                    }
                    _ = Self::wait_for_dream_tick(&mut dream_interval) => {
                        self.maybe_start_idle_dream();
                    }
                    result = channel.receive() => {
                        let user_msg = result?;
                        self.note_user_activity();
                        self.process_streaming_turn(user_msg).await?;
                    }
                }
            }

            self.finish_runtime_shutdown().await;
            Ok(())
        }

        #[cfg(not(feature = "memory"))]
        {
            tokio::pin!(shutdown);
            self.start_heartbeat_runtime()?;

            loop {
                let channel = self.channel.clone();
                tokio::select! {
                    _ = &mut shutdown => break,
                    heartbeat = self.next_heartbeat() => {
                        if heartbeat.is_some() {
                            self.handle_autonomy_tick().await?;
                        }
                    }
                    result = channel.receive() => {
                        let user_msg = result?;
                        self.process_streaming_turn(user_msg).await?;
                    }
                }
            }

            self.finish_runtime_shutdown().await;
            Ok(())
        }
    }

    #[cfg(feature = "runtime")]
    async fn next_heartbeat(&mut self) -> Option<Envelope> {
        match self.heartbeat_receiver.as_mut() {
            Some(receiver) => receiver.recv().await,
            None => None,
        }
    }

    #[cfg(feature = "runtime")]
    async fn finish_runtime_shutdown(&mut self) {
        #[cfg(feature = "memory")]
        self.run_shutdown_dream_if_due().await;
        #[cfg(feature = "memory")]
        self.flush_memory_store().await;
        self.shutdown();
    }
}
