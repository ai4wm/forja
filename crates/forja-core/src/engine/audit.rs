use super::Engine;
use crate::audit::logger::{AuditEvent, AuditLogger};
use crate::autonomy::AutonomyAction;
use serde_json::Value;
use std::sync::Arc;

impl Engine {
    pub fn with_audit_logger(mut self, audit_logger: Arc<AuditLogger>) -> Self {
        self.audit_logger = Some(audit_logger);
        self
    }

    pub(super) fn log_llm_call(&self, mode: &str, token_count: usize) {
        self.log_audit_event(
            "llm_call",
            serde_json::json!({
                "mode": mode,
            }),
            token_count,
        );
    }

    pub(super) fn log_tool_call(&self, tool_name: &str, arguments: &Value) {
        self.log_audit_event(
            "tool_call",
            serde_json::json!({
                "tool_name": tool_name,
                "arguments": arguments,
            }),
            0,
        );
    }

    pub(super) fn log_tool_result(&self, call_id: &str, result: &Value) {
        self.log_audit_event(
            "tool_result",
            serde_json::json!({
                "call_id": call_id,
                "result": result,
            }),
            0,
        );
    }

    pub(super) fn log_compression_event(&self, kind: &str, before_tokens: usize, after_tokens: usize) {
        self.log_audit_event(
            "compression",
            serde_json::json!({
                "kind": kind,
                "before_tokens": before_tokens,
                "after_tokens": after_tokens,
            }),
            after_tokens,
        );
    }

    pub(super) fn log_engine_error(&self, operation: &str, error_text: &str) {
        self.log_audit_event(
            "error",
            serde_json::json!({
                "operation": operation,
                "error": error_text,
            }),
            0,
        );
    }

    pub(super) fn log_budget_event(&self, event_type: &str, used: usize, limit: usize) {
        let percent = if limit == 0 { 0 } else { used * 100 / limit };
        self.log_audit_event(
            event_type,
            serde_json::json!({
                "agent_id": self.current_agent_id.clone(),
                "used": used,
                "limit": limit,
                "percent": percent,
            }),
            used,
        );
    }

    pub(super) fn log_autonomy_action(&self, action: &AutonomyAction) {
        self.log_audit_event(
            "autonomy_action",
            serde_json::json!({
                "action": format!("{action:?}"),
            }),
            0,
        );
    }

    pub(super) fn log_autonomy_note(&self, event_type: &str, tool_name: &str) {
        self.log_audit_event(
            event_type,
            serde_json::json!({
                "tool_name": tool_name,
            }),
            0,
        );
    }

    fn log_audit_event(&self, event_type: &str, payload: Value, token_count: usize) {
        let Some(audit_logger) = &self.audit_logger else {
            return;
        };

        let mut event = AuditEvent::new(event_type, payload)
            .with_agent_id(self.current_agent_id.clone())
            .with_token_count(token_count);
        if let Some(channel) = self.active_channel_name() {
            event = event.with_channel(channel);
        }

        let _ = audit_logger.log_event(event);
    }

    fn active_channel_name(&self) -> Option<&'static str> {
        if self.channel.is_cli_source() {
            Some("cli")
        } else {
            Some("telegram")
        }
    }
}
