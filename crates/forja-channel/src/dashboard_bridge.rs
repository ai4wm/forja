use forja_core::error::{ForjaError, Result};
use serde::Serialize;
use tokio::sync::{broadcast, mpsc};

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DashboardEvent {
    UserMessage { text: String },
    AssistantChunk { text: String },
    AssistantMessage { text: String },
    Error { text: String },
}

#[derive(Clone)]
pub struct DashboardBridge {
    input_tx: mpsc::Sender<String>,
    event_tx: broadcast::Sender<DashboardEvent>,
}

impl DashboardBridge {
    pub fn new(
        input_tx: mpsc::Sender<String>,
        event_tx: broadcast::Sender<DashboardEvent>,
    ) -> Self {
        Self { input_tx, event_tx }
    }

    pub async fn send_user_text(&self, text: impl Into<String>) -> Result<()> {
        let text = text.into();
        self.input_tx
            .send(text.clone())
            .await
            .map_err(|_| ForjaError::ChannelError("Dashboard input channel closed".to_string()))?;
        let _ = self.event_tx.send(DashboardEvent::UserMessage { text });
        Ok(())
    }

    pub fn subscribe(&self) -> broadcast::Receiver<DashboardEvent> {
        self.event_tx.subscribe()
    }

    pub(crate) fn emit_assistant_chunk(&self, text: impl Into<String>) {
        let _ = self
            .event_tx
            .send(DashboardEvent::AssistantChunk { text: text.into() });
    }

    pub(crate) fn emit_assistant_message(&self, text: impl Into<String>) {
        let _ = self
            .event_tx
            .send(DashboardEvent::AssistantMessage { text: text.into() });
    }

    pub fn emit_error(&self, text: impl Into<String>) {
        let _ = self
            .event_tx
            .send(DashboardEvent::Error { text: text.into() });
    }
}
