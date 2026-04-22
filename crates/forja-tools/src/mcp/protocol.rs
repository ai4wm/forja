use forja_core::types::ToolDefinition;
use serde::Deserialize;
use serde_json::{Value, json};

pub const JSONRPC_VERSION: &str = "2.0";
pub const MCP_PROTOCOL_VERSION: &str = "2024-11-05";
pub const METHOD_INITIALIZE: &str = "initialize";
pub const METHOD_INITIALIZED: &str = "notifications/initialized";
pub const METHOD_TOOLS_LIST: &str = "tools/list";
pub const METHOD_TOOLS_CALL: &str = "tools/call";
pub const METHOD_PING: &str = "ping";
pub const ERR_PARSE: i64 = -32700;
pub const ERR_INVALID_REQUEST: i64 = -32600;
pub const ERR_METHOD_NOT_FOUND: i64 = -32601;
pub const ERR_INVALID_PARAMS: i64 = -32602;
pub const ERR_TOOL_NOT_FOUND: i64 = -32001;

#[derive(Debug, Deserialize)]
pub struct RpcRequest {
    #[serde(default)]
    pub jsonrpc: Option<String>,
    #[serde(default)]
    pub id: Option<Value>,
    pub method: String,
    #[serde(default)]
    pub params: Value,
}

pub fn success(id: Value, result: Value) -> Value {
    json!({
        "jsonrpc": JSONRPC_VERSION,
        "id": id,
        "result": result,
    })
}

pub fn error(id: Option<Value>, code: i64, message: impl Into<String>) -> Value {
    json!({
        "jsonrpc": JSONRPC_VERSION,
        "id": id.unwrap_or(Value::Null),
        "error": {
            "code": code,
            "message": message.into(),
        }
    })
}

pub fn initialize_result(server_name: &str, server_version: &str) -> Value {
    json!({
        "protocolVersion": MCP_PROTOCOL_VERSION,
        "capabilities": {
            "tools": {
                "listChanged": false
            }
        },
        "serverInfo": {
            "name": server_name,
            "version": server_version
        }
    })
}

pub fn tools_list_result(definitions: Vec<ToolDefinition>) -> Value {
    let tools = definitions
        .into_iter()
        .map(|tool| {
            json!({
                "name": tool.name,
                "description": tool.description,
                "inputSchema": tool.parameters,
            })
        })
        .collect::<Vec<_>>();
    json!({ "tools": tools })
}

pub fn tool_call_result(result: Value, is_error: bool) -> Value {
    json!({
        "content": [text_content(value_to_text(&result))],
        "isError": is_error,
    })
}

pub fn text_content(text: impl Into<String>) -> Value {
    json!({
        "type": "text",
        "text": text.into(),
    })
}

pub fn value_to_text(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        _ => serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::{error, tool_call_result, tools_list_result, value_to_text};
    use forja_core::types::ToolDefinition;
    use serde_json::json;

    #[test]
    fn tools_list_uses_input_schema() {
        let result = tools_list_result(vec![ToolDefinition {
            name: "demo".to_string(),
            description: "demo tool".to_string(),
            parameters: json!({"type":"object"}),
        }]);

        assert_eq!(result["tools"][0]["name"], "demo");
        assert_eq!(result["tools"][0]["inputSchema"]["type"], "object");
    }

    #[test]
    fn tool_call_result_wraps_text_content() {
        let result = tool_call_result(json!({"status":"ok"}), false);
        assert_eq!(result["content"][0]["type"], "text");
        assert!(value_to_text(&json!({"status":"ok"})).contains("status"));
    }

    #[test]
    fn error_includes_null_id_when_missing() {
        let response = error(None, -1, "boom");
        assert_eq!(response["id"], serde_json::Value::Null);
        assert_eq!(response["error"]["message"], "boom");
    }
}
