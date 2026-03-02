# Forja 아키텍처

## 레이어 구조
```
[Channel Layer]  CLI / Telegram / Discord
       ↕ Message
[Core Layer]     EventBus → Domain Router
       ↕ Message
[Service Layer]  LLM / Memory / Tools
       ↕
[Storage Layer]  SQLite / File System
```

## Core Traits
```rust
pub trait LlmProvider: Send + Sync {
    async fn chat(&self, messages: Vec<Message>) -> Result<String>;
    async fn stream(&self, messages: Vec<Message>) -> Result<Stream>;
}

pub trait MemoryStore: Send + Sync {
    async fn save(&self, key: &str, content: &str) -> Result<()>;
    async fn search(&self, query: &str, limit: usize) -> Result<Vec<Entry>>;
}

pub trait Channel: Send + Sync {
    async fn receive(&self) -> Result<Message>;
    async fn send(&self, message: Message) -> Result<()>;
}
```

## 데이터 흐름
User Input → Channel → Engine → LLM Provider → Response
                                → Memory (저장/검색)
                                → Tools (실행)
