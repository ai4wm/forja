pub mod audit;
pub mod budget;
pub mod creation;
pub mod context;
pub mod engine;
pub mod emotion;
#[cfg(feature = "runtime")]
pub mod scheduler;
pub mod error;
pub mod gateway;
pub mod heartbeat;
pub mod knowledge;
pub mod mode;
pub mod prompt;
pub mod ralf;
pub mod serendipity;
pub mod traits;
pub mod types;

// Re-export core types.
pub use engine::Engine;
pub use error::{ForjaError, Result};
pub use knowledge::{KnowledgeManager, TopicEntry};
pub use serendipity::SerendipityEngine;
pub use traits::{Channel, LlmProvider, MemoryStore, Tool};
pub use types::{Content, MemoryEntry, Message, Role, ToolDefinition};

#[cfg(test)]
mod tests;
