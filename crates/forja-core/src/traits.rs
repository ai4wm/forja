use crate::error::Result;
use crate::types::{MemoryEntry, Message, ToolDefinition};
use async_trait::async_trait;
use std::pin::Pin;
use tokio_stream::Stream;

/// LLM 프로바이더 (forja-llm에서 구현: Anthropic, OpenAI 등).
#[async_trait]
pub trait LlmProvider: Send + Sync {
    /// 단일 응답 (도구 호출 정보 포함 가능).
    async fn chat(&self, messages: &[Message], tools: Option<&[ToolDefinition]>) -> Result<Message>;

    /// 토큰 단위 스트리밍 응답 (도구 목록 포함 가능).
    async fn stream(
        &self,
        messages: &[Message],
        tools: Option<&[ToolDefinition]>,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<String>> + Send>>>;
}

/// 기억 저장소 (forja-memory에서 구현: 마크다운 파일, 벡터 DB 등).
/// search는 MemoryEntry를 반환하여 점수 기반 정렬을 지원.
#[async_trait]
pub trait MemoryStore: Send + Sync {
    async fn save(&self, entry: &MemoryEntry) -> Result<()>;
    async fn search(&self, query: &str, limit: usize) -> Result<Vec<MemoryEntry>>;
    async fn flush(&self) -> Result<()>;
}

/// 입출력 채널 (forja-channel에서 구현: CLI, Telegram, Discord 등).
///
/// # 설계 결정: &self vs &mut self
/// Channel trait은 &self를 사용한다.
/// CLI stdin처럼 가변 상태가 필요한 구현체는 내부적으로
/// `Mutex<BufReader<Stdin>>` 등을 사용하여 interior mutability로 해결한다.
/// 이렇게 하면 Arc<dyn Channel>로 공유가 가능해 멀티 에이전트 시나리오에서 유리하다.
#[async_trait]
pub trait Channel: Send + Sync {
    async fn receive(&self) -> Result<Message>;
    async fn send(&self, message: Message) -> Result<()>;
}

/// 도구 (forja-tools에서 구현: Shell, File 조작 등).
///
/// # 설계 결정: args 타입
/// MCP 프로토콜이 JSON 기반이므로 args를 serde_json::Value로 받는다.
/// 구조화된 입력(파라미터 이름, 타입)을 자연스럽게 처리 가능.
#[async_trait]
pub trait Tool: Send + Sync {
    /// 도구의 고유 이름 (예: "shell", "file_read").
    fn name(&self) -> &str;

    /// 도구의 명세(JSON Schema 포함)를 반환.
    fn definition(&self) -> ToolDefinition;

    /// JSON 형태의 구조화된 인자로 도구 실행.
    async fn execute(&self, args: serde_json::Value) -> Result<serde_json::Value>;
}
