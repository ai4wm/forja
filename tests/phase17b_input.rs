use async_trait::async_trait;
use forja_core::traits::Tool;
use forja_tools::confirm::ConfirmationHandler;
use forja_tools::input::MockBackend;
use forja_tools::InputTool;
use serde_json::json;
use std::sync::Arc;

struct AllowConfirmation;

#[async_trait]
impl ConfirmationHandler for AllowConfirmation {
    async fn confirm(&self, _cmd: &str, _dangerous: bool) -> bool {
        true
    }
}

struct DenyConfirmation;

#[async_trait]
impl ConfirmationHandler for DenyConfirmation {
    async fn confirm(&self, _cmd: &str, _dangerous: bool) -> bool {
        false
    }
}

#[tokio::test]
async fn type_text_parses_valid_text_field() {
    let backend = Arc::new(MockBackend::new());
    let tool = InputTool::with_backend_and_settings(
        backend.clone(),
        Arc::new(AllowConfirmation),
        false,
    );

    let result = tool
        .execute(json!({
            "action": "type_text",
            "text": "안녕하세요 hello"
        }))
        .await
        .unwrap();

    assert_eq!(result["status"], json!("ok"));
    assert_eq!(result["action"], json!("type_text"));
    assert_eq!(backend.events_snapshot(), vec!["type_text:안녕하세요 hello"]);
}

#[tokio::test]
async fn key_press_maps_common_named_keys() {
    let cases = [
        ("Enter", "key_press:enter"),
        ("Tab", "key_press:tab"),
        ("Escape", "key_press:escape"),
        ("F1", "key_press:f1"),
        ("Left", "key_press:left"),
    ];

    for (key, expected) in cases {
        let backend = Arc::new(MockBackend::new());
        let tool = InputTool::with_backend_and_settings(
            backend.clone(),
            Arc::new(AllowConfirmation),
            false,
        );

        let result = tool
            .execute(json!({
                "action": "key_press",
                "key": key
            }))
            .await
            .unwrap();

        assert_eq!(result["status"], json!("ok"), "{key}");
        assert_eq!(backend.events_snapshot(), vec![expected.to_string()], "{key}");
    }
}

#[tokio::test]
async fn hotkey_parsing_records_normalized_combo() {
    let backend = Arc::new(MockBackend::new());
    let tool = InputTool::with_backend_and_settings(
        backend.clone(),
        Arc::new(AllowConfirmation),
        false,
    );

    let result = tool
        .execute(json!({
            "action": "hotkey",
            "keys": ["ctrl", "c"]
        }))
        .await
        .unwrap();

    assert_eq!(result["status"], json!("ok"));
    assert_eq!(result["action"], json!("hotkey"));
    assert_eq!(backend.events_snapshot(), vec!["hotkey:ctrl+c"]);
}

#[tokio::test]
async fn dangerous_hotkeys_are_blocked_without_confirmation() {
    let cases = [
        vec!["alt", "f4"],
        vec!["ctrl", "alt", "delete"],
        vec!["win", "l"],
    ];

    for keys in cases {
        let backend = Arc::new(MockBackend::new());
        let tool = InputTool::with_backend_and_settings(
            backend.clone(),
            Arc::new(DenyConfirmation),
            false,
        );

        let result = tool
            .execute(json!({
                "action": "hotkey",
                "keys": keys
            }))
            .await
            .unwrap();

        assert_eq!(result["status"], json!("blocked"));
        assert_eq!(result["action"], json!("hotkey"));
        assert!(backend.events_snapshot().is_empty());
    }
}

#[tokio::test]
async fn unsafe_mode_bypasses_dangerous_hotkey_safety() {
    let backend = Arc::new(MockBackend::new());
    let tool = InputTool::with_backend_and_settings(
        backend.clone(),
        Arc::new(DenyConfirmation),
        true,
    );

    let result = tool
        .execute(json!({
            "action": "hotkey",
            "keys": ["alt", "f4"]
        }))
        .await
        .unwrap();

    assert_eq!(result["status"], json!("ok"));
    assert_eq!(backend.events_snapshot(), vec!["hotkey:alt+f4"]);
}

#[tokio::test]
async fn mouse_click_parses_button_and_coordinates() {
    let backend = Arc::new(MockBackend::new());
    let tool = InputTool::with_backend_and_settings(
        backend.clone(),
        Arc::new(AllowConfirmation),
        false,
    );

    let result = tool
        .execute(json!({
            "action": "mouse_click",
            "button": "left",
            "x": 500,
            "y": 300
        }))
        .await
        .unwrap();

    assert_eq!(result["status"], json!("ok"));
    assert_eq!(backend.events_snapshot(), vec!["mouse_click:left@500,300"]);
}

#[tokio::test]
async fn mouse_drag_parses_start_and_end_coordinates() {
    let backend = Arc::new(MockBackend::new());
    let tool = InputTool::with_backend_and_settings(
        backend.clone(),
        Arc::new(AllowConfirmation),
        false,
    );

    let result = tool
        .execute(json!({
            "action": "mouse_drag",
            "from_x": 10,
            "from_y": 20,
            "to_x": 30,
            "to_y": 40
        }))
        .await
        .unwrap();

    assert_eq!(result["status"], json!("ok"));
    assert_eq!(backend.events_snapshot(), vec!["mouse_drag:10,20->30,40"]);
}

#[tokio::test]
async fn scroll_parses_direction_and_amount() {
    let backend = Arc::new(MockBackend::new());
    let tool = InputTool::with_backend_and_settings(
        backend.clone(),
        Arc::new(AllowConfirmation),
        false,
    );

    let result = tool
        .execute(json!({
            "action": "scroll",
            "direction": "down",
            "amount": 3
        }))
        .await
        .unwrap();

    assert_eq!(result["status"], json!("ok"));
    assert_eq!(backend.events_snapshot(), vec!["scroll:down:3"]);
}

#[tokio::test]
async fn invalid_action_name_returns_error_status() {
    let backend = Arc::new(MockBackend::new());
    let tool = InputTool::with_backend_and_settings(
        backend,
        Arc::new(AllowConfirmation),
        false,
    );

    let result = tool
        .execute(json!({
            "action": "teleport_mouse"
        }))
        .await
        .unwrap();

    assert_eq!(result["status"], json!("error"));
    assert_eq!(result["action"], json!("teleport_mouse"));
}

#[tokio::test]
async fn missing_required_fields_return_error_status() {
    let backend = Arc::new(MockBackend::new());
    let tool = InputTool::with_backend_and_settings(
        backend,
        Arc::new(AllowConfirmation),
        false,
    );

    let result = tool
        .execute(json!({
            "action": "mouse_click",
            "button": "left"
        }))
        .await
        .unwrap();

    assert_eq!(result["status"], json!("error"));
    assert_eq!(result["action"], json!("mouse_click"));
    assert!(result["detail"].as_str().unwrap().contains("x"));
}
