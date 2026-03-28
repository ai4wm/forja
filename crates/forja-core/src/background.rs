use crate::traits::LlmProvider;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;
use tokio::time::MissedTickBehavior;

pub struct BackgroundManager {
    provider: Option<Arc<dyn LlmProvider>>,
    provider_name: Option<String>,
    model_name: Option<String>,
    interval: Duration,
    enabled: bool,
    active: Arc<AtomicBool>,
    shutdown_tx: Option<oneshot::Sender<()>>,
    task: Option<JoinHandle<()>>,
}

impl BackgroundManager {
    pub fn new(interval_seconds: u64) -> Self {
        Self {
            provider: None,
            provider_name: None,
            model_name: None,
            interval: Duration::from_secs(interval_seconds.max(1)),
            enabled: false,
            active: Arc::new(AtomicBool::new(false)),
            shutdown_tx: None,
            task: None,
        }
    }

    pub fn configure(
        &mut self,
        provider_name: String,
        model_name: String,
        provider: Arc<dyn LlmProvider>,
        interval_seconds: u64,
    ) {
        self.provider = Some(provider);
        self.provider_name = Some(provider_name);
        self.model_name = Some(model_name);
        self.interval = Duration::from_secs(interval_seconds.max(1));
        self.enabled = true;
    }

    pub fn disable(&mut self) {
        self.provider = None;
        self.provider_name = None;
        self.model_name = None;
        self.enabled = false;
    }

    pub fn start(&mut self) {
        if !self.enabled || self.provider.is_none() || self.is_active() {
            return;
        }

        let interval = self.interval;
        let active = self.active.clone();
        let (shutdown_tx, mut shutdown_rx) = oneshot::channel::<()>();
        self.shutdown_tx = Some(shutdown_tx);
        active.store(true, Ordering::SeqCst);

        self.task = Some(tokio::spawn(async move {
            let mut ticker = tokio::time::interval(interval);
            ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);

            loop {
                tokio::select! {
                    _ = ticker.tick() => {
                        println!("Background monitor tick");
                    }
                    _ = &mut shutdown_rx => {
                        active.store(false, Ordering::SeqCst);
                        break;
                    }
                }
            }
        }));
    }

    pub async fn stop(&mut self) {
        if let Some(shutdown_tx) = self.shutdown_tx.take() {
            let _ = shutdown_tx.send(());
        }

        if let Some(task) = self.task.take() {
            let _ = task.await;
        }

        self.active.store(false, Ordering::SeqCst);
    }

    pub fn is_active(&self) -> bool {
        self.active.load(Ordering::SeqCst)
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled && self.provider.is_some()
    }

    pub fn get_provider(&self) -> Option<Arc<dyn LlmProvider>> {
        self.provider.clone()
    }

    pub fn provider_name(&self) -> Option<&str> {
        self.provider_name.as_deref()
    }

    pub fn model_name(&self) -> Option<&str> {
        self.model_name.as_deref()
    }

    pub fn interval_seconds(&self) -> u64 {
        self.interval.as_secs()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::{ForjaError, Result};
    use crate::types::{Message, ToolDefinition};
    use async_trait::async_trait;
    use std::pin::Pin;
    use tokio_stream::Stream;

    struct DummyProvider;

    #[async_trait]
    impl LlmProvider for DummyProvider {
        async fn chat(
            &self,
            _messages: &[Message],
            _tools: Option<&[ToolDefinition]>,
        ) -> Result<Message> {
            Err(ForjaError::LlmError("not implemented".to_string()))
        }

        async fn stream(
            &self,
            _messages: &[Message],
            _tools: Option<&[ToolDefinition]>,
        ) -> Result<Pin<Box<dyn Stream<Item = Result<String>> + Send>>> {
            Err(ForjaError::LlmError("not implemented".to_string()))
        }
    }

    #[tokio::test]
    async fn background_manager_starts_and_stops() {
        let provider: Arc<dyn LlmProvider> = Arc::new(DummyProvider);
        let mut manager = BackgroundManager::new(1);
        manager.configure(
            "groq".to_string(),
            "llama-3.1-8b-instant".to_string(),
            provider,
            1,
        );

        manager.start();
        tokio::time::sleep(Duration::from_millis(20)).await;

        assert!(manager.is_active());
        assert!(manager.is_enabled());

        manager.stop().await;

        assert!(!manager.is_active());
    }

    #[test]
    fn background_manager_returns_provider_and_metadata() {
        let provider: Arc<dyn LlmProvider> = Arc::new(DummyProvider);
        let mut manager = BackgroundManager::new(30);
        manager.configure(
            "openrouter".to_string(),
            "meta-llama/llama-3.1-8b-instruct:free".to_string(),
            provider.clone(),
            30,
        );

        assert!(manager.get_provider().is_some());
        assert_eq!(manager.provider_name(), Some("openrouter"));
        assert_eq!(
            manager.model_name(),
            Some("meta-llama/llama-3.1-8b-instruct:free")
        );
        assert_eq!(manager.interval_seconds(), 30);
    }
}
