use async_trait::async_trait;
use forja_core::traits::Tool;
use forja_tools::browser::MockBrowserBackend;
use forja_tools::confirm::ConfirmationHandler;
use forja_tools::BrowserTool;
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
async fn open_action_parses_valid_url() {
    let backend = Arc::new(MockBrowserBackend::new());
    let tool = BrowserTool::with_backend_and_settings(
        backend.clone(),
        Arc::new(AllowConfirmation),
        false,
    );

    let result = tool
        .execute(json!({
            "action": "open",
            "url": "https://google.com"
        }))
        .await
        .unwrap();

    assert_eq!(result["status"], json!("ok"));
    assert_eq!(result["action"], json!("open"));
    assert_eq!(backend.calls_snapshot(), vec!["open:https://google.com"]);
}

#[tokio::test]
async fn goto_action_parses_valid_url() {
    let backend = Arc::new(MockBrowserBackend::new());
    let tool = BrowserTool::with_backend_and_settings(
        backend.clone(),
        Arc::new(AllowConfirmation),
        false,
    );

    let result = tool
        .execute(json!({
            "action": "goto",
            "url": "https://example.com"
        }))
        .await
        .unwrap();

    assert_eq!(result["status"], json!("ok"));
    assert_eq!(backend.calls_snapshot(), vec!["goto:https://example.com"]);
}

#[tokio::test]
async fn scroll_action_parses_direction_and_amount() {
    let backend = Arc::new(MockBrowserBackend::new());
    let tool = BrowserTool::with_backend_and_settings(
        backend.clone(),
        Arc::new(AllowConfirmation),
        false,
    );

    let result = tool
        .execute(json!({
            "action": "scroll",
            "direction": "down",
            "amount": 500
        }))
        .await
        .unwrap();

    assert_eq!(result["status"], json!("ok"));
    assert_eq!(backend.calls_snapshot(), vec!["scroll:down:500"]);
}

#[tokio::test]
async fn click_action_parses_selector() {
    let backend = Arc::new(MockBrowserBackend::new());
    let tool = BrowserTool::with_backend_and_settings(
        backend.clone(),
        Arc::new(AllowConfirmation),
        false,
    );

    let result = tool
        .execute(json!({
            "action": "click",
            "selector": "button.submit"
        }))
        .await
        .unwrap();

    assert_eq!(result["status"], json!("ok"));
    assert_eq!(backend.calls_snapshot(), vec!["click:button.submit"]);
}

#[tokio::test]
async fn type_text_action_parses_selector_and_text() {
    let backend = Arc::new(MockBrowserBackend::new());
    let tool = BrowserTool::with_backend_and_settings(
        backend.clone(),
        Arc::new(AllowConfirmation),
        false,
    );

    let result = tool
        .execute(json!({
            "action": "type_text",
            "selector": "input#search",
            "text": "forja rust"
        }))
        .await
        .unwrap();

    assert_eq!(result["status"], json!("ok"));
    assert_eq!(
        backend.calls_snapshot(),
        vec!["type_text:input#search:forja rust"]
    );
}

#[tokio::test]
async fn read_text_action_parses_selector() {
    let backend = Arc::new(MockBrowserBackend::new());
    let tool = BrowserTool::with_backend_and_settings(
        backend.clone(),
        Arc::new(AllowConfirmation),
        false,
    );

    let result = tool
        .execute(json!({
            "action": "read_text",
            "selector": "h1"
        }))
        .await
        .unwrap();

    assert_eq!(result["status"], json!("ok"));
    assert_eq!(result["data"], json!("mock text"));
    assert_eq!(backend.calls_snapshot(), vec!["read_text:h1"]);
}

#[tokio::test]
async fn read_page_action_needs_no_params() {
    let backend = Arc::new(MockBrowserBackend::new());
    let tool = BrowserTool::with_backend_and_settings(
        backend.clone(),
        Arc::new(AllowConfirmation),
        false,
    );

    let result = tool.execute(json!({ "action": "read_page" })).await.unwrap();

    assert_eq!(result["status"], json!("ok"));
    assert_eq!(result["data"], json!("mock page text"));
    assert_eq!(backend.calls_snapshot(), vec!["read_page"]);
}

#[tokio::test]
async fn screenshot_action_returns_mock_base64() {
    let backend = Arc::new(MockBrowserBackend::new());
    let tool = BrowserTool::with_backend_and_settings(
        backend.clone(),
        Arc::new(AllowConfirmation),
        false,
    );

    let result = tool.execute(json!({ "action": "screenshot" })).await.unwrap();

    assert_eq!(result["status"], json!("ok"));
    assert_eq!(result["data"], json!("bW9jay1zY3JlZW5zaG90"));
    assert_eq!(backend.calls_snapshot(), vec!["screenshot"]);
}

#[tokio::test]
async fn evaluate_action_parses_javascript_string() {
    let backend = Arc::new(MockBrowserBackend::new());
    let tool = BrowserTool::with_backend_and_settings(
        backend.clone(),
        Arc::new(AllowConfirmation),
        false,
    );

    let result = tool
        .execute(json!({
            "action": "evaluate",
            "js": "document.title"
        }))
        .await
        .unwrap();

    assert_eq!(result["status"], json!("ok"));
    assert_eq!(result["data"], json!("mock eval result"));
    assert_eq!(backend.calls_snapshot(), vec!["evaluate:document.title"]);
}

#[tokio::test]
async fn evaluate_blocks_document_cookie() {
    let backend = Arc::new(MockBrowserBackend::new());
    let tool = BrowserTool::with_backend_and_settings(
        backend.clone(),
        Arc::new(DenyConfirmation),
        false,
    );

    let result = tool
        .execute(json!({
            "action": "evaluate",
            "js": "document.cookie"
        }))
        .await
        .unwrap();

    assert_eq!(result["status"], json!("blocked"));
    assert!(backend.calls_snapshot().is_empty());
}

#[tokio::test]
async fn evaluate_blocks_local_storage_clear() {
    let backend = Arc::new(MockBrowserBackend::new());
    let tool = BrowserTool::with_backend_and_settings(
        backend.clone(),
        Arc::new(DenyConfirmation),
        false,
    );

    let result = tool
        .execute(json!({
            "action": "evaluate",
            "js": "LOCALSTORAGE.CLEAR()"
        }))
        .await
        .unwrap();

    assert_eq!(result["status"], json!("blocked"));
    assert!(backend.calls_snapshot().is_empty());
}

#[tokio::test]
async fn evaluate_blocks_external_fetch_with_http() {
    let backend = Arc::new(MockBrowserBackend::new());
    let tool = BrowserTool::with_backend_and_settings(
        backend.clone(),
        Arc::new(DenyConfirmation),
        false,
    );

    let result = tool
        .execute(json!({
            "action": "evaluate",
            "js": "fetch('http://example.com')"
        }))
        .await
        .unwrap();

    assert_eq!(result["status"], json!("blocked"));
    assert!(backend.calls_snapshot().is_empty());
}

#[tokio::test]
async fn browser_unsafe_mode_bypasses_evaluate_safety() {
    let backend = Arc::new(MockBrowserBackend::new());
    let tool = BrowserTool::with_backend_and_settings(
        backend.clone(),
        Arc::new(DenyConfirmation),
        true,
    );

    let result = tool
        .execute(json!({
            "action": "evaluate",
            "js": "document.cookie"
        }))
        .await
        .unwrap();

    assert_eq!(result["status"], json!("ok"));
    assert_eq!(backend.calls_snapshot(), vec!["evaluate:document.cookie"]);
}

#[tokio::test]
async fn tab_actions_parse_indices() {
    let backend = Arc::new(MockBrowserBackend::new());
    let tool = BrowserTool::with_backend_and_settings(
        backend.clone(),
        Arc::new(AllowConfirmation),
        false,
    );

    let list_result = tool.execute(json!({ "action": "tab_list" })).await.unwrap();
    let switch_result = tool
        .execute(json!({
            "action": "tab_switch",
            "index": 2
        }))
        .await
        .unwrap();
    let close_result = tool
        .execute(json!({
            "action": "tab_close",
            "index": 1
        }))
        .await
        .unwrap();

    assert_eq!(list_result["status"], json!("ok"));
    assert_eq!(switch_result["status"], json!("ok"));
    assert_eq!(close_result["status"], json!("ok"));
    assert_eq!(
        backend.calls_snapshot(),
        vec!["tab_list", "tab_switch:2", "tab_close:1"]
    );
}

#[tokio::test]
async fn back_and_forward_need_no_params() {
    let backend = Arc::new(MockBrowserBackend::new());
    let tool = BrowserTool::with_backend_and_settings(
        backend.clone(),
        Arc::new(AllowConfirmation),
        false,
    );

    let back_result = tool.execute(json!({ "action": "back" })).await.unwrap();
    let forward_result = tool.execute(json!({ "action": "forward" })).await.unwrap();

    assert_eq!(back_result["status"], json!("ok"));
    assert_eq!(forward_result["status"], json!("ok"));
    assert_eq!(backend.calls_snapshot(), vec!["back", "forward"]);
}

#[tokio::test]
async fn invalid_action_name_returns_error() {
    let backend = Arc::new(MockBrowserBackend::new());
    let tool = BrowserTool::with_backend_and_settings(
        backend,
        Arc::new(AllowConfirmation),
        false,
    );

    let result = tool
        .execute(json!({
            "action": "teleport"
        }))
        .await
        .unwrap();

    assert_eq!(result["status"], json!("error"));
    assert_eq!(result["action"], json!("teleport"));
}

#[tokio::test]
async fn missing_required_fields_return_error() {
    let backend = Arc::new(MockBrowserBackend::new());
    let tool = BrowserTool::with_backend_and_settings(
        backend,
        Arc::new(AllowConfirmation),
        false,
    );

    let result = tool
        .execute(json!({
            "action": "click"
        }))
        .await
        .unwrap();

    assert_eq!(result["status"], json!("error"));
    assert_eq!(result["action"], json!("click"));
    assert!(result["data"].as_str().unwrap().contains("selector"));
}
