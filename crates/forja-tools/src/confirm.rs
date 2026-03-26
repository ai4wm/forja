use async_trait::async_trait;
use forja_core::mode::ExecMode;
use std::sync::{Arc, Mutex};

/// Abstraction for explicit confirmation before executing commands or dangerous actions.
#[async_trait]
pub trait ConfirmationHandler: Send + Sync {
    /// Returns whether a command or action is approved for execution.
    async fn confirm(&self, cmd: &str, dangerous: bool) -> bool;
}

/// Default CLI implementation using stdin/stdout with a [y/N] prompt.
pub struct StdinConfirmation {
    exec_mode: Arc<Mutex<ExecMode>>,
}

impl StdinConfirmation {
    pub fn new(exec_mode: ExecMode) -> Self {
        Self {
            exec_mode: Arc::new(Mutex::new(exec_mode)),
        }
    }

    pub fn from_shared(exec_mode: Arc<Mutex<ExecMode>>) -> Self {
        Self { exec_mode }
    }

    pub fn should_confirm(&self, dangerous: bool) -> bool {
        match *self.exec_mode.lock().unwrap() {
            ExecMode::Safe => true,
            ExecMode::Auto => dangerous,
            ExecMode::Trust => false,
        }
    }
}

impl Default for StdinConfirmation {
    fn default() -> Self {
        Self::new(ExecMode::Auto)
    }
}

#[async_trait]
impl ConfirmationHandler for StdinConfirmation {
    async fn confirm(&self, cmd: &str, dangerous: bool) -> bool {
        if !self.should_confirm(dangerous) {
            return true;
        }

        // Blocking I/O is acceptable here because the model is paused and waiting for input.
        println!("\n⚠️  [SECURITY] The AI wants to execute the following command:");
        println!("> {}", cmd);
        println!("Allow? [y/N]: ");

        let mut input = String::new();
        if std::io::stdin().read_line(&mut input).is_ok() {
            // Accept "y", "Y", and "yes". Default is no.
            return input.trim().eq_ignore_ascii_case("y") || input.trim().eq_ignore_ascii_case("yes");
        }
        
        false
    }
}
