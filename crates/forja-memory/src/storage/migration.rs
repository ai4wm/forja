use super::{legacy_archive_path, should_skip_entry, storage_error, Storage};
use chrono::{Datelike, Local, NaiveDate, TimeZone};
use forja_core::error::{ForjaError, Result};
use forja_core::types::MemoryEntry;
use serde::Deserialize;
use std::path::{Path, PathBuf};
use tokio::fs;

#[derive(Debug, Deserialize)]
struct LegacyFrontmatter {
    id: String,
    timestamp: u64,
    tags: Vec<String>,
}

impl Storage {
    pub(super) async fn migrate_legacy_sessions(&self) -> Result<()> {
        if !self.sessions_dir.exists() {
            return Ok(());
        }

        let entries = read_legacy_session_entries(&self.sessions_dir).await?;
        if entries.is_empty() {
            return Ok(());
        }

        for entry in entries {
            if !should_skip_entry(&entry) {
                self.write_daily_entry(&entry).await?;
                self.update_topics_for_entry(&entry).await?;
            }
        }

        let backup_dir = next_sessions_backup_dir(&self.sessions_backup_dir);
        fs::rename(&self.sessions_dir, &backup_dir)
            .await
            .map_err(|error| storage_error(format!("Failed to rename sessions dir: {error}")))?;

        Ok(())
    }

    pub(super) async fn migrate_legacy_memory_file(&self) -> Result<()> {
        if !self.legacy_memory_file.exists() {
            return Ok(());
        }

        let contents = fs::read_to_string(&self.legacy_memory_file)
            .await
            .map_err(|error| storage_error(format!("Failed to read legacy memory.md: {error}")))?;
        if contents.trim().is_empty() {
            return Ok(());
        }

        let entries = parse_legacy_memory_contents(&contents)?;
        for entry in entries {
            if !should_skip_entry(&entry) {
                self.write_daily_entry(&entry).await?;
                self.update_topics_for_entry(&entry).await?;
            }
        }

        let archive_path = legacy_archive_path(&self.archive_dir, "legacy-memory", "md").await?;
        fs::rename(&self.legacy_memory_file, archive_path)
            .await
            .map_err(|error| storage_error(format!("Failed to archive legacy memory.md: {error}")))?;

        Ok(())
    }
}

async fn read_legacy_session_entries(sessions_dir: &Path) -> Result<Vec<MemoryEntry>> {
    let mut entries = Vec::new();
    let mut read_dir = fs::read_dir(sessions_dir)
        .await
        .map_err(|error| storage_error(format!("Failed to read sessions dir: {error}")))?;

    while let Some(entry) = read_dir
        .next_entry()
        .await
        .map_err(|error| storage_error(error.to_string()))?
    {
        let path = entry.path();
        if path.extension().and_then(|extension| extension.to_str()) == Some("md")
            && let Ok(memory_entry) = parse_legacy_session_file(&path).await
        {
            entries.push(memory_entry);
        }
    }

    entries.sort_by_key(|entry| entry.timestamp);
    Ok(entries)
}

async fn parse_legacy_session_file(path: &Path) -> Result<MemoryEntry> {
    let content = fs::read_to_string(path)
        .await
        .map_err(|error| storage_error(format!("Failed to read file {:?}: {error}", path)))?;

    if !content.starts_with("---\n") && !content.starts_with("---\r\n") {
        return Err(ForjaError::Storage(format!(
            "Invalid frontmatter format in {:?}",
            path
        )));
    }

    let parts: Vec<&str> = content.splitn(3, "---").collect();
    if parts.len() < 3 {
        return Err(ForjaError::Storage(format!(
            "Cannot parse Markdown YAML block in {:?}",
            path
        )));
    }

    let frontmatter: LegacyFrontmatter = serde_yaml::from_str(parts[1].trim())
        .map_err(|error| ForjaError::Deserialization(format!("YAML deserialize error: {error}")))?;

    Ok(MemoryEntry {
        id: frontmatter.id,
        timestamp: frontmatter.timestamp,
        tags: frontmatter.tags,
        content: parts[2].trim().to_string(),
        score: 0.0,
        metadata: Default::default(),
    })
}

fn parse_legacy_memory_contents(contents: &str) -> Result<Vec<MemoryEntry>> {
    let mut entries = Vec::new();
    let mut current_date: Option<NaiveDate> = None;

    for line in contents.lines() {
        if let Some(date) = parse_date_header(line) {
            current_date = Some(date);
            continue;
        }

        let Some((time_text, role, body)) = parse_legacy_memory_line(line) else {
            continue;
        };

        let Some(date) = current_date else {
            continue;
        };

        let timestamp = parse_timestamp(date, time_text)?;
        entries.push(MemoryEntry {
            id: format!("{role}_{timestamp}"),
            timestamp,
            tags: vec![role.to_string()],
            content: body.to_string(),
            score: 0.0,
            metadata: Default::default(),
        });
    }

    Ok(entries)
}

fn parse_date_header(line: &str) -> Option<NaiveDate> {
    let date_text = line.strip_prefix("--- ")?.strip_suffix(" ---")?;
    NaiveDate::parse_from_str(date_text, "%Y-%m-%d").ok()
}

fn parse_legacy_memory_line(line: &str) -> Option<(&str, &str, &str)> {
    let mut parts = line.splitn(3, " | ");
    let time_text = parts.next()?.trim();
    let role = parts.next()?.trim();
    let body = parts.next()?.trim();

    if time_text.is_empty() || role.is_empty() || body.is_empty() {
        return None;
    }

    Some((time_text, role, body))
}

fn parse_timestamp(date: NaiveDate, time_text: &str) -> Result<u64> {
    let (hour, minute) = time_text
        .split_once(':')
        .ok_or_else(|| storage_error(format!("Invalid legacy time: {time_text}")))?;
    let hour = hour
        .parse::<u32>()
        .map_err(|error| storage_error(format!("Invalid legacy hour: {error}")))?;
    let minute = minute
        .parse::<u32>()
        .map_err(|error| storage_error(format!("Invalid legacy minute: {error}")))?;

    let local_time = Local
        .with_ymd_and_hms(date.year(), date.month(), date.day(), hour, minute, 0)
        .single()
        .ok_or_else(|| storage_error("Invalid local time while migrating memory.md".to_string()))?;

    Ok(local_time.timestamp() as u64)
}

fn next_sessions_backup_dir(base: &Path) -> PathBuf {
    if !base.exists() {
        return base.to_path_buf();
    }

    let base_name = base
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("sessions.bak");

    for index in 1.. {
        let candidate = base.with_file_name(format!("{base_name}.{index}"));
        if !candidate.exists() {
            return candidate;
        }
    }

    unreachable!("unbounded iterator should always find a backup dir")
}
