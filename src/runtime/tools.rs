use crate::config::ForjaConfig;
use forja_core::Engine;
use forja_core::mode::ExecMode;
use forja_llm::LlmConfig;
#[cfg(feature = "vision")]
use forja_tools::XcapBackend;
use forja_tools::{
    BrowserTool, ClaudeCodeTool, CodexTool, FileTool, GeminiCliTool, GptVisionAnalyzer, InputTool,
    MockCaptureBackend, MockVisionAnalyzer, ScreenCaptureBackend, SearchProvider, SearchTool,
    ShellTool, StdinConfirmation, VisionAnalyzer, VisionTool, WebTool, browser::MockBrowserBackend,
};
use std::sync::{Arc, Mutex};

pub(crate) struct ToolRuntime {
    pub(crate) capture_backend: Arc<dyn ScreenCaptureBackend>,
    pub(crate) vision_analyzer: Arc<dyn VisionAnalyzer>,
}

pub(crate) struct ToolRegistrationContext<'a> {
    pub(crate) forja_cfg: &'a ForjaConfig,
    pub(crate) exec_mode_handle: Arc<Mutex<ExecMode>>,
    pub(crate) llm_config: Option<&'a LlmConfig>,
    pub(crate) use_mock: bool,
    pub(crate) shell_enabled: bool,
    pub(crate) input_enabled: bool,
    pub(crate) browser_enabled: bool,
    pub(crate) vision_enabled: bool,
}

pub(crate) async fn register_tools(
    engine: &mut Engine,
    context: ToolRegistrationContext<'_>,
) -> ToolRuntime {
    let capture_backend: Arc<dyn ScreenCaptureBackend> = if context.use_mock {
        Arc::new(MockCaptureBackend::new())
    } else {
        #[cfg(feature = "vision")]
        {
            Arc::new(XcapBackend::new())
        }
        #[cfg(not(feature = "vision"))]
        {
            Arc::new(MockCaptureBackend::new())
        }
    };
    let vision_analyzer: Arc<dyn VisionAnalyzer> = if context.use_mock {
        Arc::new(MockVisionAnalyzer::new())
    } else {
        let llm_config = context
            .llm_config
            .expect("llm_config must exist when not in mock mode");
        Arc::new(GptVisionAnalyzer::new(
            llm_config.base_url.clone(),
            llm_config.api_key.clone(),
            llm_config.model.clone(),
        ))
    };

    let file_tool = Arc::new(FileTool::new());
    let web_tool = Arc::new(WebTool::new());
    let search_provider = match context.forja_cfg.tools.search.provider.as_deref() {
        Some("brave") => SearchProvider::Brave {
            api_key: context
                .forja_cfg
                .tools
                .search
                .brave_api_key
                .clone()
                .unwrap_or_default(),
        },
        Some("grok") | Some("xai") => SearchProvider::Grok {
            api_key: context
                .forja_cfg
                .tools
                .search
                .xai_api_key
                .clone()
                .unwrap_or_default(),
        },
        _ => SearchProvider::DuckDuckGo,
    };
    let search_tool = Arc::new(SearchTool::new(search_provider));

    engine.register_tool(file_tool);
    engine.register_tool(web_tool);
    engine.register_tool(search_tool);

    if context.shell_enabled {
        let shell_tool = Arc::new(ShellTool::new(Arc::new(StdinConfirmation::from_shared(
            context.exec_mode_handle.clone(),
        ))));
        engine.register_tool(shell_tool);
    } else {
        println!("Shell tool disabled by FORJA_SHELL=false.");
    }

    if context.input_enabled {
        match InputTool::new(Arc::new(StdinConfirmation::from_shared(
            context.exec_mode_handle.clone(),
        ))) {
            Ok(input_tool) => engine.register_tool(Arc::new(input_tool)),
            Err(error) => eprintln!("Input tool initialization failed: {error}"),
        }
    } else {
        println!("Input tool disabled by FORJA_INPUT=false.");
    }

    if context.browser_enabled {
        let confirmation = Arc::new(StdinConfirmation::from_shared(
            context.exec_mode_handle.clone(),
        ));
        if context.use_mock {
            let browser_tool = BrowserTool::with_backend_and_settings(
                Arc::new(MockBrowserBackend::new()),
                confirmation,
                false,
            );
            engine.register_tool(Arc::new(browser_tool));
        } else {
            let browser_tool = BrowserTool::new(confirmation);
            engine.register_tool(Arc::new(browser_tool));
        }
    } else {
        println!("Browser tool disabled by FORJA_BROWSER=false.");
    }

    if context.vision_enabled {
        let vision_tool =
            VisionTool::with_backends(capture_backend.clone(), vision_analyzer.clone(), false);
        engine.register_tool(Arc::new(vision_tool));
    } else {
        println!("Vision tool disabled by FORJA_VISION=false.");
    }

    if ClaudeCodeTool::is_installed().await {
        engine.register_tool(Arc::new(ClaudeCodeTool::new()));
        println!("Claude Code tool registered.");
    }
    if CodexTool::is_installed().await {
        engine.register_tool(Arc::new(CodexTool::new()));
        println!("Codex tool registered.");
    }
    if GeminiCliTool::is_installed().await {
        engine.register_tool(Arc::new(GeminiCliTool::new()));
        println!("Gemini CLI tool registered.");
    }

    ToolRuntime {
        capture_backend,
        vision_analyzer,
    }
}
