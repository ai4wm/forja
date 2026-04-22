use super::backend::BrowserBackend;
use super::chromium::ChromiumBackend;
use crate::confirm::ConfirmationHandler;
use async_trait::async_trait;
use forja_core::error::Result;
use forja_core::traits::Tool;
use serde_json::{Value, json};
use std::sync::Arc;

pub struct BrowserTool {
    backend: Arc<dyn BrowserBackend>,
    confirmation_handler: Arc<dyn ConfirmationHandler>,
    unsafe_mode: bool,
}

impl BrowserTool {
    pub fn new(handler: Arc<dyn ConfirmationHandler>) -> Self {
        Self::with_backend_and_settings(Arc::new(ChromiumBackend::new()), handler, false)
    }

    pub fn with_backend_and_settings(
        backend: Arc<dyn BrowserBackend>,
        handler: Arc<dyn ConfirmationHandler>,
        unsafe_mode: bool,
    ) -> Self {
        Self {
            backend,
            confirmation_handler: handler,
            unsafe_mode,
        }
    }
}

#[async_trait]
impl Tool for BrowserTool {
    fn name(&self) -> &str {
        "browser"
    }

    fn definition(&self) -> forja_core::types::ToolDefinition {
        forja_core::types::ToolDefinition {
            name: self.name().to_string(),
            description: "Control a Chromium-based browser via CDP. Supports navigation, clicking, typing, reading content, screenshots, JavaScript execution, and tab management.".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "action": {
                        "type": "string",
                        "description": "Browser action to execute."
                    }
                },
                "required": ["action"]
            }),
        }
    }

    async fn execute(&self, args: Value) -> Result<Value> {
        let action = args["action"]
            .as_str()
            .map(str::trim)
            .unwrap_or_default()
            .to_string();

        if action.is_empty() {
            return Ok(error_result("", "Missing 'action' field".to_string()));
        }

        if action == "evaluate" {
            let js = match required_string(&args, "js") {
                Ok(js) => js,
                Err(detail) => return Ok(error_result(&action, detail)),
            };

            let dangerous = is_dangerous_javascript(js);
            if !self.unsafe_mode {
                let prompt = format!("browser evaluate: {js}");
                if !self.confirmation_handler.confirm(&prompt, dangerous).await {
                    return Ok(blocked_result(
                        &action,
                        if dangerous {
                            format!("Blocked dangerous browser evaluate: {js}")
                        } else {
                            format!("Blocked browser evaluate: {js}")
                        },
                    ));
                }
            }

            return Ok(match self.backend.evaluate(js).await {
                Ok(data) => ok_result(&action, data),
                Err(detail) => error_result(&action, detail),
            });
        }

        if !self.unsafe_mode
            && !self
                .confirmation_handler
                .confirm(&format!("browser action: {action}"), false)
                .await
        {
            return Ok(blocked_result(
                &action,
                format!("Blocked browser action: {action}"),
            ));
        }

        let result = match action.as_str() {
            "open" | "goto" => execute_url_action(&action, &args, self.backend.as_ref()).await,
            "scroll" => execute_scroll_action(&action, &args, self.backend.as_ref()).await,
            "click" | "read_text" => {
                execute_selector_action(&action, &args, self.backend.as_ref()).await
            }
            "type_text" => execute_type_text_action(&action, &args, self.backend.as_ref()).await,
            "read_page" => simple_action_result(&action, self.backend.read_page().await),
            "screenshot" => simple_action_result(&action, self.backend.screenshot().await),
            "tab_list" => simple_action_result(&action, self.backend.tab_list().await),
            "tab_switch" | "tab_close" => {
                execute_tab_action(&action, &args, self.backend.as_ref()).await
            }
            "back" => simple_action_result(&action, self.backend.back().await),
            "forward" => simple_action_result(&action, self.backend.forward().await),
            _ => error_result(&action, format!("Unsupported browser action: {action}")),
        };

        Ok(result)
    }
}

async fn execute_url_action(action: &str, args: &Value, backend: &dyn BrowserBackend) -> Value {
    let url = match required_string(args, "url") {
        Ok(url) => url,
        Err(detail) => return error_result(action, detail),
    };
    let result = match action {
        "open" => backend.open(url).await,
        _ => backend.goto(url).await,
    };
    simple_action_result(action, result)
}

async fn execute_scroll_action(action: &str, args: &Value, backend: &dyn BrowserBackend) -> Value {
    let direction = match required_direction(args) {
        Ok(direction) => direction,
        Err(detail) => return error_result(action, detail),
    };
    let amount = match required_i32(args, "amount") {
        Ok(amount) if amount > 0 => amount,
        Ok(_) => return error_result(action, "Field 'amount' must be greater than 0".to_string()),
        Err(detail) => return error_result(action, detail),
    };
    simple_action_result(action, backend.scroll(direction, amount).await)
}

async fn execute_selector_action(
    action: &str,
    args: &Value,
    backend: &dyn BrowserBackend,
) -> Value {
    let selector = match required_string(args, "selector") {
        Ok(selector) => selector,
        Err(detail) => return error_result(action, detail),
    };
    let result = match action {
        "click" => backend.click(selector).await,
        _ => backend.read_text(selector).await,
    };
    simple_action_result(action, result)
}

async fn execute_type_text_action(
    action: &str,
    args: &Value,
    backend: &dyn BrowserBackend,
) -> Value {
    let selector = match required_string(args, "selector") {
        Ok(selector) => selector,
        Err(detail) => return error_result(action, detail),
    };
    let text = match required_string(args, "text") {
        Ok(text) => text,
        Err(detail) => return error_result(action, detail),
    };
    simple_action_result(action, backend.type_text(selector, text).await)
}

async fn execute_tab_action(action: &str, args: &Value, backend: &dyn BrowserBackend) -> Value {
    let index = match required_index(args) {
        Ok(index) => index,
        Err(detail) => return error_result(action, detail),
    };
    let result = match action {
        "tab_switch" => backend.tab_switch(index).await,
        _ => backend.tab_close(index).await,
    };
    simple_action_result(action, result)
}

fn required_string<'a>(args: &'a Value, field: &str) -> std::result::Result<&'a str, String> {
    args[field]
        .as_str()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("Missing required string field '{field}'"))
}

fn required_i32(args: &Value, field: &str) -> std::result::Result<i32, String> {
    let value = args[field]
        .as_i64()
        .ok_or_else(|| format!("Missing required integer field '{field}'"))?;

    i32::try_from(value).map_err(|_| format!("Field '{field}' is out of range"))
}

fn required_index(args: &Value) -> std::result::Result<usize, String> {
    let value = args["index"]
        .as_u64()
        .ok_or_else(|| "Missing required integer field 'index'".to_string())?;

    usize::try_from(value).map_err(|_| "Field 'index' is out of range".to_string())
}

fn required_direction(args: &Value) -> std::result::Result<&'static str, String> {
    match required_string(args, "direction")?.to_lowercase().as_str() {
        "up" => Ok("up"),
        "down" => Ok("down"),
        _ => Err("Field 'direction' must be 'up' or 'down'".to_string()),
    }
}

fn is_dangerous_javascript(js: &str) -> bool {
    let normalized = js.to_lowercase();

    normalized.contains("document.cookie")
        || normalized.contains("localstorage.clear")
        || normalized.contains("sessionstorage.clear")
        || (normalized.contains("fetch(") && normalized.contains("http"))
}

fn simple_action_result(action: &str, result: std::result::Result<String, String>) -> Value {
    match result {
        Ok(data) => ok_result(action, data),
        Err(detail) => error_result(action, detail),
    }
}

fn ok_result(action: &str, data: String) -> Value {
    json!({
        "status": "ok",
        "action": action,
        "data": data,
    })
}

fn error_result(action: &str, data: String) -> Value {
    json!({
        "status": "error",
        "action": action,
        "data": data,
    })
}

fn blocked_result(action: &str, data: String) -> Value {
    json!({
        "status": "blocked",
        "action": action,
        "data": data,
    })
}
