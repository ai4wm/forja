use forja_core::types::Message;

#[derive(Debug, Clone, Default)]
pub struct SessionBuffer {
    messages: Vec<Message>,
}

impl SessionBuffer {
    pub fn new() -> Self {
        Self {
            messages: Vec::new(),
        }
    }

    pub fn add(&mut self, message: Message) {
        self.messages.push(message);
    }

    pub fn get_recent(&self, count: usize) -> Vec<Message> {
        self.messages
            .iter()
            .rev()
            .take(count)
            .cloned()
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect()
    }

    pub fn get_all(&self) -> Vec<Message> {
        self.messages.clone()
    }

    pub fn clear(&mut self) {
        self.messages.clear();
    }

    pub fn len(&self) -> usize {
        self.messages.len()
    }

    pub fn is_empty(&self) -> bool {
        self.messages.is_empty()
    }

    pub fn token_count(&self) -> usize {
        self.messages
            .iter()
            .map(|message| message.content_text_len() / 4)
            .sum()
    }

    pub(crate) fn drain_oldest(&mut self, count: usize) -> Vec<Message> {
        let drain_count = count.min(self.messages.len());
        self.messages.drain(0..drain_count).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::SessionBuffer;
    use forja_core::types::{Message, Role};

    #[test]
    fn session_buffer_adds_and_returns_recent_messages() {
        let mut buffer = SessionBuffer::new();
        buffer.add(Message::text(Role::User, "first", None));
        buffer.add(Message::text(Role::Assistant, "second", None));
        buffer.add(Message::text(Role::User, "third", None));

        let recent = buffer.get_recent(2);

        assert_eq!(buffer.len(), 3);
        assert_eq!(recent.len(), 2);
        assert_eq!(recent[0].content_text_len(), "second".len());
        assert_eq!(recent[1].content_text_len(), "third".len());
    }

    #[test]
    fn session_buffer_token_count_increases_with_messages() {
        let mut buffer = SessionBuffer::new();
        buffer.add(Message::text(
            Role::User,
            "This is a moderately sized message for token counting.",
            None,
        ));
        buffer.add(Message::text(
            Role::Assistant,
            "Another message with enough text to count.",
            None,
        ));

        assert!(buffer.token_count() > 0);
    }
}
