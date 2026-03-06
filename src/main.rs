mod config;

use async_trait::async_trait;
use forja_core::error::Result;
use forja_core::traits::LlmProvider;
use forja_core::{Channel, Content, Engine, Message, Role, ToolDefinition};
use forja_llm::LlmClient;
use std::pin::Pin;
use std::sync::Arc;
use tokio::io::{self, AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio_stream::{Stream, StreamExt};
use std::io::Write;
use forja_tools::{FileTool, WebTool, ShellTool, SearchTool, SearchProvider, StdinConfirmation};
use forja_memory::MarkdownMemoryStore;

// ─── 터미널 채널 ────────────────────────────────────────────────────────────

struct CliChannel;

#[async_trait]
impl Channel for CliChannel {
    async fn receive(&self) -> Result<Message> {
        let mut stdout = io::stdout();
        stdout.write_all(b"\nUser: ").await.unwrap();
        stdout.flush().await.unwrap();

        let stdin = io::stdin();
        let mut reader = BufReader::new(stdin);
        let mut input = String::new();
        reader.read_line(&mut input).await.unwrap();

        let text = input.trim().to_string();
        Ok(Message::text(Role::User, text))
    }

    async fn send(&self, message: Message) -> Result<()> {
        if message.role == Role::Assistant {
            let content = match &message.content {
                Content::Text { text } => text.clone(),
                _ => "(Unknown content)".to_string(),
            };
            // 스트리밍과 동일한 형식: 완료 후 줄바꿈 2번
            println!("\n🤖 Assistant: {}\n", content);
        }
        Ok(())
    }
}

// ─── Mock LLM (API 키 없이 로컬 테스트용) ────────────────────────────────────

struct MockLlmProvider;

#[async_trait]
impl LlmProvider for MockLlmProvider {
    async fn chat(&self, messages: &[Message], _tools: Option<&[ToolDefinition]>) -> Result<Message> {
        let last = messages.iter().rev()
            .find(|m| m.role == Role::User)
            .map(|m| match &m.content {
                Content::Text { text } => text.clone(),
                _ => "(no text)".to_string(),
            })
            .unwrap_or_default();

        Ok(Message::text(
            Role::Assistant,
            format!(
                "[MockLLM] 메시지를 받았습니다: '{}' (실제 API 키를 설정하면 진짜 응답을 받을 수 있습니다.)",
                last
            )
        ))
    }

    async fn stream(
        &self,
        messages: &[Message],
        _tools: Option<&[ToolDefinition]>,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<String>> + Send>>> {
        let last = messages.iter().rev()
            .find(|m| m.role == Role::User)
            .map(|m| match &m.content {
                Content::Text { text } => text.clone(),
                _ => "(no text)".to_string(),
            })
            .unwrap_or_default();

        let response = format!(
            "[MockStream] 메시지를 받았습니다: '{}' (타이핑 효과 테스트 중...)",
            last
        );

        // 단어 단위로 쪼개어 스트림 생성
        let tokens: Vec<String> = response.split(' ')
            .map(|s| format!("{} ", s))
            .collect();

        let stream = tokio_stream::iter(tokens).map(Ok);
        Ok(Box::pin(stream))
    }
}

// ─── 배너 ───────────────────────────────────────────────────────────────────

fn print_banner(provider_info: &str) {
    let banner = r#"
    ╔═══════════════════════════════════════╗
    ║                                       ║
    ║     ⚒️  F O R J A                      ║
    ║     Lightweight AI Agent Engine       ║
    ║     v0.1.0                            ║
    ║                                       ║
    ╚═══════════════════════════════════════╝"#;
    println!("{}", banner);
    println!("    {}\n", provider_info);
}

// ─── 진입점 ─────────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() -> Result<()> {
    ctrlc::set_handler(move || {
        println!("\n[System] Exiting...");
        std::process::exit(0);
    }).expect("Error setting Ctrl+C handler");

    // ── CLI 인수 파싱 (std::env::args) ──
    let args: Vec<String> = std::env::args().collect();
    let mut force_setup = false;
    let mut new_provider = None;
    let mut new_model = None;
    let mut channel_arg = None;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--setup" => force_setup = true,
            "--provider" => {
                if i + 1 < args.len() {
                    new_provider = Some(args[i + 1].clone());
                    i += 1;
                }
            }
            "--model" => {
                if i + 1 < args.len() {
                    new_model = Some(args[i + 1].clone());
                    i += 1;
                }
            }
            "--channel" => {
                if i + 1 < args.len() {
                    channel_arg = Some(args[i + 1].clone());
                    i += 1;
                }
            }
            _ => {}
        }
        i += 1;
    }

    // ── config 로드 ──
    let mut forja_cfg = if force_setup {
        config::run_onboarding()
    } else {
        config::load_config()
    };

    // 설정 파일이 없고 환경변수도 없으면 자동 온보딩 (load_config 내부에서 처리하지 않고 명시적으로 수행)
    if forja_cfg.active.provider.is_none() && !force_setup {
        forja_cfg = config::run_onboarding();
    }

    // 플래그 적용
    let mut updated = false;
    if let Some(p) = new_provider {
        println!("[System] Switching provider to: {}", p);
        forja_cfg.active.provider = Some(p.clone());
        
        // 키가 없는 경우 즉석에서 요청
        if forja_cfg.keys.get_for(&p).is_none() && p != "ollama" {
            print!("\n⚠️  {}의 API 키가 없습니다. 입력하세요 > ", p);
            std::io::stdout().flush().ok();
            let mut key = String::new();
            std::io::stdin().read_line(&mut key).ok();
            let trimmed = key.trim().to_string();
            if !trimmed.is_empty() {
                forja_cfg.keys.set_for(&p, trimmed);
            }
        }
        updated = true;
    }
    if let Some(m) = new_model {
        println!("[System] Setting model to: {}", m);
        forja_cfg.active.model = Some(m);
        updated = true;
    }

    if updated {
        config::save_config(&forja_cfg).ok();
    }

    let info = config::provider_info(&forja_cfg);
    print_banner(&info);

    // ── Mock 모드 or 실제 프로바이더 ──
    let use_mock = std::env::var("FORJA_USE_MOCK").is_ok();
    let provider: Arc<dyn LlmProvider> = if use_mock {
        println!("[System] MockLlmProvider 모드 (실제 LLM 호출 없음)");
        Arc::new(MockLlmProvider)
    } else {
        let llm_config = config::llm_config_from(&forja_cfg)
            .map_err(forja_core::error::ForjaError::LlmError)?;
        Arc::new(LlmClient::new(llm_config)?)
    };

    // ── 채널 스위칭 ──
    let channel: Arc<dyn Channel> = if channel_arg.as_deref() == Some("telegram") {
        #[cfg(feature = "telegram")]
        {
            let bot_token = forja_cfg.channel.telegram.bot_token.clone()
                .or_else(|| std::env::var("TELEGRAM_BOT_TOKEN").ok());
            
            if let Some(token) = bot_token {
                let allowed = forja_cfg.channel.telegram.allowed_chat_ids.clone();
                if allowed.is_empty() {
                    println!("[WARN] Telegram allowed_chat_ids is empty. Anyone can talk to this bot.");
                } else {
                    println!("[System] Telegram Bot starting with allowed IDs: {:?}", allowed);
                }
                Arc::new(forja_channel::telegram::TelegramChannel::new(token, allowed).await)
            } else {
                eprintln!("[Error] Telegram bot token not found in config(bot_token) or TELEGRAM_BOT_TOKEN.");
                std::process::exit(1);
            }
        }
        #[cfg(not(feature = "telegram"))]
        {
            eprintln!("[Error] Engine was not built with telegram feature. Use `cargo run --features telegram`.");
            std::process::exit(1);
        }
    } else {
        Arc::new(CliChannel)
    };

    // ── System Prompt 설정 ──
    // ── System Prompt 설정 ──
    let today = chrono::Local::now().format("%Y년 %m월 %d일").to_string();
    let base_prompt = forja_cfg.agent.system_prompt
        .unwrap_or_else(|| "You are Forja, a lightweight AI agent engine. 반드시 한국어로 답변하세요. 검색 도구가 실패하면 정보를 지어내지 말고, 실패했다고 솔직하게 알려주세요.".to_string());

    let system_prompt = format!(
        "{}\n\n오늘 날짜는 {}입니다. 이 날짜는 정확하며 의심하지 마세요. 검색 결과의 날짜가 오늘과 일치하면 최신 정보입니다.",
        base_prompt, today
    );

    // ── 메모리 스토어 초기화 ──
    let memory_dir = dirs_next::home_dir()
        .unwrap_or_default()
        .join(".forja")
        .join("memory");
    let memory_store = Arc::new(
        MarkdownMemoryStore::new(memory_dir).await
            .expect("Failed to initialize memory store")
    );

    let mut engine = Engine::new(provider, channel)
        .with_system_prompt(system_prompt)
        .with_claude_md()
        .with_memory(memory_store);

    // ── 도구 등록 ──
    let file_tool = Arc::new(FileTool::new());
    let web_tool = Arc::new(WebTool::new());
    let shell_tool = Arc::new(ShellTool::new(Arc::new(StdinConfirmation::new())));

    let search_provider = match forja_cfg.tools.search.provider.as_deref() {
        Some("brave") => {
            let key = forja_cfg.tools.search.brave_api_key.clone().unwrap_or_default();
            SearchProvider::Brave { api_key: key }
        }
        Some("grok") | Some("xai") => {
            let key = forja_cfg.tools.search.xai_api_key.clone().unwrap_or_default();
            SearchProvider::Grok { api_key: key }
        }
        _ => SearchProvider::DuckDuckGo,
    };
    let search_tool = Arc::new(SearchTool::new(search_provider));

    engine.register_tool(file_tool);
    engine.register_tool(web_tool);
    engine.register_tool(shell_tool);
    engine.register_tool(search_tool);

    println!("[System] Engine is ready. Press Ctrl+C to quit.\n");

    engine.run_streaming(async {
        let _ = tokio::signal::ctrl_c().await;
    }).await?;

    Ok(())
}
