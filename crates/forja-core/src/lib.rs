#[cfg(feature = "runtime")]
pub mod background;
pub mod decision;
pub mod engine;
pub mod emotion;
pub mod events;
pub mod intent;
#[cfg(feature = "runtime")]
pub mod scheduler;
pub mod error;
pub mod knowledge;
pub mod mode;
pub mod notification;
pub mod prompt;
pub mod safety;
pub mod serendipity;
pub mod skill;
pub mod skill_eval;
pub mod skill_improve;
pub mod traits;
pub mod types;
#[cfg(feature = "runtime")]
pub mod watchers;

// Re-export core types.
pub use engine::Engine;
pub use error::{ForjaError, Result};
#[cfg(feature = "runtime")]
pub use background::BackgroundManager;
pub use knowledge::{KnowledgeManager, TopicEntry};
pub use serendipity::SerendipityEngine;
pub use traits::{Channel, LlmProvider, MemoryStore, Tool};
pub use types::{Content, MemoryEntry, Message, Role, ToolDefinition};

#[cfg(test)]
mod tests;
