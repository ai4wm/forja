use forja_core::traits::Tool;
use forja_core::types::ToolDefinition;
use async_trait::async_trait;
use reqwest::Client;
use serde_json::Value;
use std::time::Duration;

#[derive(Debug, Clone)]
pub enum SearchProvider {
    DuckDuckGo,
    Brave { api_key: String },
    Grok { api_key: String },
}

pub struct SearchTool {
    provider: SearchProvider,
    client: Client,
}

impl SearchTool {
    pub fn new(provider: SearchProvider) -> Self {
        let client = Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .build()
            .unwrap_or_default();
            
        Self { provider, client }
    }

    async fn search_duckduckgo(&self, query: &str) -> String {
        let url = format!("https://api.duckduckgo.com/?q={}&format=json&no_html=1", url::form_urlencoded::byte_serialize(query.as_bytes()).collect::<String>());
        
        match self.client.get(&url).send().await {
            Ok(res) => {
                if let Ok(text) = res.text().await {
                    if let Ok(json) = serde_json::from_str::<Value>(&text) {
                        let mut result = String::new();
                        
                        // Extract AbstractText
                        if let Some(abstract_text) = json.get("AbstractText").and_then(|v| v.as_str()) {
                            if !abstract_text.is_empty() {
                                result.push_str(&format!("Abstract:\n{}\n\n", abstract_text));
                            }
                        }

                        // Extract RelatedTopics
                        if let Some(topics) = json.get("RelatedTopics").and_then(|v| v.as_array()) {
                            for topic in topics {
                                if let Some(text) = topic.get("Text").and_then(|v| v.as_str()) {
                                    result.push_str(&format!("- {}\n", text));
                                }
                            }
                        }
                        
                        if result.is_empty() {
                            return "No relevant results found.".to_string();
                        }
                        
                        return Self::truncate(&result);
                    }
                }
                "Failed to parse search results.".to_string()
            }
            Err(e) => format!("Search request failed: {}", e)
        }
    }

    async fn search_brave(&self, query: &str, api_key: &str) -> String {
        let url = format!("https://api.search.brave.com/res/v1/web/search?q={}&count=5", url::form_urlencoded::byte_serialize(query.as_bytes()).collect::<String>());
        
        match self.client.get(&url).header("X-Subscription-Token", api_key).send().await {
            Ok(res) => {
                if let Ok(text) = res.text().await {
                    if let Ok(json) = serde_json::from_str::<Value>(&text) {
                        if let Some(results) = json.get("web").and_then(|w| w.get("results")).and_then(|w| w.as_array()) {
                            let mut out = String::new();
                            for item in results {
                                let title = item.get("title").and_then(|v| v.as_str()).unwrap_or("No title");
                                let url = item.get("url").and_then(|v| v.as_str()).unwrap_or("");
                                let desc = item.get("description").and_then(|v| v.as_str()).unwrap_or("");
                                out.push_str(&format!("Title: {}\nURL: {}\nDesc: {}\n\n", title, url, desc));
                            }
                            if out.is_empty() {
                                return "No results found.".to_string();
                            }
                            return Self::truncate(&out);
                        }
                    }
                }
                "Failed to parse search results.".to_string()
            }
            Err(e) => format!("Search request failed: {}", e)
        }
    }

    async fn search_grok(&self, query: &str, api_key: &str) -> String {
        // Responses API 엔드포인트
        let url = "https://api.x.ai/v1/responses";
        
        let body = serde_json::json!({
            "model": "grok-4-1-fast",
            "input": [
                { "role": "user", "content": query }
            ],
            "tools": [
                {
                    "type": "web_search"
                }
            ]
        });

        match self.client.post(url).header("Authorization", format!("Bearer {}", api_key)).json(&body).send().await {
            Ok(res) => {
                let text = res.text().await.unwrap_or_default();
                
                if let Ok(json) = serde_json::from_str::<Value>(&text) {
                    if let Some(output) = json.get("output").and_then(|v| v.as_array()) {
                        for item in output {
                            if item.get("type").and_then(|v| v.as_str()) == Some("message") {
                                if let Some(content) = item.get("content").and_then(|v| v.as_array()) {
                                    for c in content {
                                        if let Some(text) = c.get("text").and_then(|v| v.as_str()) {
                                            return Self::truncate(text);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                Self::truncate(&text)
            }
            Err(e) => format!("Search request failed: {}", e)
        }
    }

    fn truncate(text: &str) -> String {
        let max_chars = 50_000;
        if text.chars().count() > max_chars {
            let cut: String = text.chars().take(max_chars).collect();
            format!("{}...\n[Truncated]", cut)
        } else {
            text.to_string()
        }
    }
}

#[async_trait]
impl Tool for SearchTool {
    fn name(&self) -> &str {
        "search_tool"
    }

    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: self.name().to_string(),
            description: "Search the web for real-time information. Useful for up-to-date facts.".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "The search query."
                    }
                },
                "required": ["query"]
            }),
        }
    }

    async fn execute(&self, arguments: serde_json::Value) -> Result<serde_json::Value, forja_core::error::ForjaError> {
        let query = arguments.get("query").and_then(|v| v.as_str())
            .ok_or_else(|| forja_core::error::ForjaError::ToolError("Missing 'query' parameter.".to_string()))?;

        let result = match &self.provider {
            SearchProvider::DuckDuckGo => self.search_duckduckgo(query).await,
            SearchProvider::Brave { api_key } => self.search_brave(query, api_key).await,
            SearchProvider::Grok { api_key } => self.search_grok(query, api_key).await,
        };

        Ok(serde_json::json!(result))
    }
}
