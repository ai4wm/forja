use super::protocol::{
    ERR_INVALID_PARAMS, ERR_INVALID_REQUEST, ERR_METHOD_NOT_FOUND, ERR_PARSE, ERR_TOOL_NOT_FOUND,
    METHOD_INITIALIZE, METHOD_INITIALIZED, METHOD_PING, METHOD_TOOLS_CALL, METHOD_TOOLS_LIST,
    RpcRequest, error, initialize_result, success, tool_call_result, tools_list_result,
};
use super::registry::{ToolRegistry, build_default_registry};
use forja_core::error::{ForjaError, Result};
use serde_json::{Value, json};
use std::io::{self, BufRead, BufReader, Write};

pub async fn serve_stdio() -> Result<()> {
    let server = McpServer::new(
        env!("CARGO_PKG_NAME"),
        env!("CARGO_PKG_VERSION"),
        build_default_registry(),
    );
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut reader = BufReader::new(stdin.lock());
    let mut writer = stdout.lock();

    loop {
        let Some(body) = read_message(&mut reader)? else {
            break;
        };

        let request = match serde_json::from_slice::<RpcRequest>(&body) {
            Ok(request) => request,
            Err(parse_error) => {
                write_message(
                    &mut writer,
                    &error(None, ERR_PARSE, parse_error.to_string()),
                )?;
                continue;
            }
        };

        if let Some(response) = server.handle_request(request).await {
            write_message(&mut writer, &response)?;
        }
    }

    Ok(())
}

pub struct McpServer {
    server_name: &'static str,
    server_version: &'static str,
    registry: ToolRegistry,
}

impl McpServer {
    pub fn new(
        server_name: &'static str,
        server_version: &'static str,
        registry: ToolRegistry,
    ) -> Self {
        Self {
            server_name,
            server_version,
            registry,
        }
    }

    pub async fn handle_request(&self, request: RpcRequest) -> Option<Value> {
        if request.jsonrpc.as_deref() != Some("2.0") {
            return Some(error(
                request.id,
                ERR_INVALID_REQUEST,
                "jsonrpc must be '2.0'",
            ));
        }

        match request.method.as_str() {
            METHOD_INITIALIZED => None,
            METHOD_INITIALIZE => Some(success(
                request.id.unwrap_or(Value::Null),
                initialize_result(self.server_name, self.server_version),
            )),
            METHOD_PING => Some(success(request.id.unwrap_or(Value::Null), json!({}))),
            METHOD_TOOLS_LIST => Some(success(
                request.id.unwrap_or(Value::Null),
                tools_list_result(
                    self.registry
                        .values()
                        .map(|tool| tool.definition())
                        .collect::<Vec<_>>(),
                ),
            )),
            METHOD_TOOLS_CALL => Some(self.handle_tool_call(request).await),
            other => Some(error(
                request.id,
                ERR_METHOD_NOT_FOUND,
                format!("Unsupported method: {other}"),
            )),
        }
    }

    async fn handle_tool_call(&self, request: RpcRequest) -> Value {
        let id = request.id.clone().unwrap_or(Value::Null);
        let Some(name) = request.params.get("name").and_then(Value::as_str) else {
            return error(
                Some(id),
                ERR_INVALID_PARAMS,
                "Missing tools/call params.name",
            );
        };
        let arguments = request
            .params
            .get("arguments")
            .cloned()
            .unwrap_or_else(|| json!({}));

        let Some(tool) = self.registry.get(name) else {
            return error(
                Some(id),
                ERR_TOOL_NOT_FOUND,
                format!("Tool not found: {name}"),
            );
        };

        match tool.execute(arguments).await {
            Ok(result) => success(id, tool_call_result(result, false)),
            Err(error_text) => success(
                id,
                tool_call_result(
                    json!({
                        "error": error_to_string(error_text),
                        "tool": name,
                    }),
                    true,
                ),
            ),
        }
    }
}

fn read_message(reader: &mut impl BufRead) -> Result<Option<Vec<u8>>> {
    let mut content_length = None;
    let mut line = String::new();

    loop {
        line.clear();
        let bytes = reader
            .read_line(&mut line)
            .map_err(|error| ForjaError::Internal(error.to_string()))?;

        if bytes == 0 {
            return if content_length.is_some() {
                Err(ForjaError::Internal(
                    "Unexpected EOF while reading MCP headers".to_string(),
                ))
            } else {
                Ok(None)
            };
        }

        let trimmed = line.trim_end_matches(['\r', '\n']);
        if trimmed.is_empty() {
            break;
        }

        if let Some(value) = trimmed.strip_prefix("Content-Length:") {
            content_length = value.trim().parse::<usize>().ok();
        }
    }

    let Some(content_length) = content_length else {
        return Ok(None);
    };

    let mut body = vec![0u8; content_length];
    reader
        .read_exact(&mut body)
        .map_err(|error| ForjaError::Internal(error.to_string()))?;
    Ok(Some(body))
}

fn write_message(writer: &mut impl Write, message: &Value) -> Result<()> {
    let body = serde_json::to_vec(message)?;
    writer
        .write_all(format!("Content-Length: {}\r\n\r\n", body.len()).as_bytes())
        .map_err(|error| ForjaError::Internal(error.to_string()))?;
    writer
        .write_all(&body)
        .and_then(|_| writer.flush())
        .map_err(|error| ForjaError::Internal(error.to_string()))?;
    Ok(())
}

fn error_to_string(error: ForjaError) -> String {
    error.to_string()
}

#[cfg(test)]
mod tests {
    use super::{McpServer, read_message};
    use crate::mcp::protocol::{
        METHOD_INITIALIZE, METHOD_TOOLS_CALL, METHOD_TOOLS_LIST, RpcRequest,
    };
    use crate::mcp::registry::build_default_registry;
    use serde_json::json;
    use std::io::Cursor;

    #[tokio::test]
    async fn initialize_returns_capabilities() {
        let server = McpServer::new("forja-tools", "0.1.2", build_default_registry());
        let response = server
            .handle_request(RpcRequest {
                jsonrpc: Some("2.0".to_string()),
                id: Some(json!(1)),
                method: METHOD_INITIALIZE.to_string(),
                params: json!({}),
            })
            .await
            .expect("initialize should respond");

        assert_eq!(
            response["result"]["capabilities"]["tools"]["listChanged"],
            false
        );
    }

    #[tokio::test]
    async fn tools_list_returns_registered_tools() {
        let server = McpServer::new("forja-tools", "0.1.2", build_default_registry());
        let response = server
            .handle_request(RpcRequest {
                jsonrpc: Some("2.0".to_string()),
                id: Some(json!(2)),
                method: METHOD_TOOLS_LIST.to_string(),
                params: json!({}),
            })
            .await
            .expect("tools/list should respond");

        assert!(
            response["result"]["tools"]
                .as_array()
                .expect("tools array")
                .iter()
                .any(|tool| tool["name"] == "file_tool")
        );
    }

    #[tokio::test]
    async fn tools_call_returns_error_for_missing_tool() {
        let server = McpServer::new("forja-tools", "0.1.2", build_default_registry());
        let response = server
            .handle_request(RpcRequest {
                jsonrpc: Some("2.0".to_string()),
                id: Some(json!(3)),
                method: METHOD_TOOLS_CALL.to_string(),
                params: json!({ "name": "missing_tool", "arguments": {} }),
            })
            .await
            .expect("missing tool should yield error response");

        assert_eq!(response["error"]["code"], -32001);
    }

    #[test]
    fn read_message_parses_content_length_framing() {
        let body = b"{\"jsonrpc\":\"2.0\"}";
        let framed = format!(
            "Content-Length: {}\r\n\r\n{}",
            body.len(),
            std::str::from_utf8(body).expect("utf8 body")
        );
        let mut reader = Cursor::new(framed);
        let message = read_message(&mut reader).expect("message should parse");
        assert_eq!(message, Some(body.to_vec()));
    }
}
