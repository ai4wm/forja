use super::types::{DebateMessage, DebatePhase};
use super::{CreationRunContext, DebateAgent};
use crate::audit::logger::{AuditEvent, AuditLogger};
use crate::budget::{BudgetMode, BudgetStatus};
use crate::context::token_counter::count_tokens;
use crate::error::{ForjaError, Result};
use crate::ralf::RalfState;
use crate::ralf::executor::ralf_execute;
use crate::traits::LlmProvider;
use crate::types::{Content, Message, Role};
use serde_json::json;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use tokio::time::{Duration, sleep, timeout};

pub(crate) type DebateMessageCallback =
    dyn FnMut(&DebateMessage) -> Pin<Box<dyn Future<Output = Result<()>> + Send>>;

#[derive(Debug, Clone, Copy)]
pub(crate) struct DebateCallContext {
    pub phase: DebatePhase,
    pub round: usize,
    pub should_delay: bool,
}

pub(crate) async fn execute_agent_call(
    provider: &Arc<dyn LlmProvider>,
    audit_logger: Option<&AuditLogger>,
    agent: &DebateAgent,
    call_context: DebateCallContext,
    prompt: String,
    on_message: &mut Option<&mut DebateMessageCallback>,
    run_context: Option<&CreationRunContext>,
) -> Result<DebateMessage> {
    if call_context.should_delay {
        sleep(run_context.map_or(Duration::from_secs(2), |context| context.inter_call_delay)).await;
    }

    let scoped_agent_id = format!("creation/{}", agent.id);
    ensure_budget_ready(run_context, agent, &scoped_agent_id)?;
    check_budget_before_call(run_context, &scoped_agent_id)?;

    let request_messages = [
        Message::text(
            Role::System,
            format!(
                "You are {}. Your framework: {}",
                agent.role, agent.framework
            ),
            None,
        ),
        Message::text(Role::User, prompt, None),
    ];

    let content = match call_provider(provider, audit_logger, &request_messages, run_context).await
    {
        Ok(content) => content,
        Err(error) if error.to_string().to_lowercase().contains("timeout") => {
            "[timeout] No response within 60s".to_string()
        }
        Err(error) => return Err(bound_creation_error(error, run_context)),
    };

    let tokens = count_tokens(&content, "cl100k_base");
    record_budget_after_call(run_context, &scoped_agent_id, tokens)?;

    let message = DebateMessage {
        agent_id: agent.id.clone(),
        role: agent.role.clone(),
        phase: call_context.phase,
        round: call_context.round,
        tokens,
        content,
    };

    log_debate_message(audit_logger, &message, run_context);

    if let Some(callback) = on_message.as_deref_mut() {
        callback(&message).await?;
    }

    Ok(message)
}

async fn call_provider(
    provider: &Arc<dyn LlmProvider>,
    audit_logger: Option<&AuditLogger>,
    request_messages: &[Message],
    run_context: Option<&CreationRunContext>,
) -> Result<String> {
    if let Some(run_context) = run_context {
        let mut ralf_state = RalfState::default();
        let response = ralf_execute(
            "creation_agent_call",
            &run_context.ralf_config,
            &mut ralf_state,
            audit_logger,
            || {
                let provider = provider.clone();
                let request_messages = request_messages.to_vec();
                async move {
                    match timeout(
                        Duration::from_secs(60),
                        provider.chat(&request_messages, None),
                    )
                    .await
                    {
                        Ok(result) => result,
                        Err(_) => Err(ForjaError::LlmError("creation agent timeout".to_string())),
                    }
                }
            },
        )
        .await?;
        return extract_text_response(response);
    }

    match timeout(
        Duration::from_secs(60),
        provider.chat(request_messages, None),
    )
    .await
    {
        Ok(response) => extract_text_response(response?),
        Err(_) => Err(ForjaError::LlmError("creation agent timeout".to_string())),
    }
}

fn extract_text_response(response: Message) -> Result<String> {
    match response.content {
        Content::Text { text, .. } => Ok(text),
        _ => Err(ForjaError::LlmError(
            "creation response was not text".to_string(),
        )),
    }
}

fn ensure_budget_ready(
    run_context: Option<&CreationRunContext>,
    agent: &DebateAgent,
    scoped_agent_id: &str,
) -> Result<()> {
    let Some(run_context) = run_context else {
        return Ok(());
    };
    let Some(budget_manager) = &run_context.budget_manager else {
        return Ok(());
    };

    if budget_manager.check_budget(scoped_agent_id).is_ok() {
        return Ok(());
    }
    budget_manager.register_agent(scoped_agent_id, agent.budget)
}

fn check_budget_before_call(
    run_context: Option<&CreationRunContext>,
    scoped_agent_id: &str,
) -> Result<()> {
    let Some(run_context) = run_context else {
        return Ok(());
    };
    let Some(budget_manager) = &run_context.budget_manager else {
        return Ok(());
    };

    match budget_manager.check_budget(scoped_agent_id)? {
        BudgetStatus::Exceeded { .. } if run_context.budget_mode == BudgetMode::Enforce => {
            Err(ForjaError::LlmError(format!(
                "creation agent budget exceeded for {scoped_agent_id}"
            )))
        }
        _ => Ok(()),
    }
}

fn record_budget_after_call(
    run_context: Option<&CreationRunContext>,
    scoped_agent_id: &str,
    tokens: usize,
) -> Result<()> {
    let Some(run_context) = run_context else {
        return Ok(());
    };
    let Some(budget_manager) = &run_context.budget_manager else {
        return Ok(());
    };

    match budget_manager.record_usage(scoped_agent_id, tokens)? {
        BudgetStatus::Exceeded { .. } if run_context.budget_mode == BudgetMode::Enforce => {
            Err(ForjaError::LlmError(format!(
                "creation agent budget exceeded for {scoped_agent_id}"
            )))
        }
        _ => Ok(()),
    }
}

pub(crate) fn log_debate_message(
    audit_logger: Option<&AuditLogger>,
    message: &DebateMessage,
    run_context: Option<&CreationRunContext>,
) {
    let Some(audit_logger) = audit_logger else {
        return;
    };

    let sanitized = sanitize_audit_text(
        &message.content,
        run_context.map_or(512, |context| context.max_logged_chars),
    );
    let event = AuditEvent::new(
        "debate_message",
        json!({
            "role": message.role,
            "phase": message.phase.label(),
            "round": message.round,
            "content": sanitized,
        }),
    )
    .with_agent_id(message.agent_id.clone())
    .with_token_count(message.tokens);
    let _ = audit_logger.log_event(event);
}

pub(crate) fn log_debate_timeout(
    audit_logger: Option<&AuditLogger>,
    agent: &DebateAgent,
    call_context: DebateCallContext,
) {
    let Some(audit_logger) = audit_logger else {
        return;
    };

    let event = AuditEvent::new(
        "debate_timeout",
        json!({
            "agent_id": agent.id.clone(),
            "phase": call_context.phase.label(),
            "round": call_context.round,
        }),
    )
    .with_agent_id(agent.id.clone());
    let _ = audit_logger.log_event(event);
}

pub(crate) fn sanitize_audit_text(text: &str, max_logged_chars: usize) -> String {
    let collapsed = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let redacted = collapsed
        .split_whitespace()
        .map(redact_word)
        .collect::<Vec<_>>()
        .join(" ");
    truncate_chars(&redacted, max_logged_chars)
}

fn redact_word(word: &str) -> String {
    let trimmed =
        word.trim_matches(|char: char| matches!(char, ',' | '.' | ';' | ':' | '"' | '\''));
    if trimmed.starts_with("sk-")
        || trimmed.starts_with("AIza")
        || trimmed.starts_with("ghp_")
        || trimmed.starts_with("Bearer")
    {
        return "[REDACTED]".to_string();
    }
    word.to_string()
}

fn truncate_chars(value: &str, max_chars: usize) -> String {
    if value.chars().count() <= max_chars {
        return value.to_string();
    }
    let mut truncated = value
        .chars()
        .take(max_chars.saturating_sub(1))
        .collect::<String>();
    truncated.push('…');
    truncated
}

fn bound_creation_error(error: ForjaError, run_context: Option<&CreationRunContext>) -> ForjaError {
    let max_chars = run_context.map_or(512, |context| context.max_logged_chars);
    let sanitized = sanitize_audit_text(&error.to_string(), max_chars);
    match error {
        ForjaError::LlmError(_) => ForjaError::LlmError(sanitized),
        _ => ForjaError::Internal(sanitized),
    }
}
