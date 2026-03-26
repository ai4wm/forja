use crate::error::{ForjaError, Result};
use crate::traits::LlmProvider;
use crate::types::{Content, Message, Role};
use chrono::Local;
use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

const KNOWLEDGE_HEADER: &str = "[knowledge - Topic-based Persistent Knowledge]\nRelated long-term knowledge is below. Use it when it helps answer the user accurately.";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TopicEntry {
    pub topic: String,
    pub filename: String,
    pub entry: String,
}

#[derive(Debug, Clone)]
pub struct KnowledgeManager {
    pub base_dir: PathBuf,
}

impl KnowledgeManager {
    pub fn new(base_dir: impl Into<PathBuf>) -> Self {
        Self {
            base_dir: base_dir.into(),
        }
    }

    pub async fn detect_topic(
        &self,
        message: &str,
        provider: &dyn LlmProvider,
    ) -> Result<Option<TopicEntry>> {
        let trimmed = message.trim();
        if trimmed.is_empty() {
            return Ok(None);
        }

        let response = match provider
            .chat(
                &[
                    Message::text(
                        Role::System,
                        "You extract durable knowledge from a user message. Reply with JSON only or NONE.",
                        None,
                    ),
                    Message::text(Role::User, self.build_detection_prompt(trimmed), None),
                ],
                None,
            )
            .await
        {
            Ok(response) => response,
            Err(_) => return Ok(None),
        };

        let Content::Text { text, .. } = response.content else {
            return Ok(None);
        };
        let trimmed = text.trim();

        if trimmed.eq_ignore_ascii_case("NONE") {
            return Ok(None);
        }

        let parsed: TopicEntry = match serde_json::from_str(trimmed) {
            Ok(parsed) => parsed,
            Err(_) => return Ok(None),
        };

        Ok(normalize_topic_entry(parsed))
    }

    pub fn save_entry(&self, topic_entry: &TopicEntry) -> Result<()> {
        self.ensure_base_dir()?;

        let Some(entry) = normalize_topic_entry(topic_entry.clone()) else {
            return Ok(());
        };
        let path = self.base_dir.join(&entry.filename);
        let today = Local::now().format("%Y-%m-%d");
        let line = format!("- [{today}] {}\n", entry.entry);
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .map_err(storage_error)?;
        file.write_all(line.as_bytes()).map_err(storage_error)
    }

    pub fn load_relevant(&self, query: &str) -> Result<String> {
        self.ensure_base_dir()?;

        let normalized_query = normalize_text(query);
        if normalized_query.is_empty() {
            return Ok(String::new());
        }

        let file_names = self
            .list_files()
            .into_iter()
            .filter(|file_name| file_matches_query(file_name, &normalized_query))
            .collect::<Vec<_>>();

        self.load_context_for_files(&file_names)
    }

    pub fn load_all_context(&self) -> Result<String> {
        self.ensure_base_dir()?;
        let file_names = self.list_files();

        self.load_context_for_files(&file_names)
    }

    pub fn list_files(&self) -> Vec<String> {
        if fs::create_dir_all(&self.base_dir).is_err() {
            return Vec::new();
        }

        let mut files = match fs::read_dir(&self.base_dir) {
            Ok(entries) => entries
                .filter_map(|entry| {
                    let entry = entry.ok()?;
                    let path = entry.path();
                    if !is_markdown_file(&path) {
                        return None;
                    }
                    path.file_name()
                        .and_then(|name| name.to_str())
                        .map(|name| name.to_string())
                })
                .collect::<Vec<_>>(),
            Err(_) => Vec::new(),
        };
        files.sort();
        files
    }

    fn build_detection_prompt(&self, message: &str) -> String {
        let known_files = self.list_files();
        let known_files = if known_files.is_empty() {
            "projects.md, infra.md, people.md, decisions.md".to_string()
        } else {
            known_files.join(", ")
        };

        format!(
            "Analyze the message below.\n\
If it contains durable knowledge worth keeping beyond the current conversation, respond with JSON only.\n\
If not, respond with NONE only.\n\
\n\
JSON schema:\n\
{{\"topic\":\"projects|infra|people|decisions|custom\",\"filename\":\"xxx.md\",\"entry\":\"single line\"}}\n\
\n\
Rules:\n\
- Prefer projects.md, infra.md, people.md, or decisions.md when they fit.\n\
- filename must be a markdown file name only, with no directory path.\n\
- entry must be a concise single line with no bullet marker and no date.\n\
- Store only durable facts, decisions, ongoing project context, people information, or device/infrastructure details.\n\
- Ignore short-lived chat, generic greetings, and temporary wording.\n\
\n\
Known files: {known_files}\n\
\n\
Message:\n\
{message}"
        )
    }

    fn ensure_base_dir(&self) -> Result<()> {
        fs::create_dir_all(&self.base_dir).map_err(storage_error)
    }

    fn load_context_for_files(&self, file_names: &[String]) -> Result<String> {
        let mut sections = Vec::new();
        for file_name in file_names {
            let path = self.base_dir.join(file_name);
            let contents = fs::read_to_string(path).map_err(storage_error)?;
            let trimmed = contents.trim();
            if trimmed.is_empty() {
                continue;
            }

            sections.push(format!("## {file_name}\n{trimmed}"));
        }

        if sections.is_empty() {
            return Ok(String::new());
        }

        Ok(format!("{KNOWLEDGE_HEADER}\n\n{}", sections.join("\n\n")))
    }
}

fn normalize_topic_entry(entry: TopicEntry) -> Option<TopicEntry> {
    let topic = normalize_text(&entry.topic).replace(' ', "-");
    let filename = sanitize_filename(&entry.filename)?;
    let entry_text = sanitize_entry(&entry.entry);

    if topic.is_empty() || entry_text.is_empty() {
        return None;
    }

    Some(TopicEntry {
        topic,
        filename,
        entry: entry_text,
    })
}

fn sanitize_filename(filename: &str) -> Option<String> {
    let raw = filename
        .replace('\\', "/")
        .split('/')
        .next_back()
        .unwrap_or_default()
        .trim()
        .to_lowercase();

    if raw.is_empty() {
        return None;
    }

    let stem = raw.strip_suffix(".md").unwrap_or(&raw);
    let cleaned = stem
        .chars()
        .map(|char| match char {
            'a'..='z' | '0'..='9' => char,
            '-' | '_' => char,
            _ => '-',
        })
        .collect::<String>()
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-");

    if cleaned.is_empty() {
        return None;
    }

    Some(format!("{cleaned}.md"))
}

fn sanitize_entry(entry: &str) -> String {
    entry
        .replace(['\r', '\n'], " ")
        .replace('|', " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn file_matches_query(file_name: &str, normalized_query: &str) -> bool {
    let stem = file_name.strip_suffix(".md").unwrap_or(file_name);
    if normalized_query.contains(stem) || stem.contains(normalized_query) {
        return true;
    }

    knowledge_keywords(stem)
        .iter()
        .any(|keyword| normalized_query.contains(keyword))
}

fn knowledge_keywords(stem: &str) -> &'static [&'static str] {
    match stem {
        "projects" => &["project", "projects", "forja", "agx"],
        "infra" => &[
            "infra",
            "infrastructure",
            "device",
            "devices",
            "tailscale",
            "adb",
            "s25",
            "ultra",
        ],
        "people" => &["people", "person", "team", "owner", "who"],
        "decisions" => &["decision", "decisions", "decide", "policy", "rule", "rules"],
        _ => &[],
    }
}

fn normalize_text(value: &str) -> String {
    value
        .to_lowercase()
        .chars()
        .map(|char| match char {
            'a'..='z' | '0'..='9' => char,
            _ => ' ',
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn is_markdown_file(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("md"))
}

fn storage_error(error: std::io::Error) -> ForjaError {
    ForjaError::Storage(error.to_string())
}
