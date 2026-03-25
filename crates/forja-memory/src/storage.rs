use chrono::{Local, NaiveDate, TimeZone};
use forja_core::error::{ForjaError as Error, Result};
use forja_core::types::MemoryEntry;
use serde::Deserialize;
use std::fmt::Display;
use std::path::{Path, PathBuf};
use tokio::fs;
use tokio::io::AsyncWriteExt;

#[derive(Debug, Deserialize)]
struct LegacyFrontmatter {
    id: String,
    timestamp: u64,
    tags: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct Storage {
    memory_file: PathBuf,
    archive_dir: PathBuf,
    sessions_dir: PathBuf,
    sessions_backup_dir: PathBuf,
}

impl Storage {
    pub async fn init(memory_file: impl AsRef<Path>) -> Result<Self> {
        let memory_file = memory_file.as_ref().to_path_buf();
        let base_dir = memory_file
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."));

        fs::create_dir_all(&base_dir)
            .await
            .map_err(|error| Error::Storage(format!("Failed to create memory dir: {error}")))?;

        let storage = Self {
            memory_file,
            archive_dir: base_dir.join("archive"),
            sessions_dir: base_dir.join("sessions"),
            sessions_backup_dir: base_dir.join("sessions.bak"),
        };

        storage.ensure_memory_file().await?;
        storage.migrate_legacy_sessions().await?;

        Ok(storage)
    }

    pub async fn append_entry(&self, entry: &MemoryEntry) -> Result<()> {
        if should_skip_entry(entry) {
            return Ok(());
        }

        let line = format_memory_line(entry);
        let entry_date = format_date(entry.timestamp);
        let block = match self.last_recorded_date().await? {
            Some(last_date) if last_date == entry_date => line,
            _ => format!("--- {entry_date} ---\n{line}"),
        };

        self.append_block(&block).await
    }

    pub async fn flush_and_summarize<F, E>(&self, summarizer: F) -> Result<()>
    where
        F: Fn(String) -> std::result::Result<String, E>,
        E: Display,
    {
        let contents = self.read_all().await?;
        if contents.trim().is_empty() {
            return Ok(());
        }

        let sections = parse_memory_sections(&contents);
        let today = Local::now().format("%Y-%m-%d").to_string();
        let mut archived_blocks = Vec::new();
        let mut rewritten_sections = Vec::with_capacity(sections.len());

        for section in sections {
            match section {
                MemorySection::DateBlock(block)
                    if block.date != today
                        && !block.lines.is_empty()
                        && !self.archive_path(&block.date).exists() =>
                {
                    let original_block = block.render();
                    match summarizer(original_block.clone()) {
                        Ok(summary) => {
                            if let Some(summary_lines) = normalize_summary_lines(&summary) {
                                archived_blocks.push(ArchivedBlock {
                                    date: block.date.clone(),
                                    contents: original_block,
                                });
                                rewritten_sections.push(MemorySection::DateBlock(DateBlock {
                                    date: block.date,
                                    lines: summary_lines,
                                }));
                            } else {
                                rewritten_sections.push(MemorySection::DateBlock(block));
                            }
                        }
                        Err(error) => {
                            eprintln!("[Memory] summarize skipped for {}: {error}", block.date);
                            rewritten_sections.push(MemorySection::DateBlock(block));
                        }
                    }
                }
                _ => rewritten_sections.push(section),
            }
        }

        if archived_blocks.is_empty() {
            return Ok(());
        }

        fs::create_dir_all(&self.archive_dir)
            .await
            .map_err(|error| Error::Storage(format!("Failed to create archive dir: {error}")))?;

        for archived_block in &archived_blocks {
            fs::write(
                self.archive_path(&archived_block.date),
                format_with_trailing_newline(&archived_block.contents),
            )
            .await
            .map_err(|error| Error::Storage(format!("Failed to write archive file: {error}")))?;
        }

        fs::write(
            &self.memory_file,
            format_with_trailing_newline(&render_memory_sections(&rewritten_sections)),
        )
        .await
        .map_err(|error| Error::Storage(format!("Failed to rewrite memory file: {error}")))
    }

    pub async fn read_all(&self) -> Result<String> {
        self.ensure_memory_file().await?;

        fs::read_to_string(&self.memory_file)
            .await
            .map_err(|error| Error::Storage(format!("Failed to read memory file: {error}")))
    }

    async fn ensure_memory_file(&self) -> Result<()> {
        if fs::try_exists(&self.memory_file)
            .await
            .map_err(|error| Error::Storage(format!("Failed to inspect memory file: {error}")))?
        {
            return Ok(());
        }

        fs::write(&self.memory_file, "")
            .await
            .map_err(|error| Error::Storage(format!("Failed to create memory file: {error}")))
    }

    async fn append_block(&self, block: &str) -> Result<()> {
        let normalized_block = block.trim_matches('\n');
        if normalized_block.is_empty() {
            return Ok(());
        }

        let existing = self.read_all().await?;
        let needs_separator = !existing.is_empty() && !existing.ends_with('\n');

        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.memory_file)
            .await
            .map_err(|error| Error::Storage(format!("Failed to open memory file: {error}")))?;

        if needs_separator {
            file.write_all(b"\n")
                .await
                .map_err(|error| Error::Storage(format!("Failed to append separator: {error}")))?;
        }

        file.write_all(normalized_block.as_bytes())
            .await
            .map_err(|error| Error::Storage(format!("Failed to append memory block: {error}")))?;
        file.write_all(b"\n")
            .await
            .map_err(|error| Error::Storage(format!("Failed to finalize memory block: {error}")))
    }

    async fn last_recorded_date(&self) -> Result<Option<String>> {
        let contents = self.read_all().await?;
        Ok(contents.lines().rev().find_map(parse_date_header))
    }

    fn archive_path(&self, date: &str) -> PathBuf {
        self.archive_dir.join(format!("{date}.md"))
    }

    async fn migrate_legacy_sessions(&self) -> Result<()> {
        if !self.sessions_dir.exists() {
            return Ok(());
        }

        let entries = self.read_legacy_entries().await?;
        if entries.is_empty() {
            return Ok(());
        }

        let file_count = entries.len();

        for entry in &entries {
            self.append_entry(entry).await?;
        }

        let memory_size = fs::metadata(&self.memory_file)
            .await
            .map_err(|error| Error::Storage(format!("Failed to read memory file metadata: {error}")))?
            .len();
        println!(
            "[Memory] 마이그레이션: sessions/*.md {file_count}개 파일 → memory.md ({})",
            format_byte_size(memory_size)
        );

        let backup_dir = self.next_sessions_backup_dir();
        fs::rename(&self.sessions_dir, &backup_dir)
            .await
            .map_err(|error| Error::Storage(format!("Failed to rename sessions dir: {error}")))?;

        if backup_dir == self.sessions_backup_dir {
            println!("[Memory] sessions/ → sessions.bak/ 완료");
        } else if let Some(name) = backup_dir.file_name().and_then(|name| name.to_str()) {
            println!("[Memory] sessions/ → {name}/ 완료");
        }

        Ok(())
    }

    fn next_sessions_backup_dir(&self) -> PathBuf {
        if !self.sessions_backup_dir.exists() {
            return self.sessions_backup_dir.clone();
        }

        let base_name = self
            .sessions_backup_dir
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("sessions.bak");

        for index in 1.. {
            let candidate = self.sessions_backup_dir.with_file_name(format!("{base_name}.{index}"));
            if !candidate.exists() {
                return candidate;
            }
        }

        unreachable!("infinite iterator should always find an available backup path")
    }

    async fn read_legacy_entries(&self) -> Result<Vec<MemoryEntry>> {
        let mut entries = Vec::new();
        let mut read_dir = fs::read_dir(&self.sessions_dir)
            .await
            .map_err(|error| Error::Storage(format!("Failed to read sessions dir: {error}")))?;

        while let Some(entry) = read_dir
            .next_entry()
            .await
            .map_err(|error| Error::Storage(error.to_string()))?
        {
            let path = entry.path();
            if path.is_file()
                && path.extension().and_then(|extension| extension.to_str()) == Some("md")
                && let Ok(memory_entry) = Self::parse_legacy_file(&path).await
            {
                entries.push(memory_entry);
            }
        }

        entries.sort_by(|left, right| left.timestamp.cmp(&right.timestamp));
        Ok(entries)
    }

    async fn parse_legacy_file(path: &Path) -> Result<MemoryEntry> {
        let content = fs::read_to_string(path)
            .await
            .map_err(|error| Error::Storage(format!("Failed to read file {:?}: {error}", path)))?;

        if !content.starts_with("---\n") && !content.starts_with("---\r\n") {
            return Err(Error::Storage(format!("Invalid Frontmatter format in {:?}", path)));
        }

        let parts: Vec<&str> = content.splitn(3, "---").collect();
        if parts.len() < 3 {
            return Err(Error::Storage(format!(
                "Cannot parse Markdown YAML block in {:?}",
                path
            )));
        }

        let frontmatter: LegacyFrontmatter = serde_yaml::from_str(parts[1].trim()).map_err(|error| {
            Error::Deserialization(format!("YAML deserialize error: {error}"))
        })?;

        Ok(MemoryEntry {
            id: frontmatter.id,
            timestamp: frontmatter.timestamp,
            tags: frontmatter.tags,
            content: parts[2].trim().to_string(),
            score: 0.0,
            metadata: Default::default(),
        })
    }
}

fn format_memory_line(entry: &MemoryEntry) -> String {
    let time_text = format_timestamp(entry.timestamp);
    let role = entry_role(entry);
    let normalized = normalize_content(&entry.content);
    format!("{time_text} | {role} | {normalized}")
}

fn format_timestamp(timestamp: u64) -> String {
    let local_time = Local
        .timestamp_opt(timestamp as i64, 0)
        .single()
        .unwrap_or_else(|| Local.timestamp_opt(0, 0).earliest().unwrap());
    local_time.format("%H:%M").to_string()
}

fn format_date(timestamp: u64) -> String {
    let local_time = Local
        .timestamp_opt(timestamp as i64, 0)
        .single()
        .unwrap_or_else(|| Local.timestamp_opt(0, 0).earliest().unwrap());
    local_time.format("%Y-%m-%d").to_string()
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
    if entry.id.starts_with("user_") {
        return "user";
    }

    "memory"
}

fn normalize_content(content: &str) -> String {
    content.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn should_skip_entry(entry: &MemoryEntry) -> bool {
    entry.content.contains("MockStream")
}

fn parse_date_header(line: &str) -> Option<String> {
    let date_text = line.strip_prefix("--- ")?.strip_suffix(" ---")?;
    NaiveDate::parse_from_str(date_text, "%Y-%m-%d")
        .ok()
        .map(|_| date_text.to_string())
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

fn render_memory_sections(sections: &[MemorySection]) -> String {
    sections
        .iter()
        .map(MemorySection::render)
        .filter(|section| !section.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

fn format_with_trailing_newline(contents: &str) -> String {
    let trimmed = contents.trim_end_matches('\n');
    if trimmed.is_empty() {
        return String::new();
    }

    format!("{trimmed}\n")
}

fn parse_memory_sections(contents: &str) -> Vec<MemorySection> {
    let mut sections = Vec::new();
    let mut raw_lines = Vec::new();
    let mut current_block: Option<DateBlock> = None;

    for line in contents.lines() {
        if let Some(date) = parse_date_header(line) {
            if !raw_lines.is_empty() {
                sections.push(MemorySection::Raw(raw_lines.join("\n")));
                raw_lines.clear();
            }

            if let Some(block) = current_block.take() {
                sections.push(MemorySection::DateBlock(block));
            }

            current_block = Some(DateBlock {
                date,
                lines: Vec::new(),
            });
            continue;
        }

        if let Some(block) = &mut current_block {
            block.lines.push(line.to_string());
        } else {
            raw_lines.push(line.to_string());
        }
    }

    if !raw_lines.is_empty() {
        sections.push(MemorySection::Raw(raw_lines.join("\n")));
    }

    if let Some(block) = current_block {
        sections.push(MemorySection::DateBlock(block));
    }

    sections
}

#[derive(Debug, Clone)]
struct ArchivedBlock {
    date: String,
    contents: String,
}

#[derive(Debug, Clone)]
struct DateBlock {
    date: String,
    lines: Vec<String>,
}

impl DateBlock {
    fn render(&self) -> String {
        if self.lines.is_empty() {
            return format!("--- {} ---", self.date);
        }

        format!("--- {} ---\n{}", self.date, self.lines.join("\n"))
    }
}

#[derive(Debug, Clone)]
enum MemorySection {
    Raw(String),
    DateBlock(DateBlock),
}

impl MemorySection {
    fn render(&self) -> String {
        match self {
            Self::Raw(contents) => contents.clone(),
            Self::DateBlock(block) => block.render(),
        }
    }
}

fn format_byte_size(bytes: u64) -> String {
    if bytes < 1024 {
        return format!("{bytes}B");
    }

    let kilobytes = bytes as f64 / 1024.0;
    if kilobytes < 1024.0 {
        return format!("{kilobytes:.1}KB");
    }

    let megabytes = kilobytes / 1024.0;
    format!("{megabytes:.1}MB")
}
