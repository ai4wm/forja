use crate::error::{ForjaError, Result};
use crate::traits::{Channel, LlmProvider, Tool};
use crate::types::{Content, Message, Role, ToolDefinition};
use std::collections::HashMap;
use std::sync::Arc;

#[cfg(feature = "memory")]
mod memory;

#[cfg(feature = "memory")]
use crate::traits::MemoryStore;

const MAX_TOOL_DEPTH: usize = 10;

pub enum SlashCommandResult {
    Reply(String),
    UpdateSystemPrompt {
        reply: String,
        system_prompt: Option<String>,
        reset_history: bool,
    },
}

/// /models, /model 슬래시 명령 처리용 콜백 타입
pub type SlashHandler =
    Arc<dyn Fn(&str, &mut Arc<dyn LlmProvider>) -> Option<SlashCommandResult> + Send + Sync>;

/// Forja의 핵심 엔진 코어
///
/// 채널(Channel), LLM 프로바이더(LlmProvider), 도구(Tool)를 조율하고
/// 메인 이벤트 루프 및 도구 호출의 재귀적 평가(handle_step)를 담당합니다.
pub struct Engine {
    provider: Arc<dyn LlmProvider>,
    #[cfg_attr(not(feature = "runtime"), allow(dead_code))]
    channel: Arc<dyn Channel>,
    tools: HashMap<String, Arc<dyn Tool>>,
    conversation_history: Vec<Message>,
    max_history: usize,
    system_prompt: Option<String>,
    slash_handler: Option<SlashHandler>,

    #[cfg(feature = "memory")]
    memory: Option<Arc<dyn MemoryStore>>,
    #[cfg(feature = "memory")]
    turn_memory_context: Option<String>,
}

impl Engine {
    pub fn new(provider: Arc<dyn LlmProvider>, channel: Arc<dyn Channel>) -> Self {
        
        Self {
            provider: provider.clone(),
            channel,
            tools: HashMap::new(),
            conversation_history: Vec::new(),
            max_history: 100,
            system_prompt: None,
            slash_handler: None,
            #[cfg(feature = "memory")]
            memory: None,
            #[cfg(feature = "memory")]
            turn_memory_context: None,
        }
    }

    /// 커스텀 System Prompt를 설정합니다. (history 주입은 메시지 수신 시 처리)
    pub fn with_system_prompt(mut self, prompt: String) -> Self {
        self.system_prompt = Some(prompt);
        self
    }

    /// (선택) 메모리 저장소 연동 확장 메서드
    #[cfg(feature = "memory")]
    pub fn with_memory(mut self, memory: Arc<dyn MemoryStore>) -> Self {
        self.memory = Some(memory);
        self
    }

    /// /models, /model 슬래시 명령 핸들러를 등록합니다.
    /// 콜백은 (입력 텍스트, 현재 provider mut ref) → Option<응답 텍스트> 형태입니다.
    pub fn with_slash_handler(mut self, handler: SlashHandler) -> Self {
        self.slash_handler = Some(handler);
        self
    }

    /// 외부에서 엔진에 도구를 등록합니다.
    pub fn register_tool(&mut self, tool: Arc<dyn Tool>) {
        self.tools.insert(tool.name().to_string(), tool);
    }

    /// 런타임에 LLM 프로바이더를 교체합니다 (예: /model 명령 처리).
    pub fn swap_provider(&mut self, new_provider: Arc<dyn LlmProvider>) {
        self.provider = new_provider;
    }

    /// 슬래시 명령('/...') 이면 Some(응답 텍스트)를, 아니면 None을 반환합니다.
    /// main.rs 에서 engine.run_streaming 외부 루프 대신 호출 가능.
    pub fn slash_response(&self, text: &str) -> Option<&'static str> {
        // 단순 감지만 수행, 실제 처리는 호출자가 담당
        if text.trim_start().starts_with('/') { Some("") } else { None }
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

    fn request_messages(&self) -> Vec<Message> {
        #[cfg(feature = "memory")]
        let mut messages = self.conversation_history.clone();
        #[cfg(not(feature = "memory"))]
        let messages = self.conversation_history.clone();

        #[cfg(feature = "memory")]
        if let Some(memory_context) = &self.turn_memory_context {
            let insertion_index = messages
                .iter()
                .take_while(|message| message.role == Role::System)
                .count();
            messages.insert(
                insertion_index,
                Message::text(Role::System, memory_context.clone(), None),
            );
        }

        messages
    }

    #[cfg(feature = "runtime")]
    fn apply_system_prompt_update(
        &mut self,
        next_system_prompt: Option<String>,
        reset_history: bool,
    ) {
        self.system_prompt = next_system_prompt.clone();

        if reset_history {
            self.conversation_history.clear();
        } else {
            self.conversation_history.retain(|message| message.role != Role::System);
        }

        if let Some(system_prompt) = next_system_prompt {
            let system_message = Message::text(Role::System, system_prompt, None);
            self.conversation_history.insert(0, system_message);
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

        let request_messages = self.request_messages();
        let response_msg = self.provider.chat(&request_messages, tools).await?;

        match &response_msg.content {
            Content::ToolCall {
                call_id,
                tool_name,
                arguments,
                reasoning_content: _,
                thought_signature: _,
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
                    
                    // 히스토리가 비어있으면 System 프롬프트 주입
                    if self.conversation_history.is_empty()
                        && let Some(prompt) = &self.system_prompt {
                            let sys_msg = Message::text(Role::System, prompt, None);
                            self.push_message(sys_msg);
                        }

                    #[cfg(feature = "memory")]
                    self.refresh_turn_memory_context(&user_msg).await;

                    self.push_message(user_msg.clone());

                    // LLM 프로바이더로 전달하여 한 턴 평가 (handle_step 내부에서 도구 명세 수집함)
                    let response = self.handle_step(0).await?;

                    // 채널로 최종 출력 결과 반환
                    self.channel.send(response.clone()).await?;

                    #[cfg(feature = "memory")]
                    {
                        let assistant_text = match &response.content {
                            Content::Text { text, .. } => Some(text.as_str()),
                            _ => None,
                        };
                        self.save_turn_memory_entries(&user_msg, assistant_text).await;
                        self.clear_turn_memory_context();
                        self.check_and_flush_context().await?;
                    }
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

                    // ── 슬래시 명령 가로채기 ───────────────────────────
                    let slash_reply = if let Content::Text { text, .. } = &user_msg.content {
                        if let Some(handler) = &self.slash_handler.clone() {
                            handler(text, &mut self.provider)
                        } else {
                            None
                        }
                    } else {
                        None
                    };

                    if let Some(slash_result) = slash_reply {
                        let reply = match slash_result {
                            SlashCommandResult::Reply(reply) => reply,
                            SlashCommandResult::UpdateSystemPrompt {
                                reply,
                                system_prompt,
                                reset_history,
                            } => {
                                self.apply_system_prompt_update(system_prompt, reset_history);
                                reply
                            }
                        };

                        let reply_msg = Message::text(Role::Assistant, &reply, None);
                        let _ = self.channel.send(reply_msg).await;
                        // 슬래시 명령은 대화 히스토리에 추가하지 않음
                        continue;
                    }

                    // 히스토리가 비어있으면 System 프롬프트 주입
                    if self.conversation_history.is_empty()
                        && let Some(prompt) = &self.system_prompt {
                            let sys_msg = Message::text(Role::System, prompt, None);
                            self.push_message(sys_msg);
                        }

                    #[cfg(feature = "memory")]
                    self.refresh_turn_memory_context(&user_msg).await;

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
                                    crate::types::Role::Assistant, &text, None
                                );
                                self.push_message(response_msg.clone());
                                
                                if self.channel.is_cli_source() {
                                    // CLI는 이미 스트리밍으로 출력됨 → 프롬프트만 복원
                                    let _ = tokio::task::spawn_blocking(|| {
                                        use std::io::Write;
                                        println!();
                                        print!("> ");
                                        std::io::stdout().flush().ok();
                                    }).await;
                                } else {
                                    // 텔레그램 등은 send()로 메시지 전송
                                    self.channel.send(response_msg).await?;
                                }
                                
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
                                    if let Content::Text { text, .. } = &final_msg.content {
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
                            let _ = self.channel.send(crate::types::Message::text(crate::types::Role::Assistant, err_text, None)).await;
                            None
                        }
                    };

                    #[cfg(feature = "memory")]
                    {
                        self.save_turn_memory_entries(&user_msg, final_assistant_text.as_deref()).await;
                        self.clear_turn_memory_context();
                        self.check_and_flush_context().await?;
                    }
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
        let request_messages = self.request_messages();
        let mut stream = match self.provider.stream(&request_messages, tools).await {
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
}
