use super::{Engine, ANSI_BLUE, ANSI_GREEN};
use crate::context::token_counter::count_messages_tokens;
use crate::error::Result;
use crate::ralf::executor::ralf_execute;
use crate::ralf::RalfState;
use crate::traits::{NotificationLevel, NotificationTopic};
use crate::types::Content;

impl Engine {
    #[cfg(feature = "runtime")]
    pub(crate) async fn process_streaming_turn(&mut self, user_msg: crate::types::Message) -> Result<()> {
        if self.dispatch_slash_command(&user_msg).await? {
            return Ok(());
        }

        self.push_message(user_msg.clone());
        self.prepare_user_turn(&user_msg).await;

        let mut response_result = self.execute_streaming_turn_once().await;
        if should_retry_with_emergency(response_result.as_ref()) {
            self.log_cli_stage(ANSI_BLUE, "Compressing context...").await;
            response_result = if let Err(error) = self.emergency_compress_context().await {
                Err(error)
            } else {
                self.execute_streaming_turn_once().await
            };
        }

        let final_assistant_text = match response_result {
            Ok(text_opt) => text_opt,
            Err(error) => {
                let err_text = format!("⚠️ Error: {}", error);
                eprintln!("[Engine Error] {}", err_text);
                self.log_engine_error("run_streaming", &error.to_string());
                let _ = self
                    .channel
                    .send_notification_with_level(
                        &format!("Forja runtime error: {}", error),
                        NotificationTopic::Error,
                        NotificationLevel::Critical,
                    )
                    .await;

                let _ = self.channel.send(self.build_text_response(&err_text)).await;
                None
            }
        };

        self.finish_user_turn(&user_msg, final_assistant_text.as_deref())
            .await?;
        Ok(())
    }

    #[cfg(feature = "runtime")]
    pub(crate) async fn execute_streaming_turn_once(&mut self) -> Result<Option<String>> {
        self.compress_context().await?;
        self.check_current_agent_budget()?;
        let request_token_count =
            count_messages_tokens(&self.request_messages(), &self.context_model);
        self.log_llm_call("stream", request_token_count);

        let mut ralf_state = RalfState::default();
        let streaming_result = ralf_execute(
            "llm_stream",
            &self.ralf_config,
            &mut ralf_state,
            self.audit_logger.as_deref(),
            || self.stream_step_with_tools(),
        )
        .await
        .unwrap_or(None);

        match streaming_result {
            Some(text) => {
                let streamed_text = text;
                let text = self
                    .maybe_append_serendipity_to_text(streamed_text.clone())
                    .await;
                let response_msg = self.build_text_response(&text);
                self.track_response_usage(&response_msg)?;
                self.push_message(response_msg.clone());

                if self.channel.is_cli_source() {
                    if let Some(suffix) = text.strip_prefix(&streamed_text)
                        && !suffix.is_empty()
                    {
                        print!("{suffix}");
                        std::io::Write::flush(&mut std::io::stdout()).ok();
                    }
                    let _ = tokio::task::spawn_blocking(|| {
                        use std::io::Write;
                        println!();
                        print!("> ");
                        std::io::stdout().flush().ok();
                    })
                    .await;
                } else {
                    self.channel.send(response_msg).await?;
                }

                Ok(Some(text))
            }
            None => {
                let final_msg = self.handle_step(0).await?;
                let final_msg = self.maybe_append_serendipity_to_message(final_msg).await;
                self.channel.send(final_msg.clone()).await?;

                Ok(if let Content::Text { text, .. } = &final_msg.content {
                    Some(text.clone())
                } else {
                    None
                })
            }
        }
    }

    #[cfg(feature = "runtime")]
    pub(crate) async fn stream_step_with_tools(&self) -> Result<Option<String>> {
        use tokio_stream::StreamExt;

        let tool_defs: Vec<crate::types::ToolDefinition> =
            self.tools.values().map(|tool| tool.definition()).collect();
        let tools = if tool_defs.is_empty() {
            None
        } else {
            Some(tool_defs.as_slice())
        };

        let request_messages = self.request_messages();
        self.log_cli_stage(ANSI_GREEN, "Calling LLM...").await;
        let mut stream = match self.provider.stream(&request_messages, tools).await {
            Ok(stream) => stream,
            Err(_) => return Ok(None),
        };

        let mut full_text = String::new();
        let mut first_token = true;

        while let Some(chunk) = stream.next().await {
            match chunk {
                Ok(token) => {
                    if token.is_empty() {
                        continue;
                    }

                    if first_token
                        && (token.trim_start().starts_with("{\"") || token.contains("tool_call"))
                    {
                        return Ok(None);
                    }

                    if first_token {
                        self.channel.cancel_typing().await;
                        first_token = false;
                    }

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
            Ok(None)
        } else {
            if self.channel.is_cli_source() {
                println!();
            }
            Ok(Some(full_text))
        }
    }
}

fn should_retry_with_emergency(
    response_result: std::result::Result<&Option<String>, &crate::error::ForjaError>,
) -> bool {
    response_result
        .err()
        .map(|error| {
            let err_str = error.to_string().to_lowercase();
            err_str.contains("token")
                || err_str.contains("limit")
                || err_str.contains("exceeded")
                || err_str.contains("context")
        })
        .unwrap_or(false)
}
