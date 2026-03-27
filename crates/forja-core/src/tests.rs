use crate::types::{Content, Message, Role};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

// Message creation

#[test]
fn test_message_text_creation() {
    let msg = Message::text(Role::User, "hello", None);
    assert_eq!(msg.role, Role::User);
    assert!(!msg.id.is_empty());
    assert!(msg.timestamp > 0);

    match msg.content {
        Content::Text { text, .. } => assert_eq!(text, "hello"),
        _ => panic!("Expected Text content"),
    }
}

#[test]
fn test_message_tool_call_creation() {
    let args = serde_json::json!({ "path": "/tmp/test.txt" });
    let msg = Message::tool_call("call-001", "file_read", args.clone(), None);

    assert_eq!(msg.role, Role::Assistant);
    match &msg.content {
        Content::ToolCall {
            call_id,
            tool_name,
            arguments,
            ..
        } => {
            assert_eq!(call_id, "call-001");
            assert_eq!(tool_name, "file_read");
            assert_eq!(arguments, &args);
        }
        _ => panic!("Expected ToolCall content"),
    }
}

#[test]
fn test_message_tool_result_creation() {
    let result = serde_json::json!({ "ok": true, "content": "hello" });
    let msg = Message::tool_result("call-001", result.clone());

    assert_eq!(msg.role, Role::Tool);
    match &msg.content {
        Content::ToolResult { call_id, result: r } => {
            assert_eq!(call_id, "call-001");
            assert_eq!(r, &result);
        }
        _ => panic!("Expected ToolResult content"),
    }
}

// Content enum matching

#[test]
fn test_content_enum_variants() {
    let text_content = Content::Text {
        text: "hi".to_string(),
        thought_signature: None,
    };
    let tool_call_content = Content::ToolCall {
        call_id: "id1".to_string(),
        tool_name: "shell".to_string(),
        arguments: serde_json::Value::Null,
        reasoning_content: None,
        thought_signature: None,
    };
    let tool_result_content = Content::ToolResult {
        call_id: "id1".to_string(),
        result: serde_json::json!("done"),
    };

    assert!(matches!(text_content, Content::Text { .. }));
    assert!(matches!(tool_call_content, Content::ToolCall { .. }));
    assert!(matches!(tool_result_content, Content::ToolResult { .. }));
}

// ─── content_text_len() ───────────────────────────────────────────────────

#[test]
fn test_content_text_len_text() {
    let msg = Message::text(Role::User, "Hello World", None);
    assert_eq!(msg.content_text_len(), 11);
}

#[test]
fn test_content_text_len_tool_call() {
    let args = serde_json::json!({ "cmd": "ls" });
    let msg = Message::tool_call("id", "shell", args.clone(), None);

    let expected = "shell".len() + args.to_string().len();
    assert_eq!(msg.content_text_len(), expected);
}

#[test]
fn test_content_text_len_tool_result() {
    let result = serde_json::json!("done");
    let msg = Message::tool_result("id", result.clone());

    let expected = result.to_string().len();
    assert_eq!(msg.content_text_len(), expected);
}

#[test]
fn test_content_text_len_empty() {
    let msg = Message::text(Role::User, "", None);
    assert_eq!(msg.content_text_len(), 0);
}

// ─── Role PartialEq ───────────────────────────────────────────────────────

#[test]
fn test_role_equality() {
    assert_eq!(Role::User, Role::User);
    assert_ne!(Role::User, Role::Assistant);
    assert_ne!(Role::System, Role::Tool);
}

// metadata builder pattern

#[test]
fn test_message_with_metadata() {
    let msg = Message::text(Role::User, "test", None)
        .with_metadata("model", serde_json::json!("gpt-5.2"))
        .with_metadata("tokens", serde_json::json!(42));

    assert_eq!(
        msg.metadata.get("model").unwrap(),
        &serde_json::json!("gpt-5.2")
    );
    assert_eq!(msg.metadata.get("tokens").unwrap(), &serde_json::json!(42));
}

fn unique_temp_dir(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    std::env::temp_dir().join(format!("forja_core_{name}_{nanos}"))
}

#[test]
fn prompt_loader_falls_back_to_embedded_defaults() {
    let prompts_dir = unique_temp_dir("prompt_loader_fallback");
    let loader = crate::prompt::loader::PromptLoader::new(prompts_dir.as_path());

    let base = loader.load_base("Forja", "User");

    assert!(base.contains("You are Forja, a personal AI assistant."));
    assert!(base.contains("Address the user as \"User\""));
    assert_eq!(
        loader.load_role("coder"),
        crate::prompt::coder::DEFAULT_CODER_PROMPT
    );
    assert_eq!(
        loader.load_think("max"),
        crate::prompt::think::DEFAULT_THINK_MAX
    );
    assert_eq!(
        loader.load_memory_rules(),
        crate::prompt::loader::DEFAULT_MEMORY_RULES
    );
    assert_eq!(loader.load_file("roles/missing.md"), None);
}

#[test]
fn prompt_loader_prefers_prompt_files_on_disk() {
    let prompts_dir = unique_temp_dir("prompt_loader_disk");
    std::fs::create_dir_all(prompts_dir.join("roles")).unwrap();
    std::fs::create_dir_all(prompts_dir.join("think")).unwrap();
    std::fs::write(
        prompts_dir.join("base.md"),
        "Base for {assistant_name} -> {user_title}",
    )
    .unwrap();
    std::fs::write(prompts_dir.join("roles").join("coder.md"), "disk coder").unwrap();
    std::fs::write(prompts_dir.join("think").join("min.md"), "disk think").unwrap();
    std::fs::write(prompts_dir.join("memory-rules.md"), "disk memory rules").unwrap();

    let loader = crate::prompt::loader::PromptLoader::new(prompts_dir.as_path());

    assert_eq!(
        loader.load_base("Nova", "Captain"),
        "Base for Nova -> Captain"
    );
    assert_eq!(loader.load_role("coder"), "disk coder");
    assert_eq!(loader.load_think("min"), "disk think");
    assert_eq!(loader.load_memory_rules(), "disk memory rules");
    assert_eq!(
        loader.load_file("roles/coder.md"),
        Some("disk coder".to_string())
    );

    let _ = std::fs::remove_dir_all(prompts_dir);
}

#[test]
fn prompt_loader_writes_missing_default_files_without_overwriting_existing_files() {
    let prompts_dir = unique_temp_dir("prompt_loader_bootstrap");
    std::fs::create_dir_all(prompts_dir.join("roles")).unwrap();
    std::fs::write(prompts_dir.join("roles").join("coder.md"), "custom coder").unwrap();

    let loader = crate::prompt::loader::PromptLoader::new(prompts_dir.as_path());
    loader.ensure_default_files().unwrap();

    assert_eq!(
        std::fs::read_to_string(prompts_dir.join("roles").join("coder.md")).unwrap(),
        "custom coder"
    );
    assert!(prompts_dir.join("base.md").exists());
    assert!(prompts_dir.join("memory-rules.md").exists());
    assert!(prompts_dir.join("roles").join("writer.md").exists());
    assert!(prompts_dir.join("roles").join("assistant.md").exists());
    assert!(prompts_dir.join("roles").join("analyst.md").exists());
    assert!(prompts_dir.join("think").join("min.md").exists());
    assert!(prompts_dir.join("think").join("max.md").exists());

    let _ = std::fs::remove_dir_all(prompts_dir);
}

#[test]
fn assemble_system_prompt_uses_prompt_loader_content() {
    let prompts_dir = unique_temp_dir("assemble_system_prompt");
    std::fs::create_dir_all(prompts_dir.join("roles")).unwrap();
    std::fs::create_dir_all(prompts_dir.join("think")).unwrap();
    std::fs::write(
        prompts_dir.join("base.md"),
        "Base prompt for {assistant_name} and {user_title}",
    )
    .unwrap();
    std::fs::write(prompts_dir.join("roles").join("coder.md"), "role from disk").unwrap();
    std::fs::write(prompts_dir.join("think").join("max.md"), "think from disk").unwrap();

    let loader = crate::prompt::loader::PromptLoader::new(prompts_dir.as_path());
    let mode_state = crate::mode::ModeState::new(
        crate::mode::ExecMode::Auto,
        crate::mode::ThinkLevel::Max,
        crate::mode::Role::Coder,
    );

    let prompt = crate::prompt::assemble_system_prompt(
        &loader,
        &mode_state,
        "Forja",
        "Captain",
        "identity section",
        "",
        "tools section",
        "",
        "",
        "",
        "",
    );

    assert!(prompt.contains("Base prompt for Forja and Captain"));
    assert!(prompt.contains("think from disk"));
    assert!(prompt.contains("role from disk"));
    assert!(prompt.contains("identity section"));
    assert!(prompt.contains("tools section"));

    let _ = std::fs::remove_dir_all(prompts_dir);
}
