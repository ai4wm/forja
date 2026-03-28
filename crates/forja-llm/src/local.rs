use async_trait::async_trait;
use forja_core::error::{ForjaError, Result};
use forja_core::traits::LlmProvider;
use forja_core::types::{Message, ToolDefinition};
use std::path::{Path, PathBuf};
use std::pin::Pin;
use tokio_stream::Stream;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalModelInfo {
    pub file_name: String,
    pub path: PathBuf,
}

pub fn ensure_models_dir(base_dir: &Path) -> std::io::Result<PathBuf> {
    let models_dir = base_dir.join(".forja").join("models");
    std::fs::create_dir_all(&models_dir)?;
    Ok(models_dir)
}

pub fn detect_local_models(base_dir: &Path) -> std::io::Result<Vec<LocalModelInfo>> {
    let models_dir = ensure_models_dir(base_dir)?;
    let mut models = std::fs::read_dir(&models_dir)?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .and_then(|extension| extension.to_str())
                .map(|extension| extension.eq_ignore_ascii_case("gguf"))
                .unwrap_or(false)
        })
        .filter_map(|path| {
            let file_name = path.file_name()?.to_str()?.to_string();
            Some(LocalModelInfo { file_name, path })
        })
        .collect::<Vec<_>>();

    models.sort_by(|left, right| left.file_name.cmp(&right.file_name));
    Ok(models)
}

#[derive(Debug, Clone)]
pub struct LocalModelProvider {
    model: LocalModelInfo,
}

impl LocalModelProvider {
    pub fn new(model: LocalModelInfo) -> Self {
        Self { model }
    }

    pub fn model(&self) -> &LocalModelInfo {
        &self.model
    }
}

#[async_trait]
impl LlmProvider for LocalModelProvider {
    async fn chat(
        &self,
        _messages: &[Message],
        _tools: Option<&[ToolDefinition]>,
    ) -> Result<Message> {
        Err(ForjaError::LlmError(format!(
            "Local GGUF inference is not implemented yet for {}",
            self.model.file_name
        )))
    }

    async fn stream(
        &self,
        _messages: &[Message],
        _tools: Option<&[ToolDefinition]>,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<String>> + Send>>> {
        Err(ForjaError::LlmError(format!(
            "Local GGUF inference is not implemented yet for {}",
            self.model.file_name
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_temp_dir(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        std::env::temp_dir().join(format!("forja_llm_{name}_{nanos}"))
    }

    #[test]
    fn ensure_models_dir_creates_models_directory() {
        let home_dir = unique_temp_dir("models_dir");

        let models_dir = ensure_models_dir(&home_dir).unwrap();

        assert!(models_dir.exists());

        let _ = std::fs::remove_dir_all(home_dir);
    }

    #[test]
    fn detect_local_models_finds_gguf_files() {
        let home_dir = unique_temp_dir("detect_models");
        let models_dir = ensure_models_dir(&home_dir).unwrap();
        std::fs::write(models_dir.join("phi-4-mini.gguf"), "stub").unwrap();
        std::fs::write(models_dir.join("notes.txt"), "ignore").unwrap();

        let models = detect_local_models(&home_dir).unwrap();

        assert_eq!(models.len(), 1);
        assert_eq!(models[0].file_name, "phi-4-mini.gguf");

        let _ = std::fs::remove_dir_all(home_dir);
    }
}
