use super::storage_error;
use chrono::Utc;
use forja_core::error::Result;
use forja_core::traits::DreamTrigger;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tokio::fs;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct JournalFileStamp {
    pub path: String,
    pub mtime_secs: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct DreamJournal {
    pub trigger: DreamTrigger,
    pub created_at: String,
    pub files: Vec<JournalFileStamp>,
}

impl DreamJournal {
    pub(super) fn new(trigger: DreamTrigger, files: Vec<JournalFileStamp>) -> Self {
        Self {
            trigger,
            created_at: Utc::now().to_rfc3339(),
            files,
        }
    }
}

pub(super) async fn load_pending_journal(path: &Path) -> Result<Option<DreamJournal>> {
    if !path.exists() {
        return Ok(None);
    }

    let contents = fs::read_to_string(path)
        .await
        .map_err(|error| storage_error(format!("Failed to read {}: {error}", path.display())))?;
    let journal = serde_yaml::from_str::<DreamJournal>(&contents)
        .map_err(|error| storage_error(format!("Failed to parse {}: {error}", path.display())))?;
    Ok(Some(journal))
}

pub(super) async fn write_pending_journal(path: &Path, journal: &DreamJournal) -> Result<()> {
    let serialized = serde_yaml::to_string(journal)
        .map_err(|error| storage_error(format!("Failed to serialize dream journal: {error}")))?;
    fs::write(path, serialized)
        .await
        .map_err(|error| storage_error(format!("Failed to write {}: {error}", path.display())))
}

pub(super) async fn clear_pending_journal(path: &Path) -> Result<()> {
    if path.exists() {
        fs::remove_file(path).await.map_err(|error| {
            storage_error(format!("Failed to remove {}: {error}", path.display()))
        })?;
    }
    Ok(())
}

pub(super) fn relative_path(base_dir: &Path, path: &Path) -> PathBuf {
    path.strip_prefix(base_dir)
        .map(PathBuf::from)
        .unwrap_or_else(|_| path.to_path_buf())
}
