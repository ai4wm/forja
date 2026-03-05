# Forja 아키텍처

## 크레이트 의존성 구조 (Workspace)
```
forja (CLI 바이너리 & 설정 관리)
  ├─ forja-core    (엔진 런타임, 공통 Trait, 도구 명세, 타입)
  ├─ forja-llm     (다중 LLM API 라우팅 및 직렬화)
  ├─ forja-memory  (BM25 기반 마크다운 메모리 자동 저장/요약)
  ├─ forja-tools   (File, Web, Shell & 명령어 화이트리스트 자동 승인)
  └─ forja-channel (CLI, Telegram 등 입출력 인터페이스)
```

## 레이어 추상화
```
[Channel Layer]  (forja-channel) CLI 
       ↕ Message
[Core Layer]     (forja-core) Engine 내부 처리 로직 & EventBus
       ↕ Message
[Service Layer]  LLM(forja-llm), Memory(forja-memory), Tools(forja-tools)
       ↕
[Storage Layer]  Markdown Directory (~/.forja/memory)
```
## Core Traits

### LlmProvider
```rust
#[async_trait]
pub trait LlmProvider: Send + Sync {
    // 단일 응답
    async fn chat(&self, messages: &[Message], tools: Option<&[ToolDefinition]>) -> Result<Message>;
    // 토큰 단위 스트리밍
    async fn stream(&self, messages: &[Message], tools: Option<&[ToolDefinition]>) 
        -> Result<Pin<Box<dyn Stream<Item = Result<String>> + Send>>>;
}
```

### MemoryStore
```rust
#[async_trait]
pub trait MemoryStore: Send + Sync {
    async fn save(&self, entry: &MemoryEntry) -> Result<()>;
    async fn search(&self, query: &str, limit: usize) -> Result<Vec<MemoryEntry>>;
    async fn flush(&self) -> Result<()>; // 영구 저장소(파일)로 동기화
}
```

### Tool
```rust
#[async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &str;
    fn definition(&self) -> ToolDefinition;
    async fn execute(&self, args: serde_json::Value) -> Result<serde_json::Value>;
}
```

## 데이터 흐름
User Input → Channel → Engine → LLM Provider → Response
                                → Memory (저장/검색)
                                → Tools (실행)
