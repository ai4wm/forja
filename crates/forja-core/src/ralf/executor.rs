use super::{RalfConfig, RalfState};
use crate::audit::logger::{AuditEvent, AuditLogger};
use crate::error::{ForjaError, Result};
use serde_json::json;
use std::future::Future;

pub async fn ralf_execute<T, Op, Fut>(
    operation: &str,
    config: &RalfConfig,
    state: &mut RalfState,
    audit_logger: Option<&AuditLogger>,
    mut work: Op,
) -> Result<T>
where
    Op: FnMut() -> Fut,
    Fut: Future<Output = Result<T>>,
{
    loop {
        match work().await {
            Ok(value) => return Ok(value),
            Err(error) => {
                state.retry_count = state.retry_count.saturating_add(1);
                let error_text = error.to_string();
                state.error_history.push(error_text.clone());

                log_retry(audit_logger, operation, state.retry_count, &error_text);

                let repeated = trailing_identical_errors(&state.error_history, &error_text);
                let exhausted = state.retry_count > config.max_retries;
                let repeated_too_many = repeated >= config.max_identical_errors;

                if exhausted || repeated_too_many {
                    log_error(audit_logger, operation, state.retry_count, &error_text);
                    return Err(wrap_error(error, operation, state.retry_count));
                }
            }
        }
    }
}

fn trailing_identical_errors(history: &[String], target: &str) -> usize {
    let mut count = 0;
    for error in history.iter().rev() {
        if error == target {
            count += 1;
        } else {
            break;
        }
    }
    count
}

fn log_retry(
    audit_logger: Option<&AuditLogger>,
    operation: &str,
    retry_count: usize,
    error_text: &str,
) {
    let Some(audit_logger) = audit_logger else {
        return;
    };

    let _ = audit_logger.log_event(
        AuditEvent::new(
            "retry",
            json!({
                "operation": operation,
                "retry_count": retry_count,
                "error": error_text,
            }),
        ),
    );
}

fn log_error(
    audit_logger: Option<&AuditLogger>,
    operation: &str,
    retry_count: usize,
    error_text: &str,
) {
    let Some(audit_logger) = audit_logger else {
        return;
    };

    let _ = audit_logger.log_event(
        AuditEvent::new(
            "error",
            json!({
                "operation": operation,
                "retry_count": retry_count,
                "error": error_text,
            }),
        ),
    );
}

fn wrap_error(error: ForjaError, operation: &str, retry_count: usize) -> ForjaError {
    match error {
        ForjaError::LlmError(message) => ForjaError::LlmError(format!(
            "{operation} failed after {retry_count} attempts: {message}"
        )),
        ForjaError::ToolError(message) => ForjaError::ToolError(format!(
            "{operation} failed after {retry_count} attempts: {message}"
        )),
        other => ForjaError::Internal(format!(
            "{operation} failed after {retry_count} attempts: {other}"
        )),
    }
}
