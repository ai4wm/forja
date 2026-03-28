use crate::compressor::CompressedEntry;
use chrono::{DateTime, Local};
use forja_core::error::{ForjaError, Result};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tokio::fs;
use tokio::io::AsyncWriteExt;

#[derive(Debug, Clone)]
pub struct LongTermStore {
    path: PathBuf,
}

impl LongTermStore {
    pub async fn new(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .await
                .map_err(|error| ForjaError::Storage(format!("Failed to create memory dir: {error}")))?;
        }
        if !fs::try_exists(&path)
            .await
            .map_err(|error| ForjaError::Storage(format!("Failed to inspect long-term store: {error}")))?
        {
            fs::write(&path, "")
                .await
                .map_err(|error| ForjaError::Storage(format!("Failed to create long-term store: {error}")))?;
        }

        Ok(Self { path })
    }

    pub async fn add(&self, entry: &CompressedEntry) -> Result<()> {
        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .await
            .map_err(|error| ForjaError::Storage(format!("Failed to open long-term store: {error}")))?;

        let block = render_entry(entry);
        file.write_all(block.as_bytes())
            .await
            .map_err(|error| ForjaError::Storage(format!("Failed to append long-term entry: {error}")))?;
        Ok(())
    }

    pub async fn load(&self) -> Result<Vec<CompressedEntry>> {
        let raw = fs::read_to_string(&self.path)
            .await
            .map_err(|error| ForjaError::Storage(format!("Failed to read long-term store: {error}")))?;
        Ok(parse_entries(&raw))
    }

    pub async fn search(&self, query: &str, limit: usize) -> Result<Vec<CompressedEntry>> {
        let entries = self.load().await?;
        Ok(rank_entries(&entries, query, limit))
    }

    pub async fn entry_count(&self) -> Result<usize> {
        Ok(self.load().await?.len())
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

pub fn longterm_path(base_dir: &Path, agent_name: Option<&str>) -> PathBuf {
    match agent_name {
        Some(agent_name) => base_dir
            .join("agents")
            .join(agent_name)
            .join("memory")
            .join("longterm.md"),
        None => base_dir.join("longterm.md"),
    }
}

fn render_entry(entry: &CompressedEntry) -> String {
    let timestamp = entry.timestamp.to_rfc3339();
    let tags = if entry.keywords.is_empty() {
        String::new()
    } else {
        entry.keywords
            .iter()
            .map(|keyword| format!("#{keyword}"))
            .collect::<Vec<_>>()
            .join(" ")
    };
    let code_section = if entry.code_snippets.is_empty() {
        String::new()
    } else {
        format!(
            "\n```text\n{}\n```\n",
            entry.code_snippets.join("\n---\n")
        )
    };

    format!(
        "## [{timestamp}]\nsummary: {}\nkeywords: {tags}\noriginal_count: {}\n{}\n",
        entry.summary.trim(),
        entry.original_count,
        code_section,
    )
}

fn parse_entries(raw: &str) -> Vec<CompressedEntry> {
    raw.split("## [")
        .filter_map(|section| {
            let section = section.trim();
            if section.is_empty() {
                return None;
            }
            parse_entry_block(section)
        })
        .collect()
}

fn parse_entry_block(section: &str) -> Option<CompressedEntry> {
    let (timestamp_raw, rest) = section.split_once("]\n")?;
    let timestamp = DateTime::parse_from_rfc3339(timestamp_raw)
        .ok()?
        .with_timezone(&Local);

    let mut summary = String::new();
    let mut keywords = Vec::new();
    let mut original_count = 0usize;
    let mut code_snippets = Vec::new();
    let mut in_code_block = false;
    let mut current_code = Vec::new();

    for line in rest.lines() {
        if line == "```text" {
            in_code_block = true;
            current_code.clear();
            continue;
        }
        if line == "```" {
            in_code_block = false;
            if !current_code.is_empty() {
                code_snippets.push(current_code.join("\n"));
            }
            current_code.clear();
            continue;
        }
        if in_code_block {
            current_code.push(line.to_string());
            continue;
        }

        if let Some(value) = line.strip_prefix("summary: ") {
            summary = value.trim().to_string();
        } else if let Some(value) = line.strip_prefix("keywords: ") {
            keywords = value
                .split_whitespace()
                .map(str::trim)
                .map(|keyword| keyword.trim_start_matches('#').to_string())
                .filter(|keyword| !keyword.is_empty())
                .collect();
        } else if let Some(value) = line.strip_prefix("original_count: ") {
            original_count = value.trim().parse().ok()?;
        }
    }

    Some(CompressedEntry {
        timestamp,
        summary,
        keywords,
        original_count,
        code_snippets,
    })
}

fn rank_entries(entries: &[CompressedEntry], query: &str, limit: usize) -> Vec<CompressedEntry> {
    let query_terms = tokenize(query);
    if query_terms.is_empty() {
        return entries.iter().rev().take(limit).cloned().collect();
    }

    let documents = entries
        .iter()
        .map(entry_text)
        .collect::<Vec<_>>();
    let average_length = documents
        .iter()
        .map(|document| document.len() as f64)
        .sum::<f64>()
        / documents.len().max(1) as f64;

    let document_frequency = query_terms
        .iter()
        .map(|term| {
            let count = documents
                .iter()
                .filter(|document| document.contains_key(term))
                .count();
            (term.clone(), count)
        })
        .collect::<HashMap<_, _>>();

    let mut scored = entries
        .iter()
        .zip(documents.iter())
        .filter_map(|(entry, document)| {
            let score = query_terms.iter().fold(0.0, |acc, term| {
                let tf = *document.get(term).unwrap_or(&0) as f64;
                if tf == 0.0 {
                    return acc;
                }
                let df = *document_frequency.get(term).unwrap_or(&0) as f64;
                let idf = ((documents.len() as f64 - df + 0.5) / (df + 0.5) + 1.0).ln();
                let length = document.values().sum::<usize>() as f64;
                let k1 = 1.2;
                let b = 0.75;
                let denominator = tf + k1 * (1.0 - b + b * (length / average_length.max(1.0)));
                acc + idf * (tf * (k1 + 1.0) / denominator)
            });

            if score > 0.0 {
                Some((score, entry.clone()))
            } else {
                None
            }
        })
        .collect::<Vec<_>>();

    scored.sort_by(|left, right| right.0.total_cmp(&left.0));
    scored.into_iter().take(limit).map(|(_, entry)| entry).collect()
}

fn entry_text(entry: &CompressedEntry) -> HashMap<String, usize> {
    let mut counts = HashMap::new();
    let mut parts = vec![entry.summary.clone()];
    parts.push(entry.keywords.join(" "));
    parts.extend(entry.code_snippets.clone());

    for token in tokenize(&parts.join(" ")) {
        *counts.entry(token).or_insert(0) += 1;
    }

    counts
}

fn tokenize(text: &str) -> Vec<String> {
    text.split(|character: char| !character.is_alphanumeric() && character != '_' && character != '.')
        .map(str::trim)
        .filter(|token| !token.is_empty())
        .map(|token| token.to_lowercase())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{LongTermStore, longterm_path};
    use crate::compressor::CompressedEntry;
    use chrono::Local;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_temp_dir(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        std::env::temp_dir().join(format!("forja_longterm_{name}_{nanos}"))
    }

    #[tokio::test]
    async fn long_term_store_adds_and_searches_entries() {
        let base_dir = unique_temp_dir("search");
        let store = LongTermStore::new(longterm_path(&base_dir, None)).await.unwrap();
        let deploy_entry = CompressedEntry {
            timestamp: Local::now(),
            summary: "Deploy completed with vercel".to_string(),
            keywords: vec!["deploy".to_string(), "vercel".to_string()],
            original_count: 4,
            code_snippets: vec!["deploy.sh".to_string()],
        };
        let review_entry = CompressedEntry {
            timestamp: Local::now(),
            summary: "Code review covered auth.rs".to_string(),
            keywords: vec!["review".to_string(), "auth.rs".to_string()],
            original_count: 3,
            code_snippets: vec!["auth.rs".to_string()],
        };

        store.add(&deploy_entry).await.unwrap();
        store.add(&review_entry).await.unwrap();

        let results = store.search("deploy vercel", 1).await.unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].summary, deploy_entry.summary);
        assert_eq!(store.entry_count().await.unwrap(), 2);
    }
}
