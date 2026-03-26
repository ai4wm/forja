mod config;
mod provider_registry;
mod oauth;
mod bootstrap;

use async_trait::async_trait;
use forja_core::emotion::{
    EmotionEngine, MoodState, generate_startup_greeting,
    generate_startup_greeting_with_context,
};
use forja_core::error::{ForjaError, Result};
use forja_core::traits::{LlmProvider, MemoryStore};
use forja_core::{
    Channel, Content, Engine, KnowledgeManager, Message, Role, SerendipityEngine,
    ToolDefinition,
};
use forja_llm::LlmClient;
use std::pin::Pin;
use std::sync::Arc;
use tokio_stream::{Stream, StreamExt};
use std::io::Write;
use forja_tools::{FileTool, WebTool, ShellTool, SearchTool, SearchProvider, StdinConfirmation, ClaudeCodeTool, CodexTool, GeminiCliTool};
use forja_memory::MarkdownMemoryStore;
use provider_registry::ProviderRegistry;

// ─── Mock LLM (API 키 없이 로컬 테스트용) ────────────────────────────────────

struct MockLlmProvider;

#[async_trait]
impl LlmProvider for MockLlmProvider {
    async fn chat(&self, messages: &[Message], _tools: Option<&[ToolDefinition]>) -> Result<Message> {
        let last = messages.iter().rev()
            .find(|m| m.role == Role::User)
            .map(|m| match &m.content {
                Content::Text { text, .. } => text.clone(),
                _ => "(no text)".to_string(),
            })
            .unwrap_or_default();

        if last.contains("감정 상태를 JSON으로만 응답하세요.") {
            return Ok(Message::text(
                Role::Assistant,
                r#"{"mood":"neutral","intensity":1,"reason":"mock mode","tone_instruction":"균형 잡힌 존중의 톤으로 답하세요."}"#,
                None,
            ));
        }

        if last.contains("자연스럽게 건넬 인사를 한 문장으로 하세요.") {
            return Ok(Message::text(Role::Assistant, "NONE", None));
        }

        if last.contains("Also, if there are unfinished tasks or a useful daily summary") {
            return Ok(Message::text(Role::Assistant, "NONE", None));
        }

        if last.contains("하루치 memory.md 기록을 한국어로 최대 3줄로 요약하세요.") {
            return Ok(Message::text(Role::Assistant, "Mock 요약", None));
        }

        if last.contains("Below is the user's recent memory and knowledge base.") {
            return Ok(Message::text(Role::Assistant, "NONE", None));
        }

        Ok(Message::text(
            Role::Assistant,
            format!(
                "[MockLLM] 메시지를 받았습니다: '{}' (실제 API 키를 설정하면 진짜 응답을 받을 수 있습니다.)",
                last
            ),
            None
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
                Content::Text { text, .. } => text.clone(),
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

// ─── 유틸리티 함수: 프롬프트 파일 로드 ──────────────────────────────────────

/// 프로젝트 내 프롬프트 파일 로드 (우선순위: AGENTS.md -> FORJA.md -> CLAUDE.md)
fn load_project_prompt() -> Option<(String, String)> {
    let candidates = ["AGENTS.md", "FORJA.md", "CLAUDE.md"];
    for file in candidates.iter() {
        if let Ok(content) = std::fs::read_to_string(file)
            && !content.trim().is_empty() {
                return Some((file.to_string(), content.trim().to_string()));
            }
    }
    None
}

fn build_system_prompt(
    bootstrap_paths: &bootstrap::BootstrapPaths,
    shell_enabled: bool,
) -> std::io::Result<(String, Option<String>)> {
    let today = chrono::Local::now().format("%Y년 %m월 %d일").to_string();
    let bootstrap_prompt = bootstrap::compose_system_prompt_prefix(bootstrap_paths)?;
    let project_prompt = load_project_prompt();
    let loaded_project_file = project_prompt.as_ref().map(|(file_name, _)| file_name.clone());

    let mut combined_prompt = String::new();

    if !bootstrap_prompt.trim().is_empty() {
        combined_prompt.push_str(&bootstrap_prompt);
    }

    if let Some((_, project_content)) = project_prompt {
        if !combined_prompt.is_empty() {
            combined_prompt.push_str("\n\n---\n\n");
        }
        combined_prompt.push_str(&project_content);
    }

    if !combined_prompt.is_empty() {
        combined_prompt.push_str(&format!(
            "\n\n오늘 날짜는 {today}입니다. 이 날짜는 정확하며 의심하지 마세요. 검색 결과의 날짜가 오늘과 일치하면 최신 정보입니다."
        ));
    }

    if shell_enabled {
        if !combined_prompt.is_empty() {
            combined_prompt.push_str("\n\n---\n\n");
        }
        combined_prompt.push_str(
            "You have access to a shell tool that can execute OS commands.\n\
When the user asks you to perform a system task (open app,\n\
manage files, check system info, etc.), use the shell tool.\n\
For Windows, use PowerShell commands.\n\
For macOS/Linux, use bash commands.\n\
Always prefer safe, non-destructive commands.\n\
Example: user says 'open notepad' -> shell: Start-Process notepad\n\
Example: user says 'what time is it' -> shell: Get-Date\n\
Example: user says 'list files' -> shell: Get-ChildItem"
        );
    }

    Ok((combined_prompt, loaded_project_file))
}

fn auto_summarize_enabled() -> bool {
    !matches!(
        std::env::var("FORJA_AUTO_SUMMARIZE"),
        Ok(value) if value.eq_ignore_ascii_case("false")
    )
}

async fn summarize_memory_block(
    provider: Arc<dyn LlmProvider>,
    block: String,
) -> Result<String> {
    let response = provider
        .chat(
            &[
                Message::text(
                    Role::System,
                    "Summarize one daily memory.md block into at most three plain-text lines. Return only the summary lines.",
                    None,
                ),
                Message::text(
                    Role::User,
                    format!(
                        "아래 하루치 memory.md 기록을 한국어로 최대 3줄로 요약하세요.\n\
중요한 선호, 결정, 진행 중 작업만 남기고 군더더기 없이 평문으로만 답하세요.\n\
\n{block}"
                    ),
                    None,
                ),
            ],
            None,
        )
        .await?;

    match response.content {
        Content::Text { text, .. } => Ok(text),
        _ => Err(ForjaError::LlmError(
            "memory summary response was not text".to_string(),
        )),
    }
}

// ─── 진입점 ─────────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() >= 3 && args[1] == "login" {
        oauth::run_login(&args[2]).await;
        std::process::exit(0);
    } else if args.len() == 2 && args[1] == "login" {
        println!("사용법: forja login <provider>");
        println!("<provider> 가능한 옵션: openai, gemini, anthropic");
        std::process::exit(1);
    }

    let _auth_data = oauth::AuthData::load();

    ctrlc::set_handler(move || {
        println!("\n[System] Exiting...");
        std::process::exit(0);
    }).expect("Error setting Ctrl+C handler");

    // ── 서브커맨드 파싱 ──
    // let args: Vec<String> = std::env::args().collect(); // Already collected above

    // `forja setup` 서브커맨드: 이름된 후 종료
    if args.get(1).map(|s| s.as_str()) == Some("setup") {
        config::run_setup();
        return Ok(());
    }

    let mut force_setup = false;
    let mut new_provider = None;
    let mut new_model = None;

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

    let shell_enabled = !matches!(
        std::env::var("FORJA_SHELL"),
        Ok(value) if value.eq_ignore_ascii_case("false")
    );
    let bootstrap_paths = bootstrap::default_paths();
    let bootstrap_outcome = bootstrap::ensure_bootstrap(&bootstrap_paths)?;
    let (combined_prompt, loaded_project_file) =
        build_system_prompt(&bootstrap_paths, shell_enabled)?;
    if let Some(file_name) = loaded_project_file {
        println!("[System] {file_name} 로드됨");
    }

    // ── ProviderRegistry 초기화 ──
    let registry = ProviderRegistry::from_config(&forja_cfg);

    // ── 핸들러용 config clone (이후 forja_cfg 일부 필드가 이동되기 전에 복사) ──
    let cfg_for_handler = forja_cfg.clone();

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

    // ── 채널 설정 ──
    let (channel, interactive_identity_supported, print_initial_prompt): (
        Arc<dyn Channel>,
        bool,
        bool,
    ) = {
        #[cfg(feature = "telegram")]
        {
            let bot_token = forja_cfg.channel.telegram.bot_token.clone()
                .or_else(|| std::env::var("TELEGRAM_BOT_TOKEN").ok());

            if let Some(token) = bot_token {
                let allowed = forja_cfg.channel.telegram.allowed_chat_ids.clone();
                if allowed.is_empty() {
                    println!("[WARN] Telegram allowed_chat_ids is empty.");
                } else {
                    println!("[System] MultiChannel starting with CLI + Telegram (IDs: {:?})", allowed);
                }
                (
                    Arc::new(forja_channel::multi::MultiChannel::new_both(token, allowed).await),
                    false,
                    true,
                )
            } else {
                println!("[System] CLI mode (Telegram not configured)");
                (
                    Arc::new(forja_channel::cli::CliChannel::new()),
                    true,
                    false,
                )
            }
        }
        #[cfg(not(feature = "telegram"))]
        {
            (
                Arc::new(forja_channel::cli::CliChannel::new()),
                true,
                false,
            )
        }
    };

    // ── System Prompt 설정 ──
    let mut engine = Engine::new(provider.clone(), channel.clone());

    if !combined_prompt.is_empty() {
        engine = engine.with_system_prompt(combined_prompt);
    }

    let knowledge_dir = std::env::var("FORJA_KNOWLEDGE_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| bootstrap_paths.forja_dir.join("knowledge"));
    let knowledge_manager = Arc::new(KnowledgeManager::new(knowledge_dir));
    engine = engine.with_knowledge(knowledge_manager.clone());

    let serendipity_enabled = !matches!(
        std::env::var("FORJA_SERENDIPITY"),
        Ok(value) if value.eq_ignore_ascii_case("false")
    );
    if serendipity_enabled {
        engine = engine.with_serendipity(SerendipityEngine::new());
    }

    // ── 메모리 스토어 초기화 ──
    let memory_dir = std::env::var("FORJA_MEMORY_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| {
            dirs_next::home_dir()
                .unwrap_or_default()
                .join(".forja")
                .join("memory")
        });
    let memory_path = memory_dir.join("memory.md");
    let memory_store = Arc::new(MarkdownMemoryStore::new(memory_path).await?);

    if auto_summarize_enabled() {
        let summary_provider = provider.clone();
        if let Err(error) = memory_store
            .flush_and_summarize(|block: String| {
                tokio::task::block_in_place(|| {
                    tokio::runtime::Handle::current()
                        .block_on(summarize_memory_block(summary_provider.clone(), block))
                })
            })
            .await
        {
            eprintln!("[Memory] auto summarize failed: {error}");
        }
    }

    let memory_contents = match memory_store.load_all().await {
        Ok(contents) => contents,
        Err(error) => {
            eprintln!("[Memory] failed to load memory for emotion bootstrap: {error}");
            String::new()
        }
    };
    let knowledge_contents = match knowledge_manager.load_all_context() {
        Ok(contents) => contents,
        Err(error) => {
            eprintln!("[Knowledge] failed to load knowledge for startup greeting: {error}");
            String::new()
        }
    };
    let restored_mood = EmotionEngine::restore_from_memory(&memory_contents)
        .unwrap_or_else(MoodState::neutral);
    let startup_greeting = if serendipity_enabled {
        generate_startup_greeting_with_context(
            provider.as_ref(),
            &bootstrap_outcome.profile.identity.name,
            &bootstrap_outcome.profile.user.name,
            &memory_contents,
            &knowledge_contents,
            bootstrap_outcome.greeting.is_some(),
        )
        .await
        .unwrap_or(None)
    } else {
        generate_startup_greeting(
            provider.as_ref(),
            &bootstrap_outcome.profile.identity.name,
            &bootstrap_outcome.profile.user.name,
            &memory_contents,
            bootstrap_outcome.greeting.is_some(),
        )
        .await
        .unwrap_or(None)
    };
    engine = engine.with_emotion(EmotionEngine::new(restored_mood));


    // ── 도구 등록 ──
    let file_tool = Arc::new(FileTool::new());
    let web_tool = Arc::new(WebTool::new());
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
    engine.register_tool(search_tool);
    if shell_enabled {
        let shell_tool = Arc::new(ShellTool::new(Arc::new(StdinConfirmation::new())));
        engine.register_tool(shell_tool);
    } else {
        println!("[System] Shell tool disabled by FORJA_SHELL=false.");
    }

    if ClaudeCodeTool::is_installed().await {
        engine.register_tool(Arc::new(ClaudeCodeTool::new()));
        println!("[System] Claude Code tool registered.");
    }
    if CodexTool::is_installed().await {
        engine.register_tool(Arc::new(CodexTool::new()));
        println!("[System] Codex tool registered.");
    }
    if GeminiCliTool::is_installed().await {
        engine.register_tool(Arc::new(GeminiCliTool::new()));
        println!("[System] Gemini CLI tool registered.");
    }

    // ── 슬래시 핸들러: ProviderRegistry 를 캐폁한 클로저 ──
    let registry = std::sync::Mutex::new(registry);
    let channel_for_slash = channel.clone();
    let bootstrap_paths_for_slash = bootstrap_paths.clone();
    let shell_enabled_for_slash = shell_enabled;
    let slash_handler: forja_core::engine::SlashHandler = Arc::new(move |text: &str, provider: &mut Arc<dyn LlmProvider>| {
        let text = text.trim();

        if text == "/models" {
            let reg = registry.lock().unwrap();
            return Some(forja_core::engine::SlashCommandResult::Reply(
                reg.list_for_config(&cfg_for_handler),
            ));
        }

        if text == "/model" {
            let reg = registry.lock().unwrap();
            let e = reg.active();
            return Some(forja_core::engine::SlashCommandResult::Reply(format!(
                "현재 모델: **{}** ({}/{})",
                e.label, e.provider, e.model_id
            )));
        }

        if let Some(target) = text.strip_prefix("/model ") {
            let mut reg = registry.lock().unwrap();
            match reg.resolve(target, &cfg_for_handler) {
                None => {
                    return Some(forja_core::engine::SlashCommandResult::Reply(format!(
                        "❌ '{}' 모델을 찾을 수 없습니다. `/models`로 목록을 확인하세요.",
                        target
                    )))
                }
                Some(idx) => {
                    match reg.switch_to(idx, &cfg_for_handler) {
                        Err(e) => {
                            return Some(forja_core::engine::SlashCommandResult::Reply(
                                format!("❌ 전환 실패: {e}"),
                            ))
                        }
                        Ok(new_config) => {
                            match forja_llm::LlmClient::new(new_config) {
                                Err(e) => {
                                    return Some(forja_core::engine::SlashCommandResult::Reply(
                                        format!("❌ LlmClient 생성 실패: {e}"),
                                    ))
                                }
                                Ok(client) => {
                                    let entry = reg.active();
                                    *provider = Arc::new(client);
                                    return Some(forja_core::engine::SlashCommandResult::Reply(format!(
                                        "✅ 모델 전환: **{}** ({}/{})",
                                        entry.label, entry.provider, entry.model_id
                                    )));
                                }
                            }
                        }
                    }
                }
            }
        }

        if text == "/identity" {
            if !interactive_identity_supported || !channel_for_slash.is_cli_source() {
                return Some(forja_core::engine::SlashCommandResult::Reply(
                    "이 명령은 CLI 단독 모드에서만 지원됩니다.".to_string(),
                ));
            }

            let outcome = match bootstrap::reset_bootstrap(&bootstrap_paths_for_slash) {
                Ok(outcome) => outcome,
                Err(error) => {
                    return Some(forja_core::engine::SlashCommandResult::Reply(format!(
                        "❌ identity 재설정 실패: {error}"
                    )))
                }
            };

            let system_prompt = match build_system_prompt(&bootstrap_paths_for_slash, shell_enabled_for_slash) {
                Ok((system_prompt, _)) => system_prompt,
                Err(error) => {
                    return Some(forja_core::engine::SlashCommandResult::Reply(format!(
                        "❌ 시스템 프롬프트 재구성 실패: {error}"
                    )))
                }
            };

            return Some(forja_core::engine::SlashCommandResult::UpdateSystemPrompt {
                reply: outcome.profile.greeting(),
                system_prompt: Some(system_prompt),
                reset_history: true,
            });
        }

        None
    });

    let identity_name = bootstrap_outcome.profile.identity.name.clone();
    let displayed_greeting = bootstrap_outcome.greeting.or(startup_greeting);
    let mut engine = engine.with_memory(memory_store).with_slash_handler(slash_handler);

    println!("[System] Engine is ready. Type /models to list models, /model <name> to switch.");
    if let Some(greeting) = displayed_greeting {
        println!();
        println!("{identity_name}: {greeting}");
    }
    if print_initial_prompt {
        print!("\n> ");
        std::io::stdout().flush().ok();
    }

    engine.run_streaming(async {
        let _ = tokio::signal::ctrl_c().await;
    }).await?;

    Ok(())
}
