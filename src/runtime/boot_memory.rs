use crate::runtime::prompt::{auto_summarize_enabled, summarize_memory_block};
use forja_core::KnowledgeManager;
use forja_core::emotion::{
    EmotionEngine, MoodState, generate_startup_greeting, generate_startup_greeting_with_context,
};
use forja_core::error::Result;
use forja_core::traits::{LlmProvider, MemoryStore};
use forja_memory::MarkdownMemoryStore;
use std::sync::Arc;

pub(crate) struct MemoryBundle {
    pub(crate) memory_store: Arc<MarkdownMemoryStore>,
    pub(crate) restored_mood: MoodState,
    pub(crate) displayed_greeting: Option<String>,
}

pub(crate) async fn build_memory_bundle(
    provider: Arc<dyn LlmProvider>,
    knowledge_manager: Arc<KnowledgeManager>,
    assistant_name: &str,
    user_title: &str,
    bootstrap_greeting: Option<String>,
    serendipity_enabled: bool,
) -> Result<MemoryBundle> {
    let bootstrap_greeting_available = bootstrap_greeting.is_some();
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
    let restored_mood =
        EmotionEngine::restore_from_memory(&memory_contents).unwrap_or_else(MoodState::neutral);
    let displayed_greeting = if serendipity_enabled {
        bootstrap_greeting.or(generate_startup_greeting_with_context(
            provider.as_ref(),
            assistant_name,
            user_title,
            &memory_contents,
            &knowledge_contents,
            bootstrap_greeting_available,
        )
        .await
        .unwrap_or(None))
    } else {
        bootstrap_greeting.or(generate_startup_greeting(
            provider.as_ref(),
            assistant_name,
            user_title,
            &memory_contents,
            bootstrap_greeting_available,
        )
        .await
        .unwrap_or(None))
    };

    Ok(MemoryBundle {
        memory_store,
        restored_mood,
        displayed_greeting,
    })
}
