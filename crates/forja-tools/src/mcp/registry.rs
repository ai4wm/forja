use crate::confirm::ConfirmationHandler;
use crate::{FileTool, SearchProvider, SearchTool, ShellTool, WebTool};
use async_trait::async_trait;
use forja_core::traits::Tool;
use std::collections::HashMap;
use std::sync::Arc;

pub type ToolRegistry = HashMap<String, Arc<dyn Tool>>;

pub fn build_default_registry() -> ToolRegistry {
    let mut registry: ToolRegistry = HashMap::new();
    registry.insert("file_tool".to_string(), Arc::new(FileTool::new()));
    registry.insert("web_tool".to_string(), Arc::new(WebTool::new()));
    registry.insert(
        "search_tool".to_string(),
        Arc::new(SearchTool::new(search_provider_from_env())),
    );
    registry.insert(
        "shell".to_string(),
        Arc::new(ShellTool::new(Arc::new(McpConfirmation::from_env()))),
    );
    registry
}

fn search_provider_from_env() -> SearchProvider {
    match std::env::var("FORJA_MCP_SEARCH_PROVIDER")
        .unwrap_or_else(|_| "duckduckgo".to_string())
        .to_ascii_lowercase()
        .as_str()
    {
        "brave" => SearchProvider::Brave {
            api_key: std::env::var("BRAVE_API_KEY").unwrap_or_default(),
        },
        "grok" | "xai" => SearchProvider::Grok {
            api_key: std::env::var("XAI_API_KEY").unwrap_or_default(),
        },
        _ => SearchProvider::DuckDuckGo,
    }
}

struct McpConfirmation {
    allow_dangerous: bool,
}

impl McpConfirmation {
    fn from_env() -> Self {
        Self {
            allow_dangerous: matches!(
                std::env::var("FORJA_MCP_ALLOW_DANGEROUS"),
                Ok(value)
                    if matches!(
                        value.trim().to_ascii_lowercase().as_str(),
                        "1" | "true" | "yes" | "on"
                    )
            ),
        }
    }
}

#[async_trait]
impl ConfirmationHandler for McpConfirmation {
    async fn confirm(&self, _cmd: &str, dangerous: bool) -> bool {
        !dangerous || self.allow_dangerous
    }
}
