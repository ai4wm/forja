use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
use async_trait::async_trait;
use chromiumoxide::browser::{Browser, BrowserConfig};
use chromiumoxide::cdp::browser_protocol::page::CaptureScreenshotFormat;
use chromiumoxide::page::ScreenshotParams;
use chromiumoxide::Page;
use forja_core::error::Result;
use forja_core::traits::Tool;
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};
use tokio::runtime::Handle;

use crate::confirm::ConfirmationHandler;

#[async_trait]
pub trait BrowserBackend: Send + Sync + 'static {
    async fn open(&self, url: &str) -> std::result::Result<String, String>;
    async fn goto(&self, url: &str) -> std::result::Result<String, String>;
    async fn scroll(&self, direction: &str, amount: i32) -> std::result::Result<String, String>;
    async fn click(&self, selector: &str) -> std::result::Result<String, String>;
    async fn type_text(&self, selector: &str, text: &str) -> std::result::Result<String, String>;
    async fn read_text(&self, selector: &str) -> std::result::Result<String, String>;
    async fn read_page(&self) -> std::result::Result<String, String>;
    async fn screenshot(&self) -> std::result::Result<String, String>;
    async fn evaluate(&self, js: &str) -> std::result::Result<String, String>;
    async fn tab_list(&self) -> std::result::Result<String, String>;
    async fn tab_switch(&self, index: usize) -> std::result::Result<String, String>;
    async fn tab_close(&self, index: usize) -> std::result::Result<String, String>;
    async fn back(&self) -> std::result::Result<String, String>;
    async fn forward(&self) -> std::result::Result<String, String>;
}

pub struct MockBrowserBackend {
    calls: Mutex<Vec<String>>,
}

impl MockBrowserBackend {
    pub fn new() -> Self {
        Self {
            calls: Mutex::new(Vec::new()),
        }
    }

    pub fn calls_snapshot(&self) -> Vec<String> {
        match self.calls.lock() {
            Ok(calls) => calls.clone(),
            Err(_) => Vec::new(),
        }
    }

    fn push_call(&self, call: String) -> std::result::Result<(), String> {
        let mut calls = self
            .calls
            .lock()
            .map_err(|_| "Mock browser backend lock was poisoned".to_string())?;
        calls.push(call);
        Ok(())
    }
}

impl Default for MockBrowserBackend {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl BrowserBackend for MockBrowserBackend {
    async fn open(&self, url: &str) -> std::result::Result<String, String> {
        self.push_call(format!("open:{url}"))?;
        Ok("opened".to_string())
    }

    async fn goto(&self, url: &str) -> std::result::Result<String, String> {
        self.push_call(format!("goto:{url}"))?;
        Ok("navigated".to_string())
    }

    async fn scroll(&self, direction: &str, amount: i32) -> std::result::Result<String, String> {
        self.push_call(format!("scroll:{direction}:{amount}"))?;
        Ok("scrolled".to_string())
    }

    async fn click(&self, selector: &str) -> std::result::Result<String, String> {
        self.push_call(format!("click:{selector}"))?;
        Ok("clicked".to_string())
    }

    async fn type_text(&self, selector: &str, text: &str) -> std::result::Result<String, String> {
        self.push_call(format!("type_text:{selector}:{text}"))?;
        Ok("typed".to_string())
    }

    async fn read_text(&self, selector: &str) -> std::result::Result<String, String> {
        self.push_call(format!("read_text:{selector}"))?;
        Ok("mock text".to_string())
    }

    async fn read_page(&self) -> std::result::Result<String, String> {
        self.push_call("read_page".to_string())?;
        Ok("mock page text".to_string())
    }

    async fn screenshot(&self) -> std::result::Result<String, String> {
        self.push_call("screenshot".to_string())?;
        Ok("bW9jay1zY3JlZW5zaG90".to_string())
    }

    async fn evaluate(&self, js: &str) -> std::result::Result<String, String> {
        self.push_call(format!("evaluate:{js}"))?;
        Ok("mock eval result".to_string())
    }

    async fn tab_list(&self) -> std::result::Result<String, String> {
        self.push_call("tab_list".to_string())?;
        Ok("[\"tab-0\",\"tab-1\"]".to_string())
    }

    async fn tab_switch(&self, index: usize) -> std::result::Result<String, String> {
        self.push_call(format!("tab_switch:{index}"))?;
        Ok("tab switched".to_string())
    }

    async fn tab_close(&self, index: usize) -> std::result::Result<String, String> {
        self.push_call(format!("tab_close:{index}"))?;
        Ok("tab closed".to_string())
    }

    async fn back(&self) -> std::result::Result<String, String> {
        self.push_call("back".to_string())?;
        Ok("went back".to_string())
    }

    async fn forward(&self) -> std::result::Result<String, String> {
        self.push_call("forward".to_string())?;
        Ok("went forward".to_string())
    }
}

struct ChromiumSession {
    browser: Browser,
    current_page: Option<Page>,
}

pub struct ChromiumBackend {
    port: u16,
    browser_path: Option<String>,
    session: tokio::sync::Mutex<Option<ChromiumSession>>,
}

impl ChromiumBackend {
    pub fn new() -> Self {
        let port = std::env::var("FORJA_BROWSER_PORT")
            .ok()
            .and_then(|value| value.parse::<u16>().ok())
            .filter(|value| *value > 0)
            .unwrap_or(9222);
        let browser_path = std::env::var("FORJA_BROWSER_PATH")
            .ok()
            .map(|path| path.trim().to_string())
            .filter(|path| !path.is_empty());

        Self {
            port,
            browser_path,
            session: tokio::sync::Mutex::new(None),
        }
    }

    async fn ensure_session(&self) -> std::result::Result<(), String> {
        let mut session = self.session.lock().await;
        if session.is_some() {
            return Ok(());
        }

        let ws_url = format!("http://127.0.0.1:{}", self.port);
        if let Ok((browser, handler)) = Browser::connect(ws_url.as_str()).await {
            spawn_browser_handler(handler);
            *session = Some(ChromiumSession {
                browser,
                current_page: None,
            });
            return Ok(());
        }

        let mut config_builder = BrowserConfig::builder()
            .no_sandbox()
            .window_size(1280, 900)
            .port(self.port);
        if let Some(path) = &self.browser_path {
            config_builder = config_builder.chrome_executable(path);
        }
        let config = config_builder
            .build()
            .map_err(|error| format!("Failed to build browser config: {error}"))?;
        let (browser, handler) = Browser::launch(config)
            .await
            .map_err(|error| format!("Failed to launch Chromium browser: {error}"))?;
        spawn_browser_handler(handler);
        *session = Some(ChromiumSession {
            browser,
            current_page: None,
        });

        Ok(())
    }

    async fn with_page<F, Fut>(&self, f: F) -> std::result::Result<String, String>
    where
        F: FnOnce(Page) -> Fut + Send,
        Fut: std::future::Future<Output = std::result::Result<(Page, String), String>> + Send,
    {
        self.ensure_session().await?;
        let current_page = {
            let mut session = self.session.lock().await;
            let chromium = session
                .as_mut()
                .ok_or_else(|| "Browser session is not available".to_string())?;

            if let Some(page) = chromium.current_page.clone() {
                page
            } else {
                let page = chromium
                    .browser
                    .new_page("about:blank")
                    .await
                    .map_err(|error| format!("Failed to create a page: {error}"))?;
                chromium.current_page = Some(page.clone());
                page
            }
        };

        let (page, data) = f(current_page).await?;

        let mut session = self.session.lock().await;
        if let Some(session) = session.as_mut() {
            session.current_page = Some(page);
        }

        Ok(data)
    }
}

impl Default for ChromiumBackend {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl BrowserBackend for ChromiumBackend {
    async fn open(&self, url: &str) -> std::result::Result<String, String> {
        self.ensure_session().await?;
        let mut session = self.session.lock().await;
        let chromium = session
            .as_mut()
            .ok_or_else(|| "Browser session is not available".to_string())?;
        let page = chromium
            .browser
            .new_page(url)
            .await
            .map_err(|error| format!("Failed to open page: {error}"))?;
        chromium.current_page = Some(page);

        Ok(format!("opened {url}"))
    }

    async fn goto(&self, url: &str) -> std::result::Result<String, String> {
        self.with_page(|page| async move {
            page.goto(url)
                .await
                .map_err(|error| format!("Failed to navigate: {error}"))?;
            Ok((page, format!("navigated to {url}")))
        })
        .await
    }

    async fn scroll(&self, direction: &str, amount: i32) -> std::result::Result<String, String> {
        let delta = match direction {
            "up" => -amount,
            "down" => amount,
            _ => return Err(format!("Unsupported scroll direction: {direction}")),
        };

        self.with_page(|page| async move {
            page.evaluate(format!("window.scrollBy(0, {delta})"))
                .await
                .map_err(|error| format!("Failed to scroll page: {error}"))?;
            Ok((page, format!("scrolled {direction} by {amount}px")))
        })
        .await
    }

    async fn click(&self, selector: &str) -> std::result::Result<String, String> {
        self.with_page(|page| async move {
            let element = page
                .find_element(selector)
                .await
                .map_err(|error| format!("Failed to find element: {error}"))?;
            element
                .click()
                .await
                .map_err(|error| format!("Failed to click element: {error}"))?;
            Ok((page, format!("clicked {selector}")))
        })
        .await
    }

    async fn type_text(&self, selector: &str, text: &str) -> std::result::Result<String, String> {
        self.with_page(|page| async move {
            let element = page
                .find_element(selector)
                .await
                .map_err(|error| format!("Failed to find element: {error}"))?;
            element
                .click()
                .await
                .map_err(|error| format!("Failed to focus element: {error}"))?;
            element
                .type_str(text)
                .await
                .map_err(|error| format!("Failed to type text: {error}"))?;
            Ok((page, format!("typed text into {selector}")))
        })
        .await
    }

    async fn read_text(&self, selector: &str) -> std::result::Result<String, String> {
        self.with_page(|page| async move {
            let value = page
                .evaluate(format!(
                    "(() => {{ const el = document.querySelector({selector:?}); return el ? el.innerText : ''; }})()"
                ))
                .await
                .map_err(|error| format!("Failed to read text: {error}"))?;
            Ok((page, format!("{value:?}")))
        })
        .await
    }

    async fn read_page(&self) -> std::result::Result<String, String> {
        self.with_page(|page| async move {
            let value = page
                .evaluate("document.body ? document.body.innerText : ''")
                .await
                .map_err(|error| format!("Failed to read page text: {error}"))?;
            Ok((page, format!("{value:?}")))
        })
        .await
    }

    async fn screenshot(&self) -> std::result::Result<String, String> {
        self.with_page(|page| async move {
            let params = ScreenshotParams::builder()
                .format(CaptureScreenshotFormat::Png)
                .build();
            let image = page
                .screenshot(params)
                .await
                .map_err(|error| format!("Failed to capture screenshot: {error}"))?;
            Ok((page, BASE64_STANDARD.encode(image)))
        })
        .await
    }

    async fn evaluate(&self, js: &str) -> std::result::Result<String, String> {
        self.with_page(|page| async move {
            let value = page
                .evaluate(js)
                .await
                .map_err(|error| format!("Failed to evaluate JavaScript: {error}"))?;
            Ok((page, format!("{value:?}")))
        })
        .await
    }

    async fn tab_list(&self) -> std::result::Result<String, String> {
        self.ensure_session().await?;
        let session = self.session.lock().await;
        let chromium = session
            .as_ref()
            .ok_or_else(|| "Browser session is not available".to_string())?;
        let pages = chromium
            .browser
            .pages()
            .await
            .map_err(|error| format!("Failed to list tabs: {error}"))?;
        let tab_names = pages
            .into_iter()
            .enumerate()
            .map(|(index, page)| format!("{index}:{:?}", page.target_id()))
            .collect::<Vec<_>>();

        Ok(tab_names.join("\n"))
    }

    async fn tab_switch(&self, index: usize) -> std::result::Result<String, String> {
        self.ensure_session().await?;
        let mut session = self.session.lock().await;
        let chromium = session
            .as_mut()
            .ok_or_else(|| "Browser session is not available".to_string())?;
        let page = chromium
            .browser
            .pages()
            .await
            .map_err(|error| format!("Failed to list tabs: {error}"))?
            .into_iter()
            .nth(index)
            .ok_or_else(|| format!("Tab index out of range: {index}"))?;
        chromium.current_page = Some(page);

        Ok(format!("switched to tab {index}"))
    }

    async fn tab_close(&self, index: usize) -> std::result::Result<String, String> {
        self.ensure_session().await?;
        let mut session = self.session.lock().await;
        let chromium = session
            .as_mut()
            .ok_or_else(|| "Browser session is not available".to_string())?;
        let page = chromium
            .browser
            .pages()
            .await
            .map_err(|error| format!("Failed to list tabs: {error}"))?
            .into_iter()
            .nth(index)
            .ok_or_else(|| format!("Tab index out of range: {index}"))?;
        let closed_target_id = page.target_id().clone();
        page.close()
            .await
            .map_err(|error| format!("Failed to close tab: {error}"))?;
        if chromium
            .current_page
            .as_ref()
            .is_some_and(|current| current.target_id() == &closed_target_id)
        {
            chromium.current_page = None;
        }

        Ok(format!("closed tab {index}"))
    }

    async fn back(&self) -> std::result::Result<String, String> {
        self.with_page(|page| async move {
            page.evaluate("history.back()")
                .await
                .map_err(|error| format!("Failed to navigate back: {error}"))?;
            Ok((page, "went back".to_string()))
        })
        .await
    }

    async fn forward(&self) -> std::result::Result<String, String> {
        self.with_page(|page| async move {
            page.evaluate("history.forward()")
                .await
                .map_err(|error| format!("Failed to navigate forward: {error}"))?;
            Ok((page, "went forward".to_string()))
        })
        .await
    }
}

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
            "open" => {
                let url = match required_string(&args, "url") {
                    Ok(url) => url,
                    Err(detail) => return Ok(error_result(&action, detail)),
                };
                match self.backend.open(url).await {
                    Ok(data) => ok_result(&action, data),
                    Err(detail) => error_result(&action, detail),
                }
            }
            "goto" => {
                let url = match required_string(&args, "url") {
                    Ok(url) => url,
                    Err(detail) => return Ok(error_result(&action, detail)),
                };
                match self.backend.goto(url).await {
                    Ok(data) => ok_result(&action, data),
                    Err(detail) => error_result(&action, detail),
                }
            }
            "scroll" => {
                let direction = match required_direction(&args) {
                    Ok(direction) => direction,
                    Err(detail) => return Ok(error_result(&action, detail)),
                };
                let amount = match required_i32(&args, "amount") {
                    Ok(amount) if amount > 0 => amount,
                    Ok(_) => return Ok(error_result(&action, "Field 'amount' must be greater than 0".to_string())),
                    Err(detail) => return Ok(error_result(&action, detail)),
                };

                match self.backend.scroll(direction, amount).await {
                    Ok(data) => ok_result(&action, data),
                    Err(detail) => error_result(&action, detail),
                }
            }
            "click" => {
                let selector = match required_string(&args, "selector") {
                    Ok(selector) => selector,
                    Err(detail) => return Ok(error_result(&action, detail)),
                };
                match self.backend.click(selector).await {
                    Ok(data) => ok_result(&action, data),
                    Err(detail) => error_result(&action, detail),
                }
            }
            "type_text" => {
                let selector = match required_string(&args, "selector") {
                    Ok(selector) => selector,
                    Err(detail) => return Ok(error_result(&action, detail)),
                };
                let text = match required_string(&args, "text") {
                    Ok(text) => text,
                    Err(detail) => return Ok(error_result(&action, detail)),
                };

                match self.backend.type_text(selector, text).await {
                    Ok(data) => ok_result(&action, data),
                    Err(detail) => error_result(&action, detail),
                }
            }
            "read_text" => {
                let selector = match required_string(&args, "selector") {
                    Ok(selector) => selector,
                    Err(detail) => return Ok(error_result(&action, detail)),
                };
                match self.backend.read_text(selector).await {
                    Ok(data) => ok_result(&action, data),
                    Err(detail) => error_result(&action, detail),
                }
            }
            "read_page" => match self.backend.read_page().await {
                Ok(data) => ok_result(&action, data),
                Err(detail) => error_result(&action, detail),
            },
            "screenshot" => match self.backend.screenshot().await {
                Ok(data) => ok_result(&action, data),
                Err(detail) => error_result(&action, detail),
            },
            "tab_list" => match self.backend.tab_list().await {
                Ok(data) => ok_result(&action, data),
                Err(detail) => error_result(&action, detail),
            },
            "tab_switch" => {
                let index = match required_index(&args) {
                    Ok(index) => index,
                    Err(detail) => return Ok(error_result(&action, detail)),
                };
                match self.backend.tab_switch(index).await {
                    Ok(data) => ok_result(&action, data),
                    Err(detail) => error_result(&action, detail),
                }
            }
            "tab_close" => {
                let index = match required_index(&args) {
                    Ok(index) => index,
                    Err(detail) => return Ok(error_result(&action, detail)),
                };
                match self.backend.tab_close(index).await {
                    Ok(data) => ok_result(&action, data),
                    Err(detail) => error_result(&action, detail),
                }
            }
            "back" => match self.backend.back().await {
                Ok(data) => ok_result(&action, data),
                Err(detail) => error_result(&action, detail),
            },
            "forward" => match self.backend.forward().await {
                Ok(data) => ok_result(&action, data),
                Err(detail) => error_result(&action, detail),
            },
            _ => error_result(&action, format!("Unsupported browser action: {action}")),
        };

        Ok(result)
    }
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

fn spawn_browser_handler(handler: chromiumoxide::handler::Handler) {
    Handle::current().spawn(async move {
        use futures::StreamExt;

        let mut handler = handler;
        while handler.next().await.is_some() {}
    });
}
