use crate::error::Result;
use crate::traits::LlmProvider;
use crate::types::{Content, Message, Role};
use chrono::{DateTime, Duration, Local};

const DEFAULT_SERENDIPITY_INTERVAL: u32 = 5;

#[derive(Debug, Clone)]
pub struct SerendipityEngine {
    interval: u32,
}

impl Default for SerendipityEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl SerendipityEngine {
    pub fn new() -> Self {
        let interval = std::env::var("FORJA_SERENDIPITY_INTERVAL")
            .ok()
            .and_then(|value| value.parse::<u32>().ok())
            .filter(|value| *value > 0)
            .unwrap_or(DEFAULT_SERENDIPITY_INTERVAL);

        Self { interval }
    }

    pub fn with_interval(interval: u32) -> Self {
        Self {
            interval: interval.max(1),
        }
    }

    pub async fn generate_insight(
        &self,
        memory: &str,
        knowledge: &str,
        provider: &dyn LlmProvider,
    ) -> Result<Option<String>> {
        if memory.trim().is_empty() && knowledge.trim().is_empty() {
            return Ok(None);
        }

        let response = match provider
            .chat(
                &[
                    Message::text(
                        Role::System,
                        "You generate one proactive, concise suggestion for the user or NONE.",
                        None,
                    ),
                    Message::text(
                        Role::User,
                        format!(
                            "Below is the user's recent memory and knowledge base.\n\
If you find something proactively useful to mention\n\
(unfinished tasks, related connections, daily summary,\n\
upcoming patterns), respond with a single concise suggestion.\n\
If nothing is worth mentioning, respond with NONE only.\n\
\n\
Memory:\n\
{memory}\n\
\n\
Knowledge:\n\
{knowledge}"
                        ),
                        None,
                    ),
                ],
                None,
            )
            .await
        {
            Ok(response) => response,
            Err(_) => return Ok(None),
        };

        let Content::Text { text, .. } = response.content else {
            return Ok(None);
        };
        let insight = text
            .replace(['\r', '\n'], " ")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");

        if insight.eq_ignore_ascii_case("NONE") || insight.is_empty() {
            return Ok(None);
        }

        Ok(Some(insight))
    }

    pub fn should_trigger(
        &self,
        turn_count: u32,
        last_triggered: Option<DateTime<Local>>,
    ) -> bool {
        if let Some(last_triggered) = last_triggered
            && Local::now().signed_duration_since(last_triggered) >= Duration::minutes(10)
        {
            return true;
        }

        turn_count > 0 && turn_count.is_multiple_of(self.interval)
    }
}
