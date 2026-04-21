use super::backend::BrowserBackend;
use async_trait::async_trait;

#[cfg(feature = "browser")]
use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
#[cfg(feature = "browser")]
use chromiumoxide::browser::{Browser, BrowserConfig};
#[cfg(feature = "browser")]
use chromiumoxide::cdp::browser_protocol::page::CaptureScreenshotFormat;
#[cfg(feature = "browser")]
use chromiumoxide::page::ScreenshotParams;
#[cfg(feature = "browser")]
use chromiumoxide::Page;
#[cfg(feature = "browser")]
use tokio::runtime::Handle;

#[cfg(feature = "browser")]
struct ChromiumSession {
    browser: Browser,
    current_page: Option<Page>,
}

pub struct ChromiumBackend {
    port: u16,
    browser_path: Option<String>,
    #[cfg(feature = "browser")]
    session: tokio::sync::Mutex<Option<ChromiumSession>>,
}

impl ChromiumBackend {
    pub fn new() -> Self {
        Self {
            port: std::env::var("FORJA_BROWSER_PORT")
                .ok()
                .and_then(|value| value.parse::<u16>().ok())
                .filter(|value| *value > 0)
                .unwrap_or(9222),
            browser_path: std::env::var("FORJA_BROWSER_PATH")
                .ok()
                .map(|path| path.trim().to_string())
                .filter(|path| !path.is_empty()),
            #[cfg(feature = "browser")]
            session: tokio::sync::Mutex::new(None),
        }
    }

    #[cfg(feature = "browser")]
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

    #[cfg(feature = "browser")]
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

    #[cfg(not(feature = "browser"))]
    fn feature_disabled_error(&self) -> String {
        let _ = (&self.port, &self.browser_path);
        "Browser backend is unavailable because the 'browser' feature is disabled".to_string()
    }
}

impl Default for ChromiumBackend {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(feature = "browser")]
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

#[cfg(not(feature = "browser"))]
#[async_trait]
impl BrowserBackend for ChromiumBackend {
    async fn open(&self, _url: &str) -> std::result::Result<String, String> {
        Err(self.feature_disabled_error())
    }

    async fn goto(&self, _url: &str) -> std::result::Result<String, String> {
        Err(self.feature_disabled_error())
    }

    async fn scroll(
        &self,
        _direction: &str,
        _amount: i32,
    ) -> std::result::Result<String, String> {
        Err(self.feature_disabled_error())
    }

    async fn click(&self, _selector: &str) -> std::result::Result<String, String> {
        Err(self.feature_disabled_error())
    }

    async fn type_text(
        &self,
        _selector: &str,
        _text: &str,
    ) -> std::result::Result<String, String> {
        Err(self.feature_disabled_error())
    }

    async fn read_text(&self, _selector: &str) -> std::result::Result<String, String> {
        Err(self.feature_disabled_error())
    }

    async fn read_page(&self) -> std::result::Result<String, String> {
        Err(self.feature_disabled_error())
    }

    async fn screenshot(&self) -> std::result::Result<String, String> {
        Err(self.feature_disabled_error())
    }

    async fn evaluate(&self, _js: &str) -> std::result::Result<String, String> {
        Err(self.feature_disabled_error())
    }

    async fn tab_list(&self) -> std::result::Result<String, String> {
        Err(self.feature_disabled_error())
    }

    async fn tab_switch(&self, _index: usize) -> std::result::Result<String, String> {
        Err(self.feature_disabled_error())
    }

    async fn tab_close(&self, _index: usize) -> std::result::Result<String, String> {
        Err(self.feature_disabled_error())
    }

    async fn back(&self) -> std::result::Result<String, String> {
        Err(self.feature_disabled_error())
    }

    async fn forward(&self) -> std::result::Result<String, String> {
        Err(self.feature_disabled_error())
    }
}

#[cfg(feature = "browser")]
fn spawn_browser_handler(handler: chromiumoxide::handler::Handler) {
    Handle::current().spawn(async move {
        use futures::StreamExt;

        let mut handler = handler;
        while handler.next().await.is_some() {}
    });
}
