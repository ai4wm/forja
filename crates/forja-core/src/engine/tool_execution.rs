use super::{Engine, MAX_TOOL_DEPTH, ANSI_GREEN};
use crate::context::token_counter::{count_message_tokens, count_messages_tokens};
use crate::error::{ForjaError, Result};
use crate::ralf::executor::ralf_execute;
use crate::ralf::RalfState;
use crate::types::{Content, Message, ToolDefinition};

impl Engine {
    #[async_recursion::async_recursion]
    pub async fn handle_step(&mut self, depth: usize) -> Result<Message> {
        if depth >= MAX_TOOL_DEPTH {
            return Err(ForjaError::MaxDepthExceeded(MAX_TOOL_DEPTH));
        }

        let tool_defs: Vec<ToolDefinition> = self.tools.values().map(|tool| tool.definition()).collect();

        self.compress_context().await?;
        self.check_current_agent_budget()?;
        let request_messages = self.request_messages();
        let request_token_count = count_messages_tokens(&request_messages, &self.context_model);
        self.log_llm_call("chat", request_token_count);
        self.log_cli_stage(ANSI_GREEN, "Calling LLM...").await;

        let provider = self.provider.clone();
        let tool_defs_for_retry = tool_defs.clone();
        let mut ralf_state = RalfState::default();
        let response_msg = ralf_execute(
            "llm_call",
            &self.ralf_config,
            &mut ralf_state,
            self.audit_logger.as_deref(),
            move || {
                let provider = provider.clone();
                let request_messages = request_messages.clone();
                let tool_defs = tool_defs_for_retry.clone();
                async move {
                    let tools = if tool_defs.is_empty() {
                        None
                    } else {
                        Some(tool_defs.as_slice())
                    };
                    provider.chat(&request_messages, tools).await
                }
            },
        )
        .await?;
        self.record_current_agent_usage(count_message_tokens(&response_msg, &self.context_model))?;

        match response_msg.content.clone() {
            Content::ToolCall {
                call_id,
                tool_name,
                arguments,
                reasoning_content: _,
                thought_signature: _,
            } => self.handle_tool_call_response(
                depth,
                response_msg,
                call_id,
                tool_name,
                arguments,
            )
            .await,
            _ => {
                self.push_message(response_msg.clone());
                Ok(response_msg)
            }
        }
    }

    async fn handle_tool_call_response(
        &mut self,
        depth: usize,
        response_msg: Message,
        call_id: String,
        tool_name: String,
        arguments: serde_json::Value,
    ) -> Result<Message> {
        self.push_message(response_msg);

        let result = if let Some(tool) = self.tools.get(&tool_name).cloned() {
            self.log_tool_call(&tool_name, &arguments);
            let arguments = arguments.clone();
            let mut ralf_state = RalfState::default();
            let result = ralf_execute(
                "tool_call",
                &self.ralf_config,
                &mut ralf_state,
                self.audit_logger.as_deref(),
                move || {
                    let tool = tool.clone();
                    let arguments = arguments.clone();
                    async move { tool.execute(arguments).await }
                },
            )
            .await?;
            self.log_tool_result(&call_id, &result);
            result
        } else {
            let result = serde_json::json!({
                "error": format!("Unknown tool requested: {}", tool_name)
            });
            self.log_tool_call(&tool_name, &arguments);
            self.log_tool_result(&call_id, &result);
            result
        };

        let result_msg = Message::tool_result(&call_id, result);
        self.push_message(result_msg);
        self.handle_step(depth + 1).await
    }
}
