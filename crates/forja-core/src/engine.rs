use crate::error::{ForjaError, Result};
use crate::traits::{Channel, LlmProvider, Tool};
use crate::types::{Content, Message, Role, ToolDefinition};
use std::collections::HashMap;
use std::sync::Arc;

#[cfg(feature = "memory")]
use crate::traits::MemoryStore;

const MAX_TOOL_DEPTH: usize = 10;

const DEFAULT_SYSTEM_PROMPT: &str =
    "You are Forja, a lightweight AI agent engine.";

/// Forja의 핵심 엔진 코어
///
/// 채널(Channel), LLM 프로바이더(LlmProvider), 도구(Tool)를 조율하고
/// 메인 이벤트 루프 및 도구 호출의 재귀적 평가(handle_step)를 담당합니다.
pub struct Engine {
    provider: Arc<dyn LlmProvider>,
    channel: Arc<dyn Channel>,
    tools: HashMap<String, Arc<dyn Tool>>,
    conversation_history: Vec<Message>,
    max_history: usize,
    system_prompt: String,

    #[cfg(feature = "memory")]
    memory: Option<Arc<dyn MemoryStore>>,
}

impl Engine {
    pub fn new(provider: Arc<dyn LlmProvider>, channel: Arc<dyn Channel>) -> Self {
        let mut engine = Self {
            provider,
            channel,
            tools: HashMap::new(),
            conversation_history: Vec::new(),
            max_history: 100,
            system_prompt: DEFAULT_SYSTEM_PROMPT.to_string(),
            #[cfg(feature = "memory")]
            memory: None,
        };
        // 기본 System 프롬프트를 대화 기록에 삽입
        engine.inject_system_prompt();
        engine
    }

    /// 커스텀 System Prompt를 설정하고 대화록을 리셋합니다.
    pub fn with_system_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.system_prompt = prompt.into();
        // 기존 System 메시지 제거 후 재삽입
        self.conversation_history.retain(|m| m.role != Role::System);
        self.inject_system_prompt();
        self
    }

    /// CLAUDE.md 파일이 현재 디렉토리에 있으면 내용을 System Prompt에 append합니다.
    pub fn with_claude_md(mut self) -> Self {
        if let Ok(content) = std::fs::read_to_string("CLAUDE.md")
            && !content.trim().is_empty() {
                self.system_prompt = format!("{}

---
{}", self.system_prompt, content.trim());
                // 기존 System 메시지 제거 후 재삽입
                self.conversation_history.retain(|m| m.role != Role::System);
                self.inject_system_prompt();
            }
        self
    }

    /// 현재 `system_prompt` 필드값으로 System 메시지를 history에 삽입.
    fn inject_system_prompt(&mut self) {
        let sys_msg = Message::text(Role::System, &self.system_prompt);
        self.conversation_history.insert(0, sys_msg);
    }

    /// (선택) 메모리 저장소 연동 확장 메서드
    #[cfg(feature = "memory")]
    pub fn with_memory(mut self, memory: Arc<dyn MemoryStore>) -> Self {
        self.memory = Some(memory);
        self
    }

    /// 외부에서 엔진에 도구를 등록합니다.
    pub fn register_tool(&mut self, tool: Arc<dyn Tool>) {
        self.tools.insert(tool.name().to_string(), tool);
    }

    /// 대화 히스토리에 새 메시지를 추가하고,
    /// 허용된 윈도우(max_history) 초과 시 System 메시지를 보존한 채로 컴팩션합니다.
    fn push_message(&mut self, msg: Message) {
        self.conversation_history.push(msg);
        while self.conversation_history.len() > self.max_history {
            if let Some(pos) = self.conversation_history.iter().position(|m| m.role != Role::System) {
                self.conversation_history.remove(pos);
            } else {
                break;
            }
        }
    }

    /// 한 턴(step)을 평가하고 처리합니다.
    /// LLM의 응답이 ToolCall일 경우, 등록된 Tool을 실행한 뒤 결과를 추가하여
    /// LLM을 재귀 호출(handle_step)합니다.
    ///
    /// `MAX_TOOL_DEPTH`로 무한루프를 방어합니다.
    #[async_recursion::async_recursion]
    pub async fn handle_step(&mut self, depth: usize) -> Result<Message> {
        if depth >= MAX_TOOL_DEPTH {
            return Err(ForjaError::MaxDepthExceeded(MAX_TOOL_DEPTH));
        }

        // 등록된 모든 도구의 명세 수집
        let tool_defs: Vec<ToolDefinition> = self.tools.values()
            .map(|t| t.definition())
            .collect();
        let tools = if tool_defs.is_empty() { None } else { Some(tool_defs.as_slice()) };

        let response_msg = self.provider.chat(&self.conversation_history, tools).await?;

        match &response_msg.content {
            Content::ToolCall {
                call_id,
                tool_name,
                arguments,
                reasoning_content: _,
            } => {
                // LLM의 ToolCall 요청을 히스토리에 먼저 push
                self.push_message(response_msg.clone());

                let result = if let Some(tool) = self.tools.get(tool_name) {
                    tool.execute(arguments.clone()).await?
                } else {
                    serde_json::json!({
                        "error": format!("Unknown tool requested: {}", tool_name)
                    })
                };

                let result_msg = Message::tool_result(call_id, result);
                self.push_message(result_msg);

                // 결과 반환 후 LLM의 최종 해석을 위해 재귀 깊이를 증가(depth+1)하여 호출
                self.handle_step(depth + 1).await
            }
            _ => {
                // ToolCall이 아닌 경우(일반 Text 등), 턴을 종료
                self.push_message(response_msg.clone());
                Ok(response_msg)
            }
        }
    }

    /// 메인 이벤트 순환 루프.
    /// `runtime` feature 설정 시 제공되는 편의 메서드입니다.
    /// shutdown future 시그널을 통해 graceful하게 빠져나갑니다.
    #[cfg(feature = "runtime")]
    pub async fn run<F>(&mut self, shutdown: F) -> Result<()>
    where
        F: std::future::Future<Output = ()> + Send,
    {
        tokio::pin!(shutdown);

        loop {
            tokio::select! {
                // 종료 시그널 캐치 시 루프 탈출
                _ = &mut shutdown => {
                    break;
                }
                // 채널 입력을 무한정 수신 대기
                result = self.channel.receive() => {
                    let user_msg = result?;
                    self.push_message(user_msg.clone());

                    // LLM 프로바이더로 전달하여 한 턴 평가 (handle_step 내부에서 도구 명세 수집함)
                    let response = self.handle_step(0).await?;

                    // 채널로 최종 출력 결과 반환
                    self.channel.send(response.clone()).await?;

                    #[cfg(feature = "memory")]
                    if let Some(mem) = &self.memory {
                        use crate::types::MemoryEntry;
                        use std::time::{SystemTime, UNIX_EPOCH};

                        let now = SystemTime::now()
                            .duration_since(UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_secs();

                        if let Content::Text { text } = &user_msg.content {
                            let entry = MemoryEntry {
                                id: format!("user_{}", now),
                                timestamp: now,
                                tags: vec!["user".to_string()],
                                content: text.clone(),
                                score: 0.0,
                                metadata: Default::default(),
                            };
                            let _ = mem.save(&entry).await;
                        }

                        if let Content::Text { text } = &response.content {
                            let entry = MemoryEntry {
                                id: format!("assistant_{}", now + 1),
                                timestamp: now + 1,
                                tags: vec!["assistant".to_string()],
                                content: text.clone(),
                                score: 0.0,
                                metadata: Default::default(),
                            };
                            let _ = mem.save(&entry).await;
                        }
                    }

                    // 턴 종료 후 컨텍스트 윈도우 검사 (Auto-Flush)
                    #[cfg(feature = "memory")]
                    self.check_and_flush_context().await?;
                }
            }
        }

        Ok(())
    }

    /// 스트리밍 전용 메인 루프.
    /// 토큰이 하나씩 도착할 때마다 stdout에 즉시 출력칙(Claude Code 스타일).
    /// 스트리밍 실패 시 chat()으로 자동 폴백.
    #[cfg(feature = "runtime")]
    pub async fn run_streaming<F>(&mut self, shutdown: F) -> Result<()>
    where
        F: std::future::Future<Output = ()> + Send,
    {
        tokio::pin!(shutdown);

        loop {
            tokio::select! {
                _ = &mut shutdown => { break; }
                result = self.channel.receive() => {
                    let user_msg = result?;
                    
                    self.push_message(user_msg.clone());

                    // 스트리밍 + 폴백 전체 에러를 catch
                    let response_result = async {
                        // LLM 호출 (스트리밍 시도)
                        let streaming_result = self.stream_step_with_tools().await
                            .unwrap_or(None);

                        match streaming_result {
                            Some(text) => {
                                // 텍스트 스트리밍 성공
                                let response_msg = crate::types::Message::text(
                                    crate::types::Role::Assistant, &text
                                );
                                self.push_message(response_msg.clone());
                                
                                // CLI는 이미 스트리밍으로 출력됨 → 텔레그램만 send 출력, CLI는 프롬프트만 복원
                                self.channel.send(response_msg).await?;
                                
                                Ok::<Option<String>, crate::error::ForjaError>(Some(text))
                            }
                            None => {
                                use indicatif::{ProgressBar, ProgressStyle};
                                use std::time::Duration;

                                // 스트리밍 불가 시(도구 호출 등) 스피너 시작
                                let spinner = ProgressBar::new_spinner();
                                spinner.set_style(
                                    ProgressStyle::default_spinner()
                                        .tick_strings(&["⠋","⠙","⠹","⠸","⠼","⠴","⠦","⠧","⠇","⠏","✓"])
                                        .template("{spinner:.cyan} {msg}")
                                        .unwrap()
                                );
                                spinner.set_message("Thinking...");
                                spinner.enable_steady_tick(Duration::from_millis(80));

                                // 순수 텍스트 chat 폴백 호출 연산 (무거운 작업)
                                let final_msg = self.handle_step(0).await?;
                                
                                // 응답 도착 후 스피너 종료
                                spinner.finish_and_clear();

                                self.channel.send(final_msg.clone()).await?;
                                
                                Ok::<Option<String>, crate::error::ForjaError>(
                                    if let Content::Text { text } = &final_msg.content {
                                        Some(text.clone())
                                    } else {
                                        None
                                    }
                                )
                            }
                        }
                    }.await;

                    let final_assistant_text = match response_result {
                        Ok(text_opt) => text_opt,
                        Err(e) => {
                            let err_text = format!("⚠️ 에러 발생: {}", e);
                            eprintln!("[Engine Error] {}", err_text);
                            
                            // 토큰 초과 등의 경우 히스토리 초기화(System 봇 역할만 남김)
                            let err_str = e.to_string().to_lowercase();
                            if err_str.contains("token") || err_str.contains("limit") || err_str.contains("exceeded") || err_str.contains("context") {
                                self.conversation_history.retain(|m| m.role == crate::types::Role::System);
                            }
                            
                            // 텔레그램 등 채널로 에러 전송
                            let _ = self.channel.send(crate::types::Message::text(crate::types::Role::Assistant, err_text)).await;
                            None
                        }
                    };

                    #[cfg(feature = "memory")]
                    if let Some(mem) = &self.memory {
                        use crate::types::MemoryEntry;
                        use std::time::{SystemTime, UNIX_EPOCH};

                        let now = SystemTime::now()
                            .duration_since(UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_secs();

                        if let Content::Text { text } = &user_msg.content {
                            let entry = MemoryEntry {
                                id: format!("user_{}", now),
                                timestamp: now,
                                tags: vec!["user".to_string()],
                                content: text.clone(),
                                score: 0.0,
                                metadata: Default::default(),
                            };
                            let _ = mem.save(&entry).await;
                        }

                        if let Some(text) = final_assistant_text {
                            let entry = MemoryEntry {
                                id: format!("assistant_{}", now + 1),
                                timestamp: now + 1,
                                tags: vec!["assistant".to_string()],
                                content: text,
                                score: 0.0,
                                metadata: Default::default(),
                            };
                            let _ = mem.save(&entry).await;
                        }
                    }

                    #[cfg(feature = "memory")]
                    self.check_and_flush_context().await?;
                }
            }
        }

        Ok(())
    }

    /// 스트리밍 토큰을 stdout에 점진적으로 출력합니다. (도구 명세 포함)
    /// 성공 시 Some(full_text), 실패 시 Err 반환.
    #[cfg(feature = "runtime")]
    async fn stream_step_with_tools(&self) -> Result<Option<String>> {
        use tokio_stream::StreamExt;
        use indicatif::{ProgressBar, ProgressStyle};
        use std::time::Duration;

        let tool_defs: Vec<ToolDefinition> = self.tools.values()
            .map(|t| t.definition())
            .collect();
        let tools = if tool_defs.is_empty() { None } else { Some(tool_defs.as_slice()) };

        // 스피너 시작
        let spinner = ProgressBar::new_spinner();
        spinner.set_style(
            ProgressStyle::default_spinner()
                .tick_strings(&["⠋","⠙","⠹","⠸","⠼","⠴","⠦","⠧","⠇","⠏","✓"])
                .template("{spinner:.cyan} {msg}")
                .unwrap()
        );
        spinner.set_message("Thinking...");
        spinner.enable_steady_tick(Duration::from_millis(80));

        // 도구 명세를 포함하여 스트리밍 시도
        let mut stream = match self.provider.stream(&self.conversation_history, tools).await {
            Ok(s) => s,
            Err(_) => {
                spinner.finish_and_clear();
                return Ok(None); // 스트리밍 미지원 시 폴백
            }
        };

        let mut full_text = String::new();
        let mut first_token = true;

        while let Some(chunk) = stream.next().await {
            match chunk {
                Ok(token) => {
                    // 빈 토큰 무시
                    if token.is_empty() { continue; }

                    // tool call JSON이 감지되면 스트리밍 중단 → 폴백
                    if first_token && (token.trim_start().starts_with("{\"") || token.contains("tool_call")) {
                        spinner.finish_and_clear();
                        return Ok(None);
                    }
                    
                    if first_token {
                        if self.channel.is_cli_source() {
                            spinner.finish_and_clear(); // CLI는 출력 시작하므로 스피너 제거
                        }
                        self.channel.cancel_typing().await; // 텔레그램 등 타이핑 인디케이터 중단
                        first_token = false;
                    }

                    // CLI일 때만 터미널에 즉시 출력
                    if self.channel.is_cli_source() {
                        print!("{}", token);
                        std::io::Write::flush(&mut std::io::stdout()).ok();
                    }
                    full_text.push_str(&token);
                }
                Err(_) => break,
            }
        }

        if full_text.is_empty() {
            spinner.finish_and_clear();
            Ok(None)
        } else {
            spinner.finish_and_clear(); // 텔레그램처럼 루프 도중 스피너가 안 지워진 경우를 위해 최종 제거
            if self.channel.is_cli_source() {
                println!(); // 스트리밍 완료 후 줄바꿈
            }
            Ok(Some(full_text))
        }
    }

    /// 컨텍스트 윈도우 점검 후 임계치 초과시 Auto-Flush 및 다단계 스토리지 보관
    #[cfg(feature = "memory")]
    async fn check_and_flush_context(&mut self) -> Result<()> {
        let estimated_tokens: usize = self.conversation_history
            .iter()
            .map(|m| m.content_text_len() / 4)
            .sum();

        if estimated_tokens > 32_000 {
            if let Some(mem) = &self.memory {
                mem.flush().await?;
            }
            let drain_count = self.conversation_history.len() / 2;
            self.conversation_history.drain(0..drain_count);
        }
        Ok(())
    }
}
