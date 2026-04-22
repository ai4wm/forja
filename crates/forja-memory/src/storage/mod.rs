mod classifier;
mod dream;
mod journal;
mod migration;

use self::classifier::{
    classify_topic_slug, parse_topic_file_name, query_score, summary_text, topic_file_name,
};
use crate::sqlite::{SqliteEntryRow, SqliteSummaryRow};
use chrono::{Datelike, Local, TimeZone};
use forja_core::error::{ForjaError as Error, Result};
use forja_core::traits::{DreamRunOutcome, DreamTrigger};
use forja_core::types::MemoryEntry;
use std::fmt::Display;
use std::path::{Path, PathBuf};
use tokio::fs;
use tokio::io::AsyncWriteExt;

const INDEX_CHAR_BUDGET: usize = 16_000;
const TOPIC_CHAR_BUDGET: usize = 8_000;
const INDEX_SUMMARY_CHAR_LIMIT: usize = 150;
const STARTUP_DAILY_CHAR_BUDGET: usize = 6_000;
const MAX_RECENT_DAILY_FILES: usize = 3;
const MAX_SHARDS_PER_QUERY: usize = 2;
#[derive(Debug, Clone)]
pub struct Storage {
    base_dir: PathBuf,
    legacy_memory_file: PathBuf,
    index_file: PathBuf,
    topics_dir: PathBuf,
    daily_dir: PathBuf,
    archive_dir: PathBuf,
    dreams_dir: PathBuf,
    dream_state_file: PathBuf,
    sessions_dir: PathBuf,
    sessions_backup_dir: PathBuf,
}

#[derive(Debug, Clone)]
struct TopicIndexEntry {
    slug: String,
    shards: usize,
    summary: String,
}
impl Storage {
    pub async fn init(memory_file: impl AsRef<Path>) -> Result<Self> {
        let legacy_memory_file = memory_file.as_ref().to_path_buf();
        let base_dir = legacy_memory_file
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."));
        let storage = Self {
            index_file: base_dir.join("index.md"),
            topics_dir: base_dir.join("topics"),
            daily_dir: base_dir.join("daily"),
            archive_dir: base_dir.join("archive"),
            dreams_dir: base_dir.join("dreams"),
            dream_state_file: base_dir.join("dreams").join("pending.yaml"),
            sessions_dir: base_dir.join("sessions"),
            sessions_backup_dir: base_dir.join("sessions.bak"),
            base_dir,
            legacy_memory_file,
        };

        storage.ensure_layout().await?;
        storage.migrate_legacy_sessions().await?;
        storage.migrate_legacy_memory_file().await?;
        storage.rebuild_index().await?;
        Ok(storage)
    }

    pub async fn append_entry(&self, entry: &MemoryEntry) -> Result<()> {
        if should_skip_entry(entry) {
            return Ok(());
        }

        self.write_daily_entry(entry).await?;
        self.update_topics_for_entry(entry).await?;
        self.rebuild_index().await
    }

    pub async fn flush_and_summarize<F, E>(&self, summarizer: F) -> Result<()>
    where
        F: Fn(String) -> std::result::Result<String, E>,
        E: Display,
    {
        let today = Local::now().format("%Y-%m-%d").to_string();
        let mut daily_paths = list_markdown_files(&self.daily_dir).await?;
        daily_paths.sort();

        for path in daily_paths {
            let Some(stem) = path.file_stem().and_then(|stem| stem.to_str()) else {
                continue;
            };
            if stem == today {
                continue;
            }

            let contents = fs::read_to_string(&path)
                .await
                .map_err(|error| storage_error(format!("Failed to read daily file: {error}")))?;
            if contents.trim().is_empty() {
                continue;
            }

            let archive_path = self.archive_dir.join(format!("{stem}.md"));
            if archive_path.exists() {
                continue;
            }

            let Ok(summary) = summarizer(contents.clone()) else {
                continue;
            };
            let Some(summary_lines) = normalize_summary_lines(&summary) else {
                continue;
            };

            fs::write(&archive_path, with_trailing_newline(&contents))
                .await
                .map_err(|error| storage_error(format!("Failed to write archive file: {error}")))?;
            fs::write(&path, with_trailing_newline(&summary_lines.join("\n")))
                .await
                .map_err(|error| storage_error(format!("Failed to rewrite daily file: {error}")))?;
        }

        self.rebuild_index().await
    }

    pub async fn read_all(&self) -> Result<String> {
        let startup = self.read_startup_context().await?;
        let topics = self.read_all_topics_context().await?;
        let mut sections = Vec::new();
        if !startup.trim().is_empty() {
            sections.push(startup);
        }
        if !topics.trim().is_empty() {
            sections.push(topics);
        }
        Ok(sections.join("\n\n"))
    }

    pub async fn read_startup_context(&self) -> Result<String> {
        let index = read_trimmed(&self.index_file).await?;
        let daily = self.read_recent_daily_context().await?;
        Ok(render_memory_context(index, daily, String::new()))
    }

    pub async fn read_relevant(&self, query: &str) -> Result<String> {
        let entries = self.read_index_entries().await?;
        let mut matches = entries
            .into_iter()
            .filter_map(|entry| {
                let score = query_score(query, &entry.slug, &entry.summary);
                if score == 0 {
                    return None;
                }
                Some((score, entry))
            })
            .collect::<Vec<_>>();
        matches.sort_by(|left, right| right.0.cmp(&left.0).then(left.1.slug.cmp(&right.1.slug)));

        let mut sections = Vec::new();
        for (_, entry) in matches.into_iter().take(3) {
            let shards = self.topic_shards(&entry.slug).await?;
            for path in shards.into_iter().rev().take(MAX_SHARDS_PER_QUERY).rev() {
                let contents = read_trimmed(&path).await?;
                if contents.is_empty() {
                    continue;
                }
                if let Some(name) = path.file_name().and_then(|name| name.to_str()) {
                    sections.push(format!("## {name}\n{contents}"));
                }
            }
        }

        if sections.is_empty() {
            return Ok(String::new());
        }

        Ok(format!(
            "[memory topics - Relevant Topic Memory]\n\n{}",
            sections.join("\n\n")
        ))
    }

    pub async fn reconcile(&self) -> Result<()> {
        self.rebuild_index().await
    }

    pub async fn run_dream(&self, trigger: DreamTrigger) -> Result<DreamRunOutcome> {
        Self::execute_dream(self, trigger).await
    }

    pub async fn latest_dream_timestamp(&self) -> Result<Option<u64>> {
        Self::read_latest_dream_timestamp(self).await
    }

    pub fn memory_db_path(&self) -> PathBuf {
        self.base_dir.join("memory.db")
    }

    pub async fn export_entry_rows(&self) -> Result<Vec<SqliteEntryRow>> {
        let mut rows = Vec::new();
        let mut paths = list_markdown_files(&self.daily_dir).await?;
        paths.sort();

        for path in paths {
            let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            let date = name.strip_suffix(".md").unwrap_or(name);
            let contents = read_trimmed(&path).await?;
            for (index, line) in contents.lines().enumerate() {
                let Some((time_text, role, body)) = parse_daily_line(line) else {
                    continue;
                };
                let timestamp = daily_timestamp(date, time_text)?;
                rows.push(SqliteEntryRow {
                    id: format!("daily-{date}-{index}"),
                    timestamp,
                    role: role.to_string(),
                    content: body.to_string(),
                    source: format!("daily/{name}"),
                });
            }
        }

        Ok(rows)
    }

    pub async fn export_summary_rows(&self) -> Result<Vec<SqliteSummaryRow>> {
        let mut rows = Vec::new();
        let mut paths = list_markdown_files(&self.archive_dir).await?;
        paths.sort();

        for path in paths {
            let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            let contents = read_trimmed(&path).await?;
            if contents.is_empty() {
                continue;
            }
            rows.push(SqliteSummaryRow {
                source: format!("archive/{name}"),
                summary: summary_text(&contents, 2_000),
                created_at: 0,
            });
        }

        Ok(rows)
    }

    async fn ensure_layout(&self) -> Result<()> {
        fs::create_dir_all(&self.base_dir)
            .await
            .map_err(|error| storage_error(format!("Failed to create memory dir: {error}")))?;
        for directory in [
            &self.topics_dir,
            &self.daily_dir,
            &self.archive_dir,
            &self.dreams_dir,
        ] {
            fs::create_dir_all(directory)
                .await
                .map_err(|error| storage_error(format!("Failed to create dir: {error}")))?;
        }
        if !self.index_file.exists() {
            fs::write(&self.index_file, "")
                .await
                .map_err(|error| storage_error(format!("Failed to create index.md: {error}")))?;
        }
        Ok(())
    }

    pub(super) async fn write_daily_entry(&self, entry: &MemoryEntry) -> Result<()> {
        let date = format_date(entry.timestamp);
        let path = self.daily_dir.join(format!("{date}.md"));
        append_text(&path, &format_daily_line(entry)).await
    }

    pub(super) async fn update_topics_for_entry(&self, entry: &MemoryEntry) -> Result<()> {
        let slug = classify_topic_slug(&entry.content);
        let shards = self.topic_shards(&slug).await?;
        let next_shard = shards
            .last()
            .and_then(|path| parse_topic_file_name(path).map(|(_, shard)| shard))
            .unwrap_or(1);
        let line = format_topic_line(entry);
        let target_path = if let Some(path) = shards.last() {
            let existing = read_trimmed(path).await?;
            if existing.len() + line.len() + 1 > TOPIC_CHAR_BUDGET {
                self.topics_dir.join(topic_file_name(&slug, next_shard + 1))
            } else {
                path.clone()
            }
        } else {
            self.topics_dir.join(topic_file_name(&slug, 1))
        };
        if !target_path.exists() {
            let header = format!("# Topic: {slug}\n");
            fs::write(&target_path, header).await.map_err(|error| {
                storage_error(format!(
                    "Failed to create topic file {}: {error}",
                    target_path.display()
                ))
            })?;
        }
        append_text(&target_path, &line).await
    }

    async fn rebuild_index(&self) -> Result<()> {
        let mut groups = std::collections::BTreeMap::<String, Vec<PathBuf>>::new();
        for path in list_markdown_files(&self.topics_dir).await? {
            if let Some((slug, _)) = parse_topic_file_name(&path) {
                groups.entry(slug).or_default().push(path);
            }
        }

        let mut entries = Vec::new();
        for (slug, mut paths) in groups {
            paths.sort_by_key(|path| {
                parse_topic_file_name(path)
                    .map(|(_, shard)| shard)
                    .unwrap_or(1)
            });
            let mut summary = String::new();
            if let Some(path) = paths.last() {
                summary = latest_topic_summary(path).await?;
            }
            entries.push(TopicIndexEntry {
                slug,
                shards: paths.len(),
                summary,
            });
        }

        write_file_atomically(
            &self.index_file,
            &with_trailing_newline(&render_index(&entries)),
        )
        .await
    }

    async fn read_index_entries(&self) -> Result<Vec<TopicIndexEntry>> {
        let contents = read_trimmed(&self.index_file).await?;
        Ok(contents
            .lines()
            .filter_map(parse_index_line)
            .collect::<Vec<_>>())
    }

    async fn read_recent_daily_context(&self) -> Result<String> {
        let mut daily_paths = list_markdown_files(&self.daily_dir).await?;
        daily_paths.sort_by(|left, right| right.cmp(left));
        let mut sections = Vec::new();
        let mut used_chars = 0;

        for path in daily_paths.into_iter().take(MAX_RECENT_DAILY_FILES) {
            let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            let contents = read_trimmed(&path).await?;
            if contents.is_empty() || used_chars >= STARTUP_DAILY_CHAR_BUDGET {
                continue;
            }
            let remaining = STARTUP_DAILY_CHAR_BUDGET.saturating_sub(used_chars);
            let truncated = summary_text(&contents, remaining);
            used_chars += truncated.len();
            sections.push(format!("## daily/{name}\n{truncated}"));
        }

        Ok(sections.join("\n\n"))
    }

    async fn read_all_topics_context(&self) -> Result<String> {
        let mut sections = Vec::new();
        let mut paths = list_markdown_files(&self.topics_dir).await?;
        paths.sort();

        for path in paths {
            let contents = read_trimmed(&path).await?;
            if contents.is_empty() {
                continue;
            }
            if let Some(name) = path.file_name().and_then(|name| name.to_str()) {
                sections.push(format!("## {name}\n{contents}"));
            }
        }

        if sections.is_empty() {
            return Ok(String::new());
        }

        Ok(format!(
            "[memory topics - All Topic Memory]\n\n{}",
            sections.join("\n\n")
        ))
    }

    async fn topic_shards(&self, slug: &str) -> Result<Vec<PathBuf>> {
        let mut shards = list_markdown_files(&self.topics_dir)
            .await?
            .into_iter()
            .filter(|path| {
                parse_topic_file_name(path)
                    .map(|(candidate, _)| candidate == slug)
                    .unwrap_or(false)
            })
            .collect::<Vec<_>>();
        shards.sort_by_key(|path| {
            parse_topic_file_name(path)
                .map(|(_, shard)| shard)
                .unwrap_or(1)
        });
        Ok(shards)
    }
}
pub(super) fn should_skip_entry(entry: &MemoryEntry) -> bool {
    entry.content.contains("MockStream")
}

pub(super) fn storage_error(message: impl Into<String>) -> Error {
    Error::Storage(message.into())
}

pub(super) async fn legacy_archive_path(
    archive_dir: &Path,
    stem: &str,
    extension: &str,
) -> Result<PathBuf> {
    let mut candidate = archive_dir.join(format!("{stem}.{extension}"));
    let mut index = 1;
    while candidate.exists() {
        candidate = archive_dir.join(format!("{stem}-{index}.{extension}"));
        index += 1;
    }
    Ok(candidate)
}

fn format_daily_line(entry: &MemoryEntry) -> String {
    format!(
        "{} | {} | {}",
        format_timestamp(entry.timestamp),
        entry_role(entry),
        entry
            .content
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    )
}

fn format_topic_line(entry: &MemoryEntry) -> String {
    let date = Local
        .timestamp_opt(entry.timestamp as i64, 0)
        .single()
        .unwrap_or_else(|| Local.timestamp_opt(0, 0).earliest().unwrap())
        .format("%Y-%m-%d %H:%M")
        .to_string();
    format!(
        "- [{date}] {} | {}",
        entry_role(entry),
        entry
            .content
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    )
}

fn format_timestamp(timestamp: u64) -> String {
    Local
        .timestamp_opt(timestamp as i64, 0)
        .single()
        .unwrap_or_else(|| Local.timestamp_opt(0, 0).earliest().unwrap())
        .format("%H:%M")
        .to_string()
}

fn format_date(timestamp: u64) -> String {
    Local
        .timestamp_opt(timestamp as i64, 0)
        .single()
        .unwrap_or_else(|| Local.timestamp_opt(0, 0).earliest().unwrap())
        .format("%Y-%m-%d")
        .to_string()
}

fn entry_role(entry: &MemoryEntry) -> &str {
    if let Some(role) = entry.tags.iter().find_map(|tag| match tag.as_str() {
        "assistant" => Some("assistant"),
        "system" => Some("system"),
        "tool" => Some("tool"),
        "user" => Some("user"),
        _ => None,
    }) {
        return role;
    }
    if entry.id.starts_with("assistant_") {
        return "assistant";
    }
    if entry.id.starts_with("system_") {
        return "system";
    }
    if entry.id.starts_with("tool_") {
        return "tool";
    }
    "user"
}

fn render_memory_context(index: String, daily: String, topics: String) -> String {
    let mut sections = Vec::new();
    if !index.trim().is_empty() {
        sections.push(format!("[memory index - Topic Index]\n{index}"));
    }
    if !daily.trim().is_empty() {
        sections.push(format!("[memory daily - Recent Daily Logs]\n{daily}"));
    }
    if !topics.trim().is_empty() {
        sections.push(topics);
    }
    sections.join("\n\n")
}

fn render_index(entries: &[TopicIndexEntry]) -> String {
    let summary_limits = [INDEX_SUMMARY_CHAR_LIMIT, 120, 90, 60, 30, 0];
    for limit in summary_limits {
        let lines = entries
            .iter()
            .map(|entry| {
                if limit == 0 || entry.summary.is_empty() {
                    format!("- {} | shards={}", entry.slug, entry.shards)
                } else {
                    format!(
                        "- {} | shards={} | summary={}",
                        entry.slug,
                        entry.shards,
                        summary_text(&entry.summary, limit)
                    )
                }
            })
            .collect::<Vec<_>>();
        let rendered = lines.join("\n");
        if rendered.len() <= INDEX_CHAR_BUDGET {
            return rendered;
        }
    }

    entries
        .iter()
        .map(|entry| format!("- {} | shards={}", entry.slug, entry.shards))
        .collect::<Vec<_>>()
        .join("\n")
}

fn parse_index_line(line: &str) -> Option<TopicIndexEntry> {
    let trimmed = line.trim().strip_prefix("- ")?;
    let mut parts = trimmed.split(" | ");
    let slug = parts.next()?.trim().to_string();
    let shards_text = parts.next()?.trim().strip_prefix("shards=")?;
    let shards = shards_text.parse::<usize>().ok()?;
    let summary = parts
        .next()
        .and_then(|part| part.trim().strip_prefix("summary="))
        .unwrap_or_default()
        .to_string();
    Some(TopicIndexEntry {
        slug,
        shards,
        summary,
    })
}

async fn latest_topic_summary(path: &Path) -> Result<String> {
    let contents = read_trimmed(path).await?;
    let summary = contents
        .lines()
        .rev()
        .find_map(|line| {
            line.split_once("| ")
                .map(|(_, text)| text.trim().to_string())
        })
        .unwrap_or_default();
    Ok(summary_text(&summary, INDEX_SUMMARY_CHAR_LIMIT))
}

async fn list_markdown_files(dir: &Path) -> Result<Vec<PathBuf>> {
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut paths = Vec::new();
    let mut read_dir = fs::read_dir(dir)
        .await
        .map_err(|error| storage_error(format!("Failed to read dir {}: {error}", dir.display())))?;
    while let Some(entry) = read_dir
        .next_entry()
        .await
        .map_err(|error| storage_error(error.to_string()))?
    {
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) == Some("md") {
            paths.push(path);
        }
    }
    Ok(paths)
}

async fn read_trimmed(path: &Path) -> Result<String> {
    if !path.exists() {
        return Ok(String::new());
    }
    fs::read_to_string(path)
        .await
        .map(|contents| contents.trim().to_string())
        .map_err(|error| storage_error(format!("Failed to read {}: {error}", path.display())))
}

async fn append_text(path: &Path, line: &str) -> Result<()> {
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .await
        .map_err(|error| storage_error(format!("Failed to open {}: {error}", path.display())))?;
    if fs::metadata(path)
        .await
        .map(|metadata| metadata.len() > 0)
        .unwrap_or(false)
    {
        file.write_all(b"\n")
            .await
            .map_err(|error| storage_error(format!("Failed to append newline: {error}")))?;
    }
    file.write_all(line.as_bytes())
        .await
        .map_err(|error| storage_error(format!("Failed to append text: {error}")))
}

fn with_trailing_newline(contents: &str) -> String {
    let trimmed = contents.trim_end_matches('\n');
    if trimmed.is_empty() {
        return String::new();
    }
    format!("{trimmed}\n")
}

async fn write_file_atomically(path: &Path, contents: &str) -> Result<()> {
    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| storage_error(format!("Invalid file name for {}", path.display())))?;
    let temp_path = path.with_file_name(format!("{file_name}.tmp"));
    fs::write(&temp_path, contents).await.map_err(|error| {
        storage_error(format!("Failed to write {}: {error}", temp_path.display()))
    })?;
    if path.exists() {
        fs::remove_file(path).await.map_err(|error| {
            storage_error(format!("Failed to replace {}: {error}", path.display()))
        })?;
    }
    fs::rename(&temp_path, path).await.map_err(|error| {
        storage_error(format!(
            "Failed to rename {} to {}: {error}",
            temp_path.display(),
            path.display()
        ))
    })
}

fn normalize_summary_lines(summary: &str) -> Option<Vec<String>> {
    let lines = summary
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .take(3)
        .map(str::to_string)
        .collect::<Vec<_>>();
    if lines.is_empty() {
        return None;
    }
    Some(lines)
}

fn parse_daily_line(line: &str) -> Option<(&str, &str, &str)> {
    let mut parts = line.splitn(3, " | ");
    let time_text = parts.next()?.trim();
    let role = parts.next()?.trim();
    let body = parts.next()?.trim();
    if time_text.is_empty() || role.is_empty() || body.is_empty() {
        return None;
    }
    Some((time_text, role, body))
}

fn daily_timestamp(date: &str, time_text: &str) -> Result<u64> {
    let day = chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d")
        .map_err(|error| storage_error(format!("Invalid daily date '{date}': {error}")))?;
    let (hour, minute) = time_text
        .split_once(':')
        .ok_or_else(|| storage_error(format!("Invalid daily time '{time_text}'")))?;
    let hour = hour
        .parse::<u32>()
        .map_err(|error| storage_error(format!("Invalid daily hour: {error}")))?;
    let minute = minute
        .parse::<u32>()
        .map_err(|error| storage_error(format!("Invalid daily minute: {error}")))?;
    let local_time = Local
        .with_ymd_and_hms(day.year(), day.month(), day.day(), hour, minute, 0)
        .single()
        .ok_or_else(|| storage_error("Invalid local time while exporting memory".to_string()))?;
    Ok(local_time.timestamp() as u64)
}
