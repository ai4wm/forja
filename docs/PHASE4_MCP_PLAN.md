# forja-mcp (Model Context Protocol) 구현 계획

## 1. 개요 및 클라이언트 역할 정의
`forja-mcp` 크레이트는 Anthropic 등이 제안한 **Model Context Protocol (MCP)** 표준을 준수하는 "MCP 클라이언트"로 동작합니다. 
이 클라이언트의 주된 역할은 **외부에 독립적인 프로세스로 띄워져 있는 MCP 서버(Local stdio 방식 또는 SSE 기반)들에 접속**하여, 서버가 노출하는 기능을 Forja 엔진의 생태계로 끌어오는 것입니다.

---

## 2. JSON-RPC 기반 통신 구조

MCP는 기본적으로 **JSON-RPC 2.0** 스펙을 사용합니다.
- **Transport 레이어**: Phase 2에서는 우선 가장 범용적인 **stdio (표준 입출력)** 기반의 하위 프로세스 통신을 구현합니다.
- **메시지 타입**: 
  - `Request`: 클라이언트가 서버로 작업을 지시하고 응답이 필요한 메시지 (`id` 포함).
  - `Response`: 서버가 클라이언트의 Request에 대답하는 메시지 (`id` 매칭, `result` 또는 `error` 포함).
  - `Notification`: 단방향 통보 메시지.

**[코드 스니펫: JSON-RPC 기본 구조체 뼈대]**
```rust
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub id: u64,
    pub method: String,
    pub params: Option<serde_json::Value>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    pub id: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<serde_json::Value>,
}
```

---

## 3. 외부 MCP 서버 연결 / 도구 조회 / 도구 실행 흐름

1. **서버 연결 (`initialize`)**:
   - `McpClient::spawn("node", &["server.js"])` 처럼 하위 프로세스를 실행합니다.
   - 클라이언트 기능 목록(Capabilities)을 담아 `initialize` JSON-RPC 요청을 전송하고, 서버의 응답을 기다립니다.
2. **도구 목록 조회 (`tools/list`)**:
   - `tools/list` 요청을 보내어 서버가 제공하는 도구의 이름, 설명, `inputSchema` 배열을 응답받습니다.
3. **도구 래핑 및 등록 (Bridge)**:
   - 배열에 담긴 각 도구 정보를 바탕으로 `McpToolWrapper` 객체를 찍어냅니다 (동적 생성).
   - 생성된 래퍼들을 `Engine::register_tool()`로 등록합니다.
4. **도구 실행 흐름 (`tools/call`)**:
   - LLM이 해당 도구를 사용하기로 결정하면, Engine은 래퍼의 `execute(args)`를 호출합니다.
   - 래퍼 내부에서는 Client를 통해 `tools/call` 메서드로 인자를 묶어 서버에 JSON-RPC 요청을 날리고, 결과가 반환될 때까지 비동기 대기(`await`) 후 Engine에게 돌려줍니다.

---

## 4. `forja-core` Tool Trait과의 브릿지 설계

McpToolWrapper는 MCP 서버 상의 단일 도구를 `forja-core::traits::Tool` 형태로 변형시키는 어댑터입니다.

**[코드 스니펫: McpToolWrapper]**
```rust
use std::sync::Arc;
use forja_core::traits::Tool;
use forja_core::error::{ForjaError, Result};
use async_trait::async_trait;

pub struct McpToolWrapper {
    pub tool_name: String,
    pub description: String,
    pub mcp_client: Arc<McpClient>, // JSON-RPC 통신을 담당하는 Client
}

#[async_trait]
impl Tool for McpToolWrapper {
    fn name(&self) -> &str {
        &self.tool_name
    }

    fn description(&self) -> &str {
        &self.description
    }

    async fn execute(&self, arguments: serde_json::Value) -> Result<serde_json::Value> {
        // 내부적으로 JSON-RPC 'tools/call' 요청 생성 및 송신
        let response = self.mcp_client.call_tool(&self.tool_name, arguments).await
            .map_err(|e| ForjaError::ToolError(format!("MCP Server Error: {}", e)))?;
        
        Ok(response)
    }
}
```

---

## 5. 파일별 구현 순서 (Step)

1. **`Cargo.toml` 종속성**: `forja-core`, `tokio`(process 커맨드 용도), `serde`, `serde_json` 등 설정.
2. **`protocol.rs`**: JSON-RPC 메시지 타입 직렬화/역직렬화 구조체 명세 작성. 통신 버퍼 파싱 헬퍼 포함.
3. **`client.rs`**: `McpClient` 구조체 기반 구현. `tokio::process::Command`를 통한 하위 프로세스 `stdio` 입출력 파이프 스트림 연결 및 `id` 매칭 큐(Channel) 맵핑 로직.
4. **`wrapper.rs`**: `McpToolWrapper` 어댑터 클래스 구현 및 `forja-core`의 `Tool` Trait 맵핑.
5. **`lib.rs`**: 위 모듈 통합 및 간편한 초기화용(`connect_mcp_stdio`) 공개 엔트리 포인트 제공.
