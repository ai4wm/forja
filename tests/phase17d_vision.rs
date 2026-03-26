use forja_core::traits::Tool;
use forja_tools::vision::{MockCaptureBackend, MockVisionAnalyzer};
use forja_tools::VisionTool;
use serde_json::json;
use std::sync::Arc;

#[tokio::test]
async fn capture_screen_returns_base64_png() {
    let capture = Arc::new(MockCaptureBackend::new());
    let analyzer = Arc::new(MockVisionAnalyzer::new());
    let tool = VisionTool::with_backends(capture.clone(), analyzer, false);

    let result = tool
        .execute(json!({
            "action": "capture_screen"
        }))
        .await
        .unwrap();

    assert_eq!(result["status"], json!("ok"));
    assert_eq!(result["action"], json!("capture_screen"));
    assert!(!result["data"].as_str().unwrap().is_empty());
    assert_eq!(capture.calls_snapshot(), vec!["capture_full"]);
}

#[tokio::test]
async fn capture_region_parses_coordinates() {
    let capture = Arc::new(MockCaptureBackend::new());
    let analyzer = Arc::new(MockVisionAnalyzer::new());
    let tool = VisionTool::with_backends(capture.clone(), analyzer, false);

    let result = tool
        .execute(json!({
            "action": "capture_region",
            "x": 100,
            "y": 200,
            "width": 500,
            "height": 300
        }))
        .await
        .unwrap();

    assert_eq!(result["status"], json!("ok"));
    assert_eq!(capture.calls_snapshot(), vec!["capture_region:100,200,500,300"]);
}

#[tokio::test]
async fn capture_region_missing_width_returns_error() {
    let tool = VisionTool::with_backends(
        Arc::new(MockCaptureBackend::new()),
        Arc::new(MockVisionAnalyzer::new()),
        false,
    );

    let result = tool
        .execute(json!({
            "action": "capture_region",
            "x": 100,
            "y": 200,
            "height": 300
        }))
        .await
        .unwrap();

    assert_eq!(result["status"], json!("error"));
    assert_eq!(result["action"], json!("capture_region"));
}

#[tokio::test]
async fn analyze_requires_prompt_and_records_call() {
    let capture = Arc::new(MockCaptureBackend::new());
    let analyzer = Arc::new(MockVisionAnalyzer::new());
    let tool = VisionTool::with_backends(capture.clone(), analyzer.clone(), false);

    let result = tool
        .execute(json!({
            "action": "analyze",
            "prompt": "What is on the screen?"
        }))
        .await
        .unwrap();

    assert_eq!(result["status"], json!("ok"));
    assert_eq!(capture.calls_snapshot(), vec!["capture_full"]);
    assert_eq!(
        analyzer.calls_snapshot(),
        vec!["analyze:What is on the screen?"]
    );
}

#[tokio::test]
async fn analyze_missing_prompt_returns_error() {
    let tool = VisionTool::with_backends(
        Arc::new(MockCaptureBackend::new()),
        Arc::new(MockVisionAnalyzer::new()),
        false,
    );

    let result = tool
        .execute(json!({
            "action": "analyze"
        }))
        .await
        .unwrap();

    assert_eq!(result["status"], json!("error"));
    assert_eq!(result["action"], json!("analyze"));
}

#[tokio::test]
async fn analyze_region_records_capture_and_prompt() {
    let capture = Arc::new(MockCaptureBackend::new());
    let analyzer = Arc::new(MockVisionAnalyzer::new());
    let tool = VisionTool::with_backends(capture.clone(), analyzer.clone(), false);

    let result = tool
        .execute(json!({
            "action": "analyze_region",
            "x": 10,
            "y": 20,
            "width": 30,
            "height": 40,
            "prompt": "Describe this area"
        }))
        .await
        .unwrap();

    assert_eq!(result["status"], json!("ok"));
    assert_eq!(capture.calls_snapshot(), vec!["capture_region:10,20,30,40"]);
    assert_eq!(analyzer.calls_snapshot(), vec!["analyze:Describe this area"]);
}

#[tokio::test]
async fn find_element_uses_special_prompt() {
    let capture = Arc::new(MockCaptureBackend::new());
    let analyzer = Arc::new(MockVisionAnalyzer::new());
    let tool = VisionTool::with_backends(capture.clone(), analyzer.clone(), false);

    let result = tool
        .execute(json!({
            "action": "find_element",
            "description": "red login button"
        }))
        .await
        .unwrap();

    assert_eq!(result["status"], json!("ok"));
    assert_eq!(
        result["data"],
        json!("{\"x\": 100, \"y\": 200, \"width\": 50, \"height\": 30}")
    );
    assert_eq!(capture.calls_snapshot(), vec!["capture_full"]);
    assert_eq!(
        analyzer.calls_snapshot(),
        vec!["analyze:Find the UI element matching: red login button. Return JSON: {\"x\": number, \"y\": number, \"width\": number, \"height\": number}. If not found, return NONE."]
    );
}

#[tokio::test]
async fn find_element_none_returns_not_found_message() {
    let capture = Arc::new(MockCaptureBackend::new());
    let analyzer = Arc::new(MockVisionAnalyzer::with_response("NONE"));
    let tool = VisionTool::with_backends(capture, analyzer, false);

    let result = tool
        .execute(json!({
            "action": "find_element",
            "description": "blue save button"
        }))
        .await
        .unwrap();

    assert_eq!(result["status"], json!("ok"));
    assert_eq!(result["data"], json!("Element not found: blue save button"));
}

#[tokio::test]
async fn find_element_missing_description_returns_error() {
    let tool = VisionTool::with_backends(
        Arc::new(MockCaptureBackend::new()),
        Arc::new(MockVisionAnalyzer::new()),
        false,
    );

    let result = tool
        .execute(json!({
            "action": "find_element"
        }))
        .await
        .unwrap();

    assert_eq!(result["status"], json!("error"));
    assert_eq!(result["action"], json!("find_element"));
}

#[tokio::test]
async fn ocr_uses_region_and_fixed_prompt() {
    let capture = Arc::new(MockCaptureBackend::new());
    let analyzer = Arc::new(MockVisionAnalyzer::new());
    let tool = VisionTool::with_backends(capture.clone(), analyzer.clone(), false);

    let result = tool
        .execute(json!({
            "action": "ocr",
            "x": 0,
            "y": 0,
            "width": 1920,
            "height": 1080
        }))
        .await
        .unwrap();

    assert_eq!(result["status"], json!("ok"));
    assert_eq!(capture.calls_snapshot(), vec!["capture_region:0,0,1920,1080"]);
    assert_eq!(
        analyzer.calls_snapshot(),
        vec!["analyze:Read all text visible in this image. Return the text exactly as shown."]
    );
}

#[tokio::test]
async fn invalid_action_name_returns_error() {
    let tool = VisionTool::with_backends(
        Arc::new(MockCaptureBackend::new()),
        Arc::new(MockVisionAnalyzer::new()),
        false,
    );

    let result = tool
        .execute(json!({
            "action": "scan_everything"
        }))
        .await
        .unwrap();

    assert_eq!(result["status"], json!("error"));
    assert_eq!(result["action"], json!("scan_everything"));
}

#[tokio::test]
async fn mock_backends_record_all_calls() {
    let capture = Arc::new(MockCaptureBackend::new());
    let analyzer = Arc::new(MockVisionAnalyzer::new());
    let tool = VisionTool::with_backends(capture.clone(), analyzer.clone(), false);

    let _ = tool.execute(json!({ "action": "capture_screen" })).await.unwrap();
    let _ = tool
        .execute(json!({
            "action": "analyze",
            "prompt": "Check layout"
        }))
        .await
        .unwrap();

    assert_eq!(capture.calls_snapshot(), vec!["capture_full", "capture_full"]);
    assert_eq!(analyzer.calls_snapshot(), vec!["analyze:Check layout"]);
}
