use crate::config::{self, ForjaConfig};
use crate::provider_registry::ProviderRegistry;
use crate::runtime::mock::MockLlmProvider;
use forja_core::error::{ForjaError, Result};
use forja_core::traits::LlmProvider;
use forja_llm::{LlmClient, LlmConfig};
use std::sync::Arc;

pub(crate) struct ProviderBundle {
    pub(crate) registry: ProviderRegistry,
    pub(crate) use_mock: bool,
    pub(crate) llm_config: Option<LlmConfig>,
    pub(crate) provider: Arc<dyn LlmProvider>,
}

pub(crate) fn build_provider_bundle(forja_cfg: &ForjaConfig) -> Result<ProviderBundle> {
    let registry = ProviderRegistry::from_config(forja_cfg);
    let use_mock = std::env::var("FORJA_USE_MOCK").is_ok();
    let llm_config = if use_mock {
        None
    } else {
        Some(config::llm_config_from(forja_cfg).map_err(ForjaError::LlmError)?)
    };
    let provider: Arc<dyn LlmProvider> = if use_mock {
        println!("MockLlmProvider mode (no live LLM calls)");
        Arc::new(MockLlmProvider)
    } else {
        Arc::new(LlmClient::new(
            llm_config
                .clone()
                .expect("llm_config must exist when not in mock mode"),
        )?)
    };

    Ok(ProviderBundle {
        registry,
        use_mock,
        llm_config,
        provider,
    })
}
