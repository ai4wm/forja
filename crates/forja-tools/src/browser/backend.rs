use async_trait::async_trait;

#[async_trait]
pub trait BrowserBackend: Send + Sync + 'static {
    async fn open(&self, url: &str) -> std::result::Result<String, String>;
    async fn goto(&self, url: &str) -> std::result::Result<String, String>;
    async fn scroll(&self, direction: &str, amount: i32) -> std::result::Result<String, String>;
    async fn click(&self, selector: &str) -> std::result::Result<String, String>;
    async fn type_text(&self, selector: &str, text: &str) -> std::result::Result<String, String>;
    async fn read_text(&self, selector: &str) -> std::result::Result<String, String>;
    async fn read_page(&self) -> std::result::Result<String, String>;
    async fn screenshot(&self) -> std::result::Result<String, String>;
    async fn evaluate(&self, js: &str) -> std::result::Result<String, String>;
    async fn tab_list(&self) -> std::result::Result<String, String>;
    async fn tab_switch(&self, index: usize) -> std::result::Result<String, String>;
    async fn tab_close(&self, index: usize) -> std::result::Result<String, String>;
    async fn back(&self) -> std::result::Result<String, String>;
    async fn forward(&self) -> std::result::Result<String, String>;
}
