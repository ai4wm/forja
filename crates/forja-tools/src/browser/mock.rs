use super::backend::BrowserBackend;
use async_trait::async_trait;
use std::sync::Mutex;

pub struct MockBrowserBackend {
    calls: Mutex<Vec<String>>,
}

impl MockBrowserBackend {
    pub fn new() -> Self {
        Self {
            calls: Mutex::new(Vec::new()),
        }
    }

    pub fn calls_snapshot(&self) -> Vec<String> {
        match self.calls.lock() {
            Ok(calls) => calls.clone(),
            Err(_) => Vec::new(),
        }
    }

    fn push_call(&self, call: String) -> std::result::Result<(), String> {
        let mut calls = self
            .calls
            .lock()
            .map_err(|_| "Mock browser backend lock was poisoned".to_string())?;
        calls.push(call);
        Ok(())
    }
}

impl Default for MockBrowserBackend {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl BrowserBackend for MockBrowserBackend {
    async fn open(&self, url: &str) -> std::result::Result<String, String> {
        self.push_call(format!("open:{url}"))?;
        Ok("opened".to_string())
    }

    async fn goto(&self, url: &str) -> std::result::Result<String, String> {
        self.push_call(format!("goto:{url}"))?;
        Ok("navigated".to_string())
    }

    async fn scroll(&self, direction: &str, amount: i32) -> std::result::Result<String, String> {
        self.push_call(format!("scroll:{direction}:{amount}"))?;
        Ok("scrolled".to_string())
    }

    async fn click(&self, selector: &str) -> std::result::Result<String, String> {
        self.push_call(format!("click:{selector}"))?;
        Ok("clicked".to_string())
    }

    async fn type_text(&self, selector: &str, text: &str) -> std::result::Result<String, String> {
        self.push_call(format!("type_text:{selector}:{text}"))?;
        Ok("typed".to_string())
    }

    async fn read_text(&self, selector: &str) -> std::result::Result<String, String> {
        self.push_call(format!("read_text:{selector}"))?;
        Ok("mock text".to_string())
    }

    async fn read_page(&self) -> std::result::Result<String, String> {
        self.push_call("read_page".to_string())?;
        Ok("mock page text".to_string())
    }

    async fn screenshot(&self) -> std::result::Result<String, String> {
        self.push_call("screenshot".to_string())?;
        Ok("bW9jay1zY3JlZW5zaG90".to_string())
    }

    async fn evaluate(&self, js: &str) -> std::result::Result<String, String> {
        self.push_call(format!("evaluate:{js}"))?;
        Ok("mock eval result".to_string())
    }

    async fn tab_list(&self) -> std::result::Result<String, String> {
        self.push_call("tab_list".to_string())?;
        Ok("[\"tab-0\",\"tab-1\"]".to_string())
    }

    async fn tab_switch(&self, index: usize) -> std::result::Result<String, String> {
        self.push_call(format!("tab_switch:{index}"))?;
        Ok("tab switched".to_string())
    }

    async fn tab_close(&self, index: usize) -> std::result::Result<String, String> {
        self.push_call(format!("tab_close:{index}"))?;
        Ok("tab closed".to_string())
    }

    async fn back(&self) -> std::result::Result<String, String> {
        self.push_call("back".to_string())?;
        Ok("went back".to_string())
    }

    async fn forward(&self) -> std::result::Result<String, String> {
        self.push_call("forward".to_string())?;
        Ok("went forward".to_string())
    }
}
