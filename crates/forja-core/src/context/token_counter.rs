use crate::types::{Content, Message, Role};
use tiktoken_rs::{cl100k_base, get_bpe_from_model};

pub fn count_tokens(text: &str, model: &str) -> usize {
    let bpe = if model.is_empty() || model == "cl100k_base" {
        cl100k_base().expect("cl100k_base encoding should be available")
    } else {
        get_bpe_from_model(model)
            .unwrap_or_else(|_| cl100k_base().expect("cl100k_base encoding should be available"))
    };

    bpe.encode_with_special_tokens(text).len()
}

pub fn count_messages_tokens(messages: &[Message], model: &str) -> usize {
    messages
        .iter()
        .map(|message| count_message_tokens(message, model))
        .sum()
}

pub fn count_message_tokens(message: &Message, model: &str) -> usize {
    if let Some(tokens) = message.metadata.get("tokens").and_then(serde_json::Value::as_u64) {
        return tokens as usize;
    }

    count_tokens(&message_token_text(message), model)
}

fn message_token_text(message: &Message) -> String {
    match &message.content {
        Content::Text { text, .. } => {
            format!("role:{}\ntext:{text}", role_name(&message.role))
        }
        Content::ToolCall {
            call_id,
            tool_name,
            arguments,
            reasoning_content,
            thought_signature,
        } => {
            let reasoning = reasoning_content.as_deref().unwrap_or_default();
            let signature = thought_signature.as_deref().unwrap_or_default();
            format!(
                "role:{}\ncall_id:{call_id}\ntool_name:{tool_name}\narguments:{arguments}\nreasoning:{reasoning}\nsignature:{signature}",
                role_name(&message.role)
            )
        }
        Content::ToolResult { call_id, result } => {
            format!(
                "role:{}\ncall_id:{call_id}\nresult:{result}",
                role_name(&message.role)
            )
        }
    }
}

fn role_name(role: &Role) -> &'static str {
    match role {
        Role::User => "user",
        Role::Assistant => "assistant",
        Role::System => "system",
        Role::Tool => "tool",
    }
}
