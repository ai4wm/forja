use super::DashboardState;
use axum::Json;
use axum::extract::State;
use axum::response::Sse;
use axum::response::sse::{Event, KeepAlive};
use serde::Deserialize;
use serde_json::{Value, json};
use std::convert::Infallible;
use std::pin::Pin;
use std::time::Duration;
use tokio_stream::StreamExt;
use tokio_stream::wrappers::BroadcastStream;

#[derive(Debug, Deserialize)]
pub(crate) struct ChatRequest {
    text: String,
}

pub(crate) async fn post_chat(
    State(state): State<DashboardState>,
    Json(request): Json<ChatRequest>,
) -> Json<Value> {
    let text = request.text.trim();
    if text.is_empty() {
        return Json(json!({
            "ok": false,
            "reason": "empty_message",
        }));
    }

    let Some(bridge) = &state.dashboard_bridge else {
        return Json(json!({
            "ok": false,
            "reason": "dashboard_bridge_unavailable",
        }));
    };

    match bridge.send_user_text(text.to_string()).await {
        Ok(()) => Json(json!({ "ok": true })),
        Err(error) => Json(json!({
            "ok": false,
            "reason": error.to_string(),
        })),
    }
}

pub(crate) async fn stream_chat(
    State(state): State<DashboardState>,
) -> Sse<Pin<Box<dyn tokio_stream::Stream<Item = Result<Event, Infallible>> + Send>>> {
    let stream: Pin<Box<dyn tokio_stream::Stream<Item = Result<Event, Infallible>> + Send>> =
        if let Some(bridge) = &state.dashboard_bridge {
            Box::pin(
                BroadcastStream::new(bridge.subscribe()).filter_map(|result| {
                    let event = match result {
                        Ok(event) => event,
                        Err(_) => return None,
                    };
                    let payload = serde_json::to_string(&event).unwrap_or_else(|_| {
                        "{\"kind\":\"error\",\"text\":\"serialize_failed\"}".to_string()
                    });
                    Some(Ok(Event::default().data(payload)))
                }),
            )
        } else {
            Box::pin(tokio_stream::iter([Ok(Event::default().data(
                "{\"kind\":\"error\",\"text\":\"dashboard_bridge_unavailable\"}",
            ))]))
        };

    Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(5))
            .text("keep-alive"),
    )
}
