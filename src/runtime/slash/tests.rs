use super::{build_slash_handler, SlashHandlerDeps};
use crate::bootstrap::BootstrapPaths;
use crate::config::ForjaConfig;
use crate::provider_registry::ProviderRegistry;
use crate::runtime::mock::MockLlmProvider;
use async_trait::async_trait;
use forja_core::error::{ForjaError, Result};
use forja_core::mode::{ExecMode, ModeState, Role, ThinkLevel};
use forja_core::skill::SkillRegistry;
use forja_core::traits::{Channel, LlmProvider, NotificationLevel, NotificationState, VoiceChannelStatus};
use forja_core::Message;
use forja_tools::{MockCaptureBackend, MockVisionAnalyzer};
use std::sync::{Arc, Mutex};

struct DummyChannel {
    notification_state: Mutex<NotificationState>,
    voice_status: Mutex<VoiceChannelStatus>,
}

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

    fn supports_voice(&self) -> bool {
        true
    }

    fn notification_state(&self) -> Option<NotificationState> {
        Some(*self.notification_state.lock().unwrap())
    }

    async fn set_notifications_enabled(&self, enabled: bool) -> Result<NotificationState> {
        let mut state = self.notification_state.lock().unwrap();
        state.enabled = enabled;
        Ok(*state)
    }

    fn voice_status(&self) -> Option<VoiceChannelStatus> {
        Some(*self.voice_status.lock().unwrap())
    }

    async fn set_voice_enabled(&self, enabled: bool) -> Result<VoiceChannelStatus> {
        let status = if enabled {
            VoiceChannelStatus::Listening
        } else {
            VoiceChannelStatus::Disabled
        };
        *self.voice_status.lock().unwrap() = status;
        Ok(status)
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
    let skill_registry = empty_skill_registry("default");
    let handler = build_slash_handler(SlashHandlerDeps {
        cfg_for_handler: cfg.clone(),
        registry: ProviderRegistry::from_config(&cfg),
        channel: Arc::new(DummyChannel {
            notification_state: Mutex::new(NotificationState {
                enabled: true,
                min_level: NotificationLevel::Info,
                ..NotificationState::default()
            }),
            voice_status: Mutex::new(VoiceChannelStatus::Disabled),
        }),
        bootstrap_paths: BootstrapPaths::from_home(std::env::temp_dir()),
        interactive_identity_supported: true,
        exec_mode_handle: exec_mode_handle.clone(),
        vision_enabled: false,
        capture_backend: Arc::new(MockCaptureBackend::new()),
        vision_analyzer: Arc::new(MockVisionAnalyzer::new()),
        state_change_confirmer: Some(Arc::new(|_| true)),
        skill_registry,
    });

    (
        handler,
        exec_mode_handle,
        Arc::new(MockLlmProvider),
        ModeState::new(ExecMode::Auto, ThinkLevel::Mid, Role::Auto),
    )
}

fn empty_skill_registry(name: &str) -> Arc<SkillRegistry> {
    Arc::new(
        SkillRegistry::new(
            &std::env::temp_dir().join(format!("forja_slash_skill_registry_{name}.db")),
            &[],
        )
        .unwrap(),
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

#[test]
fn slash_handler_routes_task_add_as_task_command() {
    let (handler, _, mut provider, mut mode_state) = test_handler();
    let result = handler("/task add SPEC-RUNTIME-001", &mut provider, &mut mode_state);

    assert!(matches!(
        result,
        Some(forja_core::engine::SlashCommandResult::Task { description })
            if description == "add SPEC-RUNTIME-001"
    ));
}

#[test]
fn slash_handler_routes_task_list_to_reply_until_engine_supports_it() {
    let (handler, _, mut provider, mut mode_state) = test_handler();
    let result = handler("/task list", &mut provider, &mut mode_state);

    assert!(matches!(
        result,
        Some(forja_core::engine::SlashCommandResult::Task { description })
            if description == "list"
    ));
}

#[test]
fn slash_handler_routes_autonomy_status_to_reply_until_engine_supports_it() {
    let (handler, _, mut provider, mut mode_state) = test_handler();
    let result = handler("/autonomy status", &mut provider, &mut mode_state);

    assert!(matches!(
        result,
        Some(forja_core::engine::SlashCommandResult::AutonomyCommand { command })
            if command == "status"
    ));
}

#[test]
fn slash_handler_routes_dream_command() {
    let (handler, _, mut provider, mut mode_state) = test_handler();
    let result = handler("/dream", &mut provider, &mut mode_state);

    assert!(matches!(
        result,
        Some(forja_core::engine::SlashCommandResult::Dream)
    ));
}

#[test]
fn slash_handler_rejects_invalid_hugging_face_model_spec() {
    let (handler, _, mut provider, mut mode_state) = test_handler();
    let result = handler("/model fetch invalid-repo", &mut provider, &mut mode_state);

    assert!(matches!(
        result,
        Some(forja_core::engine::SlashCommandResult::Reply(reply))
            if reply.contains("owner/repo")
    ));
}

#[test]
fn slash_handler_reports_current_model() {
    let (handler, _, mut provider, mut mode_state) = test_handler();
    let result = handler("/model", &mut provider, &mut mode_state);

    assert!(matches!(
        result,
        Some(forja_core::engine::SlashCommandResult::Reply(reply))
            if reply.contains("Current model:")
    ));
}

#[test]
fn slash_handler_maps_natural_language_mode_change_with_confirmation() {
    let (handler, exec_mode_handle, mut provider, mut mode_state) = test_handler();
    let result = handler("자동 모드로 바꿔줘", &mut provider, &mut mode_state);

    assert!(matches!(
        result,
        Some(forja_core::engine::SlashCommandResult::Reply(reply))
            if reply == "Mode updated: auto"
    ));
    assert_eq!(mode_state.exec_mode, ExecMode::Auto);
    assert_eq!(*exec_mode_handle.lock().unwrap(), ExecMode::Auto);
}

#[test]
fn slash_handler_cancels_natural_language_change_when_confirmation_fails() {
    let cfg = ForjaConfig::default();
    let exec_mode_handle = Arc::new(Mutex::new(ExecMode::Auto));
    let handler = build_slash_handler(SlashHandlerDeps {
        cfg_for_handler: cfg.clone(),
        registry: ProviderRegistry::from_config(&cfg),
        channel: Arc::new(DummyChannel {
            notification_state: Mutex::new(NotificationState {
                enabled: true,
                min_level: NotificationLevel::Info,
                ..NotificationState::default()
            }),
            voice_status: Mutex::new(VoiceChannelStatus::Disabled),
        }),
        bootstrap_paths: BootstrapPaths::from_home(std::env::temp_dir()),
        interactive_identity_supported: true,
        exec_mode_handle: exec_mode_handle.clone(),
        vision_enabled: false,
        capture_backend: Arc::new(MockCaptureBackend::new()),
        vision_analyzer: Arc::new(MockVisionAnalyzer::new()),
        state_change_confirmer: Some(Arc::new(|_| false)),
        skill_registry: empty_skill_registry("cancel"),
    });
    let mut provider: Arc<dyn LlmProvider> = Arc::new(MockLlmProvider);
    let mut mode_state = ModeState::new(ExecMode::Auto, ThinkLevel::Mid, Role::Auto);

    let result = handler("코더 역할로 해줘", &mut provider, &mut mode_state);

    assert!(matches!(
        result,
        Some(forja_core::engine::SlashCommandResult::Reply(reply))
            if reply == "Canceled."
    ));
    assert_eq!(mode_state.role, Role::Auto);
}

#[test]
fn slash_handler_routes_explicit_skill_command() {
    let (handler, _, mut provider, mut mode_state) = test_handler();
    let result = handler("/skill demo", &mut provider, &mut mode_state);

    assert!(matches!(
        result,
        Some(forja_core::engine::SlashCommandResult::Skill { name })
            if name == "demo"
    ));
}

#[test]
fn slash_handler_routes_natural_language_skill_trigger() {
    let root = std::env::temp_dir().join("forja_slash_trigger_skill");
    let db = std::env::temp_dir().join("forja_slash_trigger_skill.db");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(
        root.join("SKILL.md"),
        "---\nname: Deploy Skill\ntrigger: deploy checklist\ndescription: Run deploy steps\n---\n\n```sh\necho deploy\n```",
    )
    .unwrap();
    let cfg = ForjaConfig::default();
    let exec_mode_handle = Arc::new(Mutex::new(ExecMode::Auto));
    let handler = build_slash_handler(SlashHandlerDeps {
        cfg_for_handler: cfg.clone(),
        registry: ProviderRegistry::from_config(&cfg),
        channel: Arc::new(DummyChannel {
            notification_state: Mutex::new(NotificationState {
                enabled: true,
                min_level: NotificationLevel::Info,
                ..NotificationState::default()
            }),
            voice_status: Mutex::new(VoiceChannelStatus::Disabled),
        }),
        bootstrap_paths: BootstrapPaths::from_home(std::env::temp_dir()),
        interactive_identity_supported: true,
        exec_mode_handle,
        vision_enabled: false,
        capture_backend: Arc::new(MockCaptureBackend::new()),
        vision_analyzer: Arc::new(MockVisionAnalyzer::new()),
        state_change_confirmer: Some(Arc::new(|_| true)),
        skill_registry: Arc::new(SkillRegistry::new(&db, &[root.clone()]).unwrap()),
    });
    let mut provider: Arc<dyn LlmProvider> = Arc::new(MockLlmProvider);
    let mut mode_state = ModeState::new(ExecMode::Auto, ThinkLevel::Mid, Role::Auto);

    let result = handler("please run the deploy checklist", &mut provider, &mut mode_state);

    assert!(matches!(
        result,
        Some(forja_core::engine::SlashCommandResult::Skill { name })
            if name == "Deploy Skill"
    ));

    let _ = std::fs::remove_dir_all(root);
    let _ = std::fs::remove_file(db);
}

#[tokio::test(flavor = "multi_thread")]
async fn slash_handler_toggles_voice_channel_on() {
    let (handler, _, mut provider, mut mode_state) = test_handler();
    let result = handler("/voice on", &mut provider, &mut mode_state);

    assert!(matches!(
        result,
        Some(forja_core::engine::SlashCommandResult::Reply(reply))
            if reply == "Voice channel enabled and listening."
    ));
}

#[test]
fn slash_handler_reports_voice_status() {
    let (handler, _, mut provider, mut mode_state) = test_handler();
    let result = handler("/voice status", &mut provider, &mut mode_state);

    assert!(matches!(
        result,
        Some(forja_core::engine::SlashCommandResult::Reply(reply))
            if reply == "Voice status: disabled"
    ));
}

#[test]
fn slash_handler_reports_notification_status() {
    let (handler, _, mut provider, mut mode_state) = test_handler();
    let result = handler("/notify status", &mut provider, &mut mode_state);

    assert!(matches!(
        result,
        Some(forja_core::engine::SlashCommandResult::Reply(reply))
            if reply == "Notifications: on (min level: info)"
    ));
}

#[tokio::test(flavor = "multi_thread")]
async fn slash_handler_toggles_notifications_off() {
    let (handler, _, mut provider, mut mode_state) = test_handler();
    let result = handler("/notify off", &mut provider, &mut mode_state);

    assert!(matches!(
        result,
        Some(forja_core::engine::SlashCommandResult::Reply(reply))
            if reply == "Notifications disabled."
    ));
}

#[tokio::test(flavor = "multi_thread")]
async fn slash_handler_toggles_notifications_on() {
    let (handler, _, mut provider, mut mode_state) = test_handler();
    let _ = handler("/notify off", &mut provider, &mut mode_state);
    let result = handler("/notify on", &mut provider, &mut mode_state);

    assert!(matches!(
        result,
        Some(forja_core::engine::SlashCommandResult::Reply(reply))
            if reply == "Notifications enabled (min level: info)."
    ));
}

#[test]
fn slash_handler_routes_tui_command() {
    let (handler, _, mut provider, mut mode_state) = test_handler();
    let result = handler("/tui", &mut provider, &mut mode_state);

    assert!(matches!(
        result,
        Some(forja_core::engine::SlashCommandResult::Tui)
    ));
}
