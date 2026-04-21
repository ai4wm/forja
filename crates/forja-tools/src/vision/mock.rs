use super::analyzer::VisionAnalyzer;
use super::capture::{transparent_png, ScreenCaptureBackend};
use async_trait::async_trait;
use std::sync::Mutex;

pub struct MockCaptureBackend {
    calls: Mutex<Vec<String>>,
}

impl MockCaptureBackend {
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
            .map_err(|_| "Mock capture backend lock was poisoned".to_string())?;
        calls.push(call);
        Ok(())
    }
}

impl Default for MockCaptureBackend {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ScreenCaptureBackend for MockCaptureBackend {
    async fn capture_full(&self) -> std::result::Result<Vec<u8>, String> {
        self.push_call("capture_full".to_string())?;
        transparent_png()
    }

    async fn capture_region(
        &self,
        x: i32,
        y: i32,
        width: u32,
        height: u32,
    ) -> std::result::Result<Vec<u8>, String> {
        self.push_call(format!("capture_region:{x},{y},{width},{height}"))?;
        transparent_png()
    }
}

pub struct MockVisionAnalyzer {
    calls: Mutex<Vec<String>>,
    response: String,
}

impl MockVisionAnalyzer {
    pub fn new() -> Self {
        Self {
            calls: Mutex::new(Vec::new()),
            response: "Mock vision response".to_string(),
        }
    }

    pub fn with_response(response: impl Into<String>) -> Self {
        Self {
            calls: Mutex::new(Vec::new()),
            response: response.into(),
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
            .map_err(|_| "Mock vision analyzer lock was poisoned".to_string())?;
        calls.push(call);
        Ok(())
    }
}

impl Default for MockVisionAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl VisionAnalyzer for MockVisionAnalyzer {
    async fn analyze_image(
        &self,
        _image_base64: &str,
        prompt: &str,
    ) -> std::result::Result<String, String> {
        self.push_call(format!("analyze:{prompt}"))?;
        if self.response != "Mock vision response" {
            return Ok(self.response.clone());
        }
        if prompt.starts_with("Find the UI element matching: ") {
            return Ok("{\"x\": 100, \"y\": 200, \"width\": 50, \"height\": 30}".to_string());
        }

        Ok(self.response.clone())
    }
}
