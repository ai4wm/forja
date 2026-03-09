#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Content, Message, Role};

    // ─── Message 생성 ─────────────────────────────────────────────────────────

    #[test]
    fn test_message_text_creation() {
        let msg = Message::text(Role::User, "안녕하세요", None);
        assert_eq!(msg.role, Role::User);
        assert!(!msg.id.is_empty());
        assert!(msg.timestamp > 0);

        match msg.content {
            Content::Text { text, .. } => assert_eq!(text, "안녕하세요"),
            _ => panic!("Expected Text content"),
        }
    }

    #[test]
    fn test_message_tool_call_creation() {
        let args = serde_json::json!({ "path": "/tmp/test.txt" });
        let msg = Message::tool_call("call-001", "file_read", args.clone(), None);

        assert_eq!(msg.role, Role::Assistant);
        match &msg.content {
            Content::ToolCall { call_id, tool_name, arguments, .. } => {
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

    // ─── Content enum 매칭 ────────────────────────────────────────────────────

    #[test]
    fn test_content_enum_variants() {
        let text_content = Content::Text { text: "hi".to_string(), thought_signature: None };
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
        // "Hello World" = 11바이트
        assert_eq!(msg.content_text_len(), 11);
    }

    #[test]
    fn test_content_text_len_tool_call() {
        let args = serde_json::json!({ "cmd": "ls" });
        let msg = Message::tool_call("id", "shell", args.clone(), None);

        // tool_name.len() + serialized_args.len()
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

    // ─── metadata 빌더 패턴 ───────────────────────────────────────────────────

    #[test]
    fn test_message_with_metadata() {
        let msg = Message::text(Role::User, "test", None)
            .with_metadata("model", serde_json::json!("gpt-5.2"))
            .with_metadata("tokens", serde_json::json!(42));

        assert_eq!(msg.metadata.get("model").unwrap(), &serde_json::json!("gpt-5.2"));
        assert_eq!(msg.metadata.get("tokens").unwrap(), &serde_json::json!(42));
    }
}
