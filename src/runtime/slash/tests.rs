use super::{build_slash_handler, SlashHandlerDeps};
use crate::bootstrap::BootstrapPaths;
use crate::config::ForjaConfig;
use crate::provider_registry::ProviderRegistry;
use crate::runtime::mock::MockLlmProvider;
use async_trait::async_trait;
use forja_core::error::{ForjaError, Result};
use forja_core::mode::{ExecMode, ModeState, Role, ThinkLevel};
use forja_core::traits::{Channel, LlmProvider};
use forja_core::Message;
use forja_tools::{MockCaptureBackend, MockVisionAnalyzer};
use std::sync::{Arc, Mutex};

struct DummyChannel;

#[async_trait]
impl Channel for DummyChannel {
    async fn receive(&self) -> Result<Message> {
        Err(ForjaError::ChannelError("not used in slash handler tests".to_string()))
    }

    async fn send(&self, _message: Message) -> Result<()> {
        Ok(())
    }

    fn is_cli_source(&self) -> bool {
        true
    }
}

fn test_handler() -> (
    forja_core::engine::SlashHandler,
    Arc<Mutex<ExecMode>>,
    Arc<dyn LlmProvider>,
    ModeState,
) {
    let cfg = ForjaConfig::default();
    let exec_mode_handle = Arc::new(Mutex::new(ExecMode::Auto));
    let handler = build_slash_handler(SlashHandlerDeps {
        cfg_for_handler: cfg.clone(),
        registry: ProviderRegistry::from_config(&cfg),
        channel: Arc::new(DummyChannel),
        bootstrap_paths: BootstrapPaths::from_home(std::env::temp_dir()),
        interactive_identity_supported: true,
        exec_mode_handle: exec_mode_handle.clone(),
        vision_enabled: false,
        capture_backend: Arc::new(MockCaptureBackend::new()),
        vision_analyzer: Arc::new(MockVisionAnalyzer::new()),
    });

    (
        handler,
        exec_mode_handle,
        Arc::new(MockLlmProvider),
        ModeState::new(ExecMode::Auto, ThinkLevel::Mid, Role::Auto),
    )
}

#[test]
fn slash_handler_returns_dashboard_command() {
    let (handler, _, mut provider, mut mode_state) = test_handler();
    let result = handler("/dashboard", &mut provider, &mut mode_state);

    assert!(matches!(
        result,
        Some(forja_core::engine::SlashCommandResult::Dashboard)
    ));
}

#[test]
fn slash_handler_returns_task_command() {
    let (handler, _, mut provider, mut mode_state) = test_handler();
    let result = handler("/task refactor runtime", &mut provider, &mut mode_state);

    assert!(matches!(
        result,
        Some(forja_core::engine::SlashCommandResult::Task { description })
            if description == "refactor runtime"
    ));
}

#[test]
fn slash_handler_updates_mode_state_and_shared_exec_mode() {
    let (handler, exec_mode_handle, mut provider, mut mode_state) = test_handler();
    let result = handler("/mode trust", &mut provider, &mut mode_state);

    assert!(matches!(
        result,
        Some(forja_core::engine::SlashCommandResult::Reply(reply))
            if reply == "Mode updated: trust"
    ));
    assert_eq!(mode_state.exec_mode, ExecMode::Trust);
    assert_eq!(*exec_mode_handle.lock().unwrap(), ExecMode::Trust);
}
