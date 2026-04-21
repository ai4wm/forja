use super::{Engine, ANSI_CYAN, ANSI_MAGENTA, ANSI_YELLOW};
use crate::error::Result;
use crate::types::{Content, Message};

impl Engine {
    #[cfg(feature = "runtime")]
    pub(crate) async fn process_non_streaming_turn(&mut self, user_msg: Message) -> Result<()> {
        self.push_message(user_msg.clone());
        self.prepare_user_turn(&user_msg).await;

        let response = self.handle_step(0).await?;
        let response = self.maybe_append_serendipity_to_message(response).await;
        self.channel.send(response.clone()).await?;

        let assistant_text = match &response.content {
            Content::Text { text, .. } => Some(text.as_str()),
            _ => None,
        };
        self.finish_user_turn(&user_msg, assistant_text).await?;
        Ok(())
    }

    #[cfg(feature = "runtime")]
    pub(crate) async fn prepare_user_turn(&mut self, user_msg: &Message) {
        self.begin_user_turn();
        self.refresh_turn_role(user_msg);
        self.log_cli_stage(ANSI_CYAN, "Loading emotion context...").await;
        self.refresh_turn_emotion_context().await;
        self.log_cli_stage(ANSI_YELLOW, "Loading knowledge...").await;
        self.refresh_turn_knowledge_context(user_msg).await;

        #[cfg(feature = "memory")]
        self.log_cli_stage(ANSI_MAGENTA, "Loading memory...").await;
        #[cfg(feature = "memory")]
        self.refresh_turn_memory_context(user_msg).await;
    }

    #[cfg(feature = "runtime")]
    pub(crate) async fn finish_user_turn(
        &mut self,
        user_msg: &Message,
        assistant_text: Option<&str>,
    ) -> Result<()> {
        #[cfg(feature = "memory")]
        {
            self.save_turn_memory_entries(user_msg, assistant_text).await;
            self.clear_turn_memory_context();
            self.check_and_flush_context().await?;
        }

        #[cfg(not(feature = "memory"))]
        let _ = assistant_text;

        self.clear_turn_knowledge_context();
        self.clear_turn_emotion_context();
        Ok(())
    }
}
