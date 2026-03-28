pub mod client;
pub mod config;
pub mod local;
pub mod models;
pub mod presets;

pub use client::LlmClient;
pub use config::LlmConfig;
pub use local::{LocalModelInfo, LocalModelProvider, detect_local_models, ensure_models_dir};
