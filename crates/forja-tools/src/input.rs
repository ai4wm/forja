use async_trait::async_trait;
use enigo::{Axis, Button, Coordinate, Direction, Enigo, Key, Keyboard, Mouse, Settings};
use forja_core::error::{ForjaError, Result};
use forja_core::traits::Tool;
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};

use crate::confirm::ConfirmationHandler;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputCommand {
    TypeText {
        text: String,
    },
    KeyPress {
        key: String,
    },
    Hotkey {
        keys: Vec<String>,
    },
    MouseMove {
        x: i32,
        y: i32,
    },
    MouseClick {
        button: String,
        x: i32,
        y: i32,
    },
    MouseDoubleClick {
        x: i32,
        y: i32,
    },
    MouseDrag {
        from_x: i32,
        from_y: i32,
        to_x: i32,
        to_y: i32,
    },
    Scroll {
        direction: String,
        amount: i32,
    },
}

impl InputCommand {
    fn action_name(&self) -> &'static str {
        match self {
            Self::TypeText { .. } => "type_text",
            Self::KeyPress { .. } => "key_press",
            Self::Hotkey { .. } => "hotkey",
            Self::MouseMove { .. } => "mouse_move",
            Self::MouseClick { .. } => "mouse_click",
            Self::MouseDoubleClick { .. } => "mouse_double_click",
            Self::MouseDrag { .. } => "mouse_drag",
            Self::Scroll { .. } => "scroll",
        }
    }

    fn detail(&self) -> String {
        match self {
            Self::TypeText { text } => format!("type_text:{text}"),
            Self::KeyPress { key } => format!("key_press:{key}"),
            Self::Hotkey { keys } => format!("hotkey:{}", keys.join("+")),
            Self::MouseMove { x, y } => format!("mouse_move:{x},{y}"),
            Self::MouseClick { button, x, y } => format!("mouse_click:{button}@{x},{y}"),
            Self::MouseDoubleClick { x, y } => format!("mouse_double_click:{x},{y}"),
            Self::MouseDrag {
                from_x,
                from_y,
                to_x,
                to_y,
            } => format!("mouse_drag:{from_x},{from_y}->{to_x},{to_y}"),
            Self::Scroll { direction, amount } => format!("scroll:{direction}:{amount}"),
        }
    }
}

pub trait InputBackend: Send + Sync {
    fn execute(&self, command: &InputCommand) -> std::result::Result<(), String>;
}

pub struct EnigoBackend {
    enigo: Mutex<Enigo>,
}

// Safety: Enigo is only accessed through Mutex, ensuring single-threaded access
unsafe impl Send for EnigoBackend {}
unsafe impl Sync for EnigoBackend {}

impl EnigoBackend {
    pub fn new() -> Result<Self> {
        let enigo = Enigo::new(&Settings::default()).map_err(|error| {
            ForjaError::ToolError(format!("Failed to initialize input backend: {error}"))
        })?;

        Ok(Self {
            enigo: Mutex::new(enigo),
        })
    }
}

impl InputBackend for EnigoBackend {
    fn execute(&self, command: &InputCommand) -> std::result::Result<(), String> {
        let mut enigo = self
            .enigo
            .lock()
            .map_err(|_| "Input backend lock was poisoned".to_string())?;

        match command {
            InputCommand::TypeText { text } => enigo.text(text).map_err(|error| error.to_string()),
            InputCommand::KeyPress { key } => enigo
                .key(enigo_key(key)?, Direction::Click)
                .map_err(|error| error.to_string()),
            InputCommand::Hotkey { keys } => execute_hotkey(&mut enigo, keys),
            InputCommand::MouseMove { x, y } => enigo
                .move_mouse(*x, *y, Coordinate::Abs)
                .map_err(|error| error.to_string()),
            InputCommand::MouseClick { button, x, y } => {
                enigo
                    .move_mouse(*x, *y, Coordinate::Abs)
                    .map_err(|error| error.to_string())?;
                enigo
                    .button(mouse_button(button)?, Direction::Click)
                    .map_err(|error| error.to_string())
            }
            InputCommand::MouseDoubleClick { x, y } => {
                enigo
                    .move_mouse(*x, *y, Coordinate::Abs)
                    .map_err(|error| error.to_string())?;
                enigo
                    .button(Button::Left, Direction::Click)
                    .map_err(|error| error.to_string())?;
                enigo
                    .button(Button::Left, Direction::Click)
                    .map_err(|error| error.to_string())
            }
            InputCommand::MouseDrag {
                from_x,
                from_y,
                to_x,
                to_y,
            } => {
                enigo
                    .move_mouse(*from_x, *from_y, Coordinate::Abs)
                    .map_err(|error| error.to_string())?;
                enigo
                    .button(Button::Left, Direction::Press)
                    .map_err(|error| error.to_string())?;
                enigo
                    .move_mouse(*to_x, *to_y, Coordinate::Abs)
                    .map_err(|error| error.to_string())?;
                enigo
                    .button(Button::Left, Direction::Release)
                    .map_err(|error| error.to_string())
            }
            InputCommand::Scroll { direction, amount } => {
                let (length, axis) = scroll_target(direction, *amount)?;
                enigo
                    .scroll(length, axis)
                    .map_err(|error| error.to_string())
            }
        }
    }
}

pub struct MockBackend {
    events: Mutex<Vec<String>>,
}

impl MockBackend {
    pub fn new() -> Self {
        Self {
            events: Mutex::new(Vec::new()),
        }
    }

    pub fn events_snapshot(&self) -> Vec<String> {
        match self.events.lock() {
            Ok(events) => events.clone(),
            Err(_) => Vec::new(),
        }
    }
}

impl Default for MockBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl InputBackend for MockBackend {
    fn execute(&self, command: &InputCommand) -> std::result::Result<(), String> {
        let mut events = self
            .events
            .lock()
            .map_err(|_| "Mock backend lock was poisoned".to_string())?;
        events.push(command.detail());
        Ok(())
    }
}

pub struct InputTool {
    backend: Arc<dyn InputBackend>,
    confirmation_handler: Arc<dyn ConfirmationHandler>,
    unsafe_mode: bool,
}

impl InputTool {
    pub fn new(handler: Arc<dyn ConfirmationHandler>) -> Result<Self> {
        let backend = Arc::new(EnigoBackend::new()?);

        Ok(Self::with_backend_and_settings(backend, handler, false))
    }

    pub fn with_backend_and_settings(
        backend: Arc<dyn InputBackend>,
        handler: Arc<dyn ConfirmationHandler>,
        unsafe_mode: bool,
    ) -> Self {
        Self {
            backend,
            confirmation_handler: handler,
            unsafe_mode,
        }
    }

    fn parse_command(
        &self,
        action: &str,
        args: &Value,
    ) -> std::result::Result<InputCommand, String> {
        match action {
            "type_text" => Ok(InputCommand::TypeText {
                text: required_string(args, "text")?.to_string(),
            }),
            "key_press" => Ok(InputCommand::KeyPress {
                key: normalize_key(required_string(args, "key")?, false)?,
            }),
            "hotkey" => {
                let keys = required_string_array(args, "keys")?
                    .into_iter()
                    .map(|key| normalize_key(&key, true))
                    .collect::<std::result::Result<Vec<_>, _>>()?;
                if keys.is_empty() {
                    return Err("Missing 'keys' values".to_string());
                }

                Ok(InputCommand::Hotkey { keys })
            }
            "mouse_move" => Ok(InputCommand::MouseMove {
                x: required_i32(args, "x")?,
                y: required_i32(args, "y")?,
            }),
            "mouse_click" => Ok(InputCommand::MouseClick {
                button: normalize_button(required_string(args, "button")?)?.to_string(),
                x: required_i32(args, "x")?,
                y: required_i32(args, "y")?,
            }),
            "mouse_double_click" => Ok(InputCommand::MouseDoubleClick {
                x: required_i32(args, "x")?,
                y: required_i32(args, "y")?,
            }),
            "mouse_drag" => Ok(InputCommand::MouseDrag {
                from_x: required_i32(args, "from_x")?,
                from_y: required_i32(args, "from_y")?,
                to_x: required_i32(args, "to_x")?,
                to_y: required_i32(args, "to_y")?,
            }),
            "scroll" => {
                let direction = normalize_direction(required_string(args, "direction")?)?;
                let amount = required_i32(args, "amount")?;
                if amount <= 0 {
                    return Err("Field 'amount' must be greater than 0".to_string());
                }

                Ok(InputCommand::Scroll {
                    direction: direction.to_string(),
                    amount,
                })
            }
            other => Err(format!("Unsupported input action: {other}")),
        }
    }
}

#[async_trait]
impl Tool for InputTool {
    fn name(&self) -> &str {
        "input"
    }

    fn definition(&self) -> forja_core::types::ToolDefinition {
        forja_core::types::ToolDefinition {
            name: self.name().to_string(),
            description: "Control keyboard and mouse input. Supports typing, key presses, hotkeys, mouse clicks, drags, and scrolling.".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "action": {
                        "type": "string",
                        "description": "Input action: type_text, key_press, hotkey, mouse_move, mouse_click, mouse_double_click, mouse_drag, or scroll."
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

        let command = match self.parse_command(&action, &args) {
            Ok(command) => command,
            Err(detail) => return Ok(error_result(&action, detail)),
        };

        let dangerous =
            matches!(&command, InputCommand::Hotkey { keys } if is_dangerous_hotkey(keys));
        if !self.unsafe_mode
            && !self
                .confirmation_handler
                .confirm(&command.detail(), dangerous)
                .await
        {
            let detail = if dangerous {
                format!(
                    "Blocked dangerous hotkey: {}",
                    match &command {
                        InputCommand::Hotkey { keys } => keys.join("+"),
                        _ => command.detail(),
                    }
                )
            } else {
                format!("Blocked input action: {}", command.detail())
            };
            return Ok(blocked_result(command.action_name(), detail));
        }

        match self.backend.execute(&command) {
            Ok(()) => Ok(ok_result(command.action_name(), command.detail())),
            Err(detail) => Ok(error_result(command.action_name(), detail)),
        }
    }
}

fn execute_hotkey(enigo: &mut Enigo, keys: &[String]) -> std::result::Result<(), String> {
    if keys.is_empty() {
        return Err("Hotkey requires at least one key".to_string());
    }

    let (last_key, held_keys) = keys
        .split_last()
        .ok_or_else(|| "Hotkey requires at least one key".to_string())?;

    for key in held_keys {
        enigo
            .key(enigo_key(key)?, Direction::Press)
            .map_err(|error| error.to_string())?;
    }

    let click_result = enigo
        .key(enigo_key(last_key)?, Direction::Click)
        .map_err(|error| error.to_string());

    for key in held_keys.iter().rev() {
        let _ = enigo.key(enigo_key(key)?, Direction::Release);
    }

    click_result
}

fn enigo_key(key: &str) -> std::result::Result<Key, String> {
    Ok(match key {
        "enter" => Key::Return,
        "tab" => Key::Tab,
        "escape" => Key::Escape,
        "space" => Key::Space,
        "backspace" => Key::Backspace,
        "delete" => Key::Delete,
        "home" => Key::Home,
        "end" => Key::End,
        "pageup" => Key::PageUp,
        "pagedown" => Key::PageDown,
        "up" => Key::UpArrow,
        "down" => Key::DownArrow,
        "left" => Key::LeftArrow,
        "right" => Key::RightArrow,
        "f1" => Key::F1,
        "f2" => Key::F2,
        "f3" => Key::F3,
        "f4" => Key::F4,
        "f5" => Key::F5,
        "f6" => Key::F6,
        "f7" => Key::F7,
        "f8" => Key::F8,
        "f9" => Key::F9,
        "f10" => Key::F10,
        "f11" => Key::F11,
        "f12" => Key::F12,
        "ctrl" => Key::Control,
        "shift" => Key::Shift,
        "alt" => Key::Alt,
        "win" => Key::Meta,
        _ => single_unicode_key(key)?,
    })
}

fn single_unicode_key(key: &str) -> std::result::Result<Key, String> {
    let mut chars = key.chars();
    let Some(character) = chars.next() else {
        return Err("Empty key is not allowed".to_string());
    };
    if chars.next().is_some() {
        return Err(format!("Unsupported key: {key}"));
    }

    Ok(Key::Unicode(character))
}

fn mouse_button(button: &str) -> std::result::Result<Button, String> {
    Ok(match button {
        "left" => Button::Left,
        "right" => Button::Right,
        "middle" => Button::Middle,
        _ => return Err(format!("Unsupported mouse button: {button}")),
    })
}

fn scroll_target(direction: &str, amount: i32) -> std::result::Result<(i32, Axis), String> {
    Ok(match direction {
        "up" => (-amount, Axis::Vertical),
        "down" => (amount, Axis::Vertical),
        "left" => (-amount, Axis::Horizontal),
        "right" => (amount, Axis::Horizontal),
        _ => return Err(format!("Unsupported scroll direction: {direction}")),
    })
}

fn normalize_key(key: &str, allow_modifiers: bool) -> std::result::Result<String, String> {
    let normalized = key.trim().to_lowercase().replace([' ', '-'], "");
    let key = match normalized.as_str() {
        "enter" | "return" => "enter",
        "tab" => "tab",
        "esc" | "escape" => "escape",
        "space" => "space",
        "backspace" => "backspace",
        "delete" | "del" => "delete",
        "home" => "home",
        "end" => "end",
        "pageup" => "pageup",
        "pagedown" => "pagedown",
        "up" | "uparrow" => "up",
        "down" | "downarrow" => "down",
        "left" | "leftarrow" => "left",
        "right" | "rightarrow" => "right",
        "f1" => "f1",
        "f2" => "f2",
        "f3" => "f3",
        "f4" => "f4",
        "f5" => "f5",
        "f6" => "f6",
        "f7" => "f7",
        "f8" => "f8",
        "f9" => "f9",
        "f10" => "f10",
        "f11" => "f11",
        "f12" => "f12",
        "ctrl" | "control" => "ctrl",
        "shift" => "shift",
        "alt" | "option" => "alt",
        "win" | "windows" | "super" | "meta" | "command" | "cmd" => "win",
        _ if normalized.chars().count() == 1 => normalized.as_str(),
        _ => return Err(format!("Unsupported key: {key}")),
    };

    if !allow_modifiers && matches!(key, "ctrl" | "shift" | "alt" | "win") {
        return Err(format!("Modifier-only key_press is not supported: {key}"));
    }

    Ok(key.to_string())
}

fn normalize_button(button: &str) -> std::result::Result<&'static str, String> {
    match button.trim().to_lowercase().as_str() {
        "left" => Ok("left"),
        "right" => Ok("right"),
        "middle" => Ok("middle"),
        _ => Err(format!("Unsupported mouse button: {button}")),
    }
}

fn normalize_direction(direction: &str) -> std::result::Result<&'static str, String> {
    match direction.trim().to_lowercase().as_str() {
        "up" => Ok("up"),
        "down" => Ok("down"),
        "left" => Ok("left"),
        "right" => Ok("right"),
        _ => Err(format!("Unsupported scroll direction: {direction}")),
    }
}

fn is_dangerous_hotkey(keys: &[String]) -> bool {
    let has = |target: &str| keys.iter().any(|key| key == target);

    (has("alt") && has("f4"))
        || (has("ctrl") && has("alt") && has("delete"))
        || (has("win") && has("l"))
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

fn required_string_array(args: &Value, field: &str) -> std::result::Result<Vec<String>, String> {
    let values = args[field]
        .as_array()
        .ok_or_else(|| format!("Missing required array field '{field}'"))?;
    if values.is_empty() {
        return Err(format!("Missing '{field}' values"));
    }

    values
        .iter()
        .map(|value| {
            value
                .as_str()
                .map(str::trim)
                .filter(|item| !item.is_empty())
                .map(ToOwned::to_owned)
                .ok_or_else(|| format!("Field '{field}' must contain only non-empty strings"))
        })
        .collect()
}

fn ok_result(action: &str, detail: String) -> Value {
    json!({
        "status": "ok",
        "action": action,
        "detail": detail,
    })
}

fn error_result(action: &str, detail: String) -> Value {
    json!({
        "status": "error",
        "action": action,
        "detail": detail,
    })
}

fn blocked_result(action: &str, detail: String) -> Value {
    json!({
        "status": "blocked",
        "action": action,
        "detail": detail,
    })
}
