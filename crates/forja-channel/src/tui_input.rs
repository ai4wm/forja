#[cfg(feature = "tui")]
use crate::tui_channel::{DisplayMessage, FocusPane, TuiState};
#[cfg(feature = "tui")]
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
#[cfg(feature = "tui")]
use forja_core::types::{Message, Role};
#[cfg(feature = "tui")]
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(feature = "tui")]
use std::sync::{Arc, Mutex};
#[cfg(feature = "tui")]
use tokio::sync::mpsc;

#[cfg(feature = "tui")]
pub(crate) fn handle_key_event(
    key: KeyEvent,
    state: &Arc<Mutex<TuiState>>,
    input_tx: &mpsc::UnboundedSender<Message>,
    running: &Arc<AtomicBool>,
) -> bool {
    let Ok(mut state) = state.lock() else {
        return false;
    };

    if let Some(confirm) = &mut state.pending_confirm {
        match key.code {
            KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => {
                if let Some(responder) = confirm.responder.take() {
                    let _ = responder.send(true);
                }
                state.pending_confirm = None;
            }
            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                if let Some(responder) = confirm.responder.take() {
                    let _ = responder.send(false);
                }
                state.pending_confirm = None;
            }
            _ => {}
        }
        return false;
    }

    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('q') {
        running.store(false, Ordering::SeqCst);
        return true;
    }

    match key.code {
        KeyCode::Esc => {
            running.store(false, Ordering::SeqCst);
            true
        }
        KeyCode::F(1) => {
            state.help_overlay = !state.help_overlay;
            false
        }
        KeyCode::Tab => {
            state.focus = match state.focus {
                FocusPane::Conversation => FocusPane::Notifications,
                FocusPane::Notifications => FocusPane::Input,
                FocusPane::Input => FocusPane::Conversation,
            };
            false
        }
        KeyCode::Up => {
            if let Ok(mut buffer) = state.messages.lock() {
                buffer.scroll_up();
            }
            false
        }
        KeyCode::Down => {
            if let Ok(mut buffer) = state.messages.lock() {
                buffer.scroll_down();
            }
            false
        }
        KeyCode::Char('l') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            if let Ok(mut buffer) = state.messages.lock() {
                buffer.clear();
            }
            false
        }
        KeyCode::Enter => {
            let input = state.input.trim().to_string();
            if input.is_empty() {
                return false;
            }
            if let Ok(mut buffer) = state.messages.lock() {
                buffer.push(DisplayMessage {
                    role: "User".to_string(),
                    text: input.clone(),
                });
            }
            let _ = input_tx.send(Message::text(Role::User, input, None));
            state.input.clear();
            false
        }
        KeyCode::Backspace => {
            state.input.pop();
            false
        }
        KeyCode::Char(character) => {
            state.input.push(character);
            false
        }
        _ => false,
    }
}
