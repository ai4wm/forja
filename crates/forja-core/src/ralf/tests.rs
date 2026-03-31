use super::executor::ralf_execute;
use super::{RalfConfig, RalfState};
use crate::error::ForjaError;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};

#[test]
fn test_ralf_succeeds_on_first_try_without_retry() {
    let attempts = Arc::new(AtomicUsize::new(0));
    let attempts_clone = attempts.clone();
    let mut state = RalfState::default();

    let result = block_on_ready(ralf_execute(
        "llm_call",
        &RalfConfig::default(),
        &mut state,
        None,
        move || {
            let attempts_clone = attempts_clone.clone();
            Box::pin(async move {
                attempts_clone.fetch_add(1, Ordering::SeqCst);
                Ok::<_, ForjaError>("ok".to_string())
            })
        },
    ))
    .expect("first attempt should succeed");

    assert_eq!(result, "ok");
    assert_eq!(attempts.load(Ordering::SeqCst), 1);
    assert_eq!(state.retry_count, 0);
}

#[test]
fn test_ralf_retries_then_succeeds() {
    let attempts = Arc::new(AtomicUsize::new(0));
    let attempts_clone = attempts.clone();
    let mut state = RalfState::default();

    let result = block_on_ready(ralf_execute(
        "tool_call",
        &RalfConfig::default(),
        &mut state,
        None,
        move || {
            let attempts_clone = attempts_clone.clone();
            Box::pin(async move {
                let current = attempts_clone.fetch_add(1, Ordering::SeqCst);
                if current == 0 {
                    Err(ForjaError::ToolError("transient tool failure".to_string()))
                } else {
                    Ok::<_, ForjaError>(serde_json::json!({ "ok": true }))
                }
            })
        },
    ))
    .expect("second attempt should succeed");

    assert_eq!(result, serde_json::json!({ "ok": true }));
    assert_eq!(attempts.load(Ordering::SeqCst), 2);
    assert_eq!(state.retry_count, 1);
}

#[test]
fn test_ralf_stops_early_after_identical_error_threshold() {
    let attempts = Arc::new(AtomicUsize::new(0));
    let attempts_clone = attempts.clone();
    let mut state = RalfState::default();
    let config = RalfConfig {
        max_retries: 5,
        max_identical_errors: 3,
    };

    let error = block_on_ready(ralf_execute(
        "llm_call",
        &config,
        &mut state,
        None,
        move || {
            let attempts_clone = attempts_clone.clone();
            Box::pin(async move {
                attempts_clone.fetch_add(1, Ordering::SeqCst);
                Err::<String, _>(ForjaError::LlmError("same error".to_string()))
            })
        },
    ))
    .expect_err("identical errors should stop early");

    assert!(error.to_string().contains("same error"));
    assert_eq!(attempts.load(Ordering::SeqCst), 3);
    assert_eq!(state.retry_count, 3);
}

#[test]
fn test_ralf_returns_error_when_max_retries_exhausted() {
    let attempts = Arc::new(AtomicUsize::new(0));
    let attempts_clone = attempts.clone();
    let mut state = RalfState::default();
    let config = RalfConfig {
        max_retries: 2,
        max_identical_errors: 10,
    };

    let error = block_on_ready(ralf_execute(
        "tool_call",
        &config,
        &mut state,
        None,
        move || {
            let attempts_clone = attempts_clone.clone();
            Box::pin(async move {
                attempts_clone.fetch_add(1, Ordering::SeqCst);
                Err::<serde_json::Value, _>(ForjaError::ToolError("persistent failure".to_string()))
            })
        },
    ))
    .expect_err("max retries exhaustion should return error");

    assert!(error.to_string().contains("persistent failure"));
    assert_eq!(attempts.load(Ordering::SeqCst), 3);
    assert_eq!(state.retry_count, 3);
}

fn block_on_ready<F>(future: F) -> F::Output
where
    F: Future,
{
    let mut future = Box::pin(future);
    let waker = noop_waker();
    let mut context = Context::from_waker(&waker);

    match Future::poll(Pin::as_mut(&mut future), &mut context) {
        Poll::Ready(output) => output,
        Poll::Pending => panic!("future unexpectedly pending in unit test"),
    }
}

fn noop_waker() -> Waker {
    // SAFETY: The vtable does nothing and the raw pointer is never dereferenced.
    unsafe { Waker::from_raw(noop_raw_waker()) }
}

fn noop_raw_waker() -> RawWaker {
    RawWaker::new(std::ptr::null(), &NOOP_WAKER_VTABLE)
}

static NOOP_WAKER_VTABLE: RawWakerVTable = RawWakerVTable::new(
    |_data| noop_raw_waker(),
    |_data| {},
    |_data| {},
    |_data| {},
);
