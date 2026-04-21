use super::classifier::normalize_text;
use super::journal::{
    DreamJournal, JournalFileStamp, clear_pending_journal, load_pending_journal, relative_path,
    write_pending_journal,
};
use super::{
    Storage, append_text, legacy_archive_path, list_markdown_files, parse_daily_line,
    parse_topic_file_name, read_trimmed, storage_error, topic_file_name, with_trailing_newline,
};
use crate::estimate_tokens;
use chrono::{Duration, Local, NaiveDateTime, TimeZone};
use forja_core::error::Result;
use forja_core::traits::{DreamRunOutcome, DreamRunStatus, DreamTrigger};
use std::collections::{BTreeSet, HashSet};
use std::path::{Path, PathBuf};
use tokio::fs;

const DREAM_DAILY_LIMIT: usize = 7;
const DREAM_STALE_DAYS: i64 = 30;
const DREAM_TOKEN_BUDGET: usize = 2_000;
const DREAM_DUPLICATE_THRESHOLD: usize = 80;

#[derive(Debug, Clone)]
struct TopicGroup {
    slug: String,
    paths: Vec<PathBuf>,
    lines: Vec<String>,
    last_timestamp: Option<u64>,
}

#[derive(Debug, Clone)]
struct DailyEvidence {
    path: PathBuf,
    tokens: BTreeSet<String>,
}

#[derive(Debug, Clone)]
struct DreamSnapshot {
    topic_groups: Vec<TopicGroup>,
    daily_evidence: Vec<DailyEvidence>,
    file_stamps: Vec<JournalFileStamp>,
}

#[derive(Debug, Clone)]
struct ArchiveMove {
    source: PathBuf,
    archive_label: String,
}

#[derive(Debug, Clone)]
struct PlannedTopic {
    slug: String,
    original_paths: Vec<PathBuf>,
    shard_bodies: Vec<String>,
}

#[derive(Debug, Clone)]
struct DreamPlan {
    archived_topics: Vec<String>,
    merged_topics: Vec<String>,
    split_topics: Vec<String>,
    archives: Vec<ArchiveMove>,
    active_topics: Vec<PlannedTopic>,
}

impl Storage {
    pub(super) async fn execute_dream(&self, trigger: DreamTrigger) -> Result<DreamRunOutcome> {
        let recovered = load_pending_journal(&self.dream_state_file)
            .await?
            .is_some();
        if recovered {
            clear_pending_journal(&self.dream_state_file).await?;
        }

        let snapshot = self.build_dream_snapshot().await?;
        let journal = DreamJournal::new(trigger, snapshot.file_stamps.clone());
        write_pending_journal(&self.dream_state_file, &journal).await?;

        let plan = self.build_dream_plan(snapshot);
        if !self.snapshot_matches_current(&journal.files).await? {
            clear_pending_journal(&self.dream_state_file).await?;
            return Ok(DreamRunOutcome {
                status: DreamRunStatus::AbortedConflict,
                summary: "dream commit aborted because memory files changed during analysis".to_string(),
                archived_topics: plan.archived_topics,
                merged_topics: plan.merged_topics,
                split_topics: plan.split_topics,
                completed_at: None,
            });
        }

        self.apply_dream_plan(&plan).await?;
        self.rebuild_index().await?;
        let completed_at = Local::now().timestamp() as u64;
        self.append_dream_log(trigger, &plan, recovered, completed_at)
            .await?;
        clear_pending_journal(&self.dream_state_file).await?;

        Ok(DreamRunOutcome {
            status: if recovered {
                DreamRunStatus::Recovered
            } else {
                DreamRunStatus::Completed
            },
            summary: self.plan_summary(&plan),
            archived_topics: plan.archived_topics,
            merged_topics: plan.merged_topics,
            split_topics: plan.split_topics,
            completed_at: Some(completed_at),
        })
    }

    pub(super) async fn read_latest_dream_timestamp(&self) -> Result<Option<u64>> {
        let mut latest: Option<u64> = None;
        for path in list_markdown_files(&self.dreams_dir).await? {
            let modified = file_mtime_secs(&path).await?;
            latest = match (latest, modified) {
                (Some(current), Some(candidate)) => Some(current.max(candidate)),
                (Some(current), None) => Some(current),
                (None, candidate) => candidate,
            };
        }
        Ok(latest)
    }

    async fn build_dream_snapshot(&self) -> Result<DreamSnapshot> {
        let topic_groups = self.read_topic_groups().await?;
        let daily_evidence = self.read_recent_daily_evidence().await?;
        let mut tracked_paths = vec![self.index_file.clone()];
        for group in &topic_groups {
            tracked_paths.extend(group.paths.iter().cloned());
        }
        tracked_paths.extend(daily_evidence.iter().map(|evidence| evidence.path.clone()));

        let mut seen = HashSet::new();
        let mut file_stamps = Vec::new();
        for path in tracked_paths {
            let relative = relative_path(&self.base_dir, &path);
            if !seen.insert(relative.clone()) {
                continue;
            }
            file_stamps.push(JournalFileStamp {
                path: relative.to_string_lossy().to_string(),
                mtime_secs: file_mtime_secs(&path).await?,
            });
        }

        Ok(DreamSnapshot {
            topic_groups,
            daily_evidence,
            file_stamps,
        })
    }

    fn build_dream_plan(&self, snapshot: DreamSnapshot) -> DreamPlan {
        let mut groups = snapshot.topic_groups;
        groups.sort_by(|left, right| left.slug.cmp(&right.slug));

        let mut consumed = HashSet::new();
        let mut active_groups = Vec::new();
        let mut archives = Vec::new();
        let mut archived_topics = Vec::new();
        let mut merged_topics = Vec::new();
        let mut split_topics = Vec::new();

        for index in 0..groups.len() {
            let group = &groups[index];
            if consumed.contains(&group.slug) {
                continue;
            }

            let mut cluster = vec![group.clone()];
            consumed.insert(group.slug.clone());

            for other in groups.iter().skip(index + 1) {
                if consumed.contains(&other.slug) {
                    continue;
                }
                if duplicate_overlap(&group.slug, &other.slug) > DREAM_DUPLICATE_THRESHOLD {
                    consumed.insert(other.slug.clone());
                    cluster.push(other.clone());
                }
            }

            let canonical = select_canonical_group(&cluster);
            let canonical_slug = canonical.slug.clone();
            let canonical_paths = canonical.paths.clone();
            let merged_lines = dedupe_lines(cluster.iter().flat_map(|item| item.lines.clone()));
            let last_timestamp = cluster.iter().filter_map(|item| item.last_timestamp).max();
            let recent_daily_evidence = snapshot
                .daily_evidence
                .iter()
                .any(|daily| has_recent_daily_evidence(&canonical_slug, &daily.tokens));

            for duplicate in cluster.iter().filter(|item| item.slug != canonical_slug) {
                merged_topics.push(format!("{} -> {}", duplicate.slug, canonical_slug));
                for path in &duplicate.paths {
                    archives.push(ArchiveMove {
                        source: path.clone(),
                        archive_label: duplicate.slug.clone(),
                    });
                }
            }

            if is_stale_topic(last_timestamp, recent_daily_evidence) {
                archived_topics.push(canonical_slug.clone());
                for path in &canonical_paths {
                    archives.push(ArchiveMove {
                        source: path.clone(),
                        archive_label: canonical_slug.clone(),
                    });
                }
                continue;
            }

            let shard_bodies = shard_topic_lines(&canonical_slug, &merged_lines);
            if shard_bodies.len() > 1 {
                split_topics.push(canonical_slug.clone());
            }

            active_groups.push(PlannedTopic {
                slug: canonical_slug,
                original_paths: canonical_paths,
                shard_bodies,
            });
        }

        DreamPlan {
            archived_topics,
            merged_topics,
            split_topics,
            archives,
            active_topics: active_groups,
        }
    }

    async fn snapshot_matches_current(&self, files: &[JournalFileStamp]) -> Result<bool> {
        for file in files {
            let path = self.base_dir.join(&file.path);
            if file_mtime_secs(&path).await? != file.mtime_secs {
                return Ok(false);
            }
        }
        Ok(true)
    }

    async fn apply_dream_plan(&self, plan: &DreamPlan) -> Result<()> {
        for archive in &plan.archives {
            if !archive.source.exists() {
                continue;
            }
            let archive_path = legacy_archive_path(
                &self.archive_dir,
                &archive.archive_label,
                "md",
            )
            .await?;
            fs::rename(&archive.source, &archive_path).await.map_err(|error| {
                storage_error(format!(
                    "Failed to archive {} to {}: {error}",
                    archive.source.display(),
                    archive_path.display()
                ))
            })?;
        }

        for topic in &plan.active_topics {
            let desired_paths = topic
                .shard_bodies
                .iter()
                .enumerate()
                .map(|(index, _)| self.topics_dir.join(topic_file_name(&topic.slug, index + 1)))
                .collect::<Vec<_>>();

            for (path, body) in desired_paths.iter().zip(&topic.shard_bodies) {
                fs::write(path, with_trailing_newline(body))
                    .await
                    .map_err(|error| storage_error(format!("Failed to write {}: {error}", path.display())))?;
            }

            for original in &topic.original_paths {
                if !desired_paths.iter().any(|candidate| candidate == original) && original.exists() {
                    fs::remove_file(original).await.map_err(|error| {
                        storage_error(format!("Failed to remove {}: {error}", original.display()))
                    })?;
                }
            }
        }

        Ok(())
    }

    async fn append_dream_log(
        &self,
        trigger: DreamTrigger,
        plan: &DreamPlan,
        recovered: bool,
        completed_at: u64,
    ) -> Result<()> {
        let date = Local::now().format("%Y-%m-%d").to_string();
        let timestamp = Local
            .timestamp_opt(completed_at as i64, 0)
            .single()
            .unwrap()
            .format("%Y-%m-%d %H:%M:%S")
            .to_string();
        let mut lines = vec![
            format!("## {timestamp} | trigger={trigger:?}"),
            format!("status: {}", if recovered { "recovered" } else { "completed" }),
            format!("summary: {}", self.plan_summary(plan)),
        ];

        for merged in &plan.merged_topics {
            lines.push(format!("- merged duplicate topics: {merged}"));
        }
        for archived in &plan.archived_topics {
            lines.push(format!("- archived stale topic: {archived}"));
        }
        for split in &plan.split_topics {
            lines.push(format!("- split oversized topic: {split}"));
        }
        if plan.merged_topics.is_empty() && plan.archived_topics.is_empty() && plan.split_topics.is_empty() {
            lines.push("- no maintenance changes were required".to_string());
        }

        let path = self.dreams_dir.join(format!("{date}.md"));
        append_text(&path, &lines.join("\n")).await
    }

    fn plan_summary(&self, plan: &DreamPlan) -> String {
        format!(
            "merged={} archived={} split={}",
            plan.merged_topics.len(),
            plan.archived_topics.len(),
            plan.split_topics.len()
        )
    }

    async fn read_topic_groups(&self) -> Result<Vec<TopicGroup>> {
        let mut groups = Vec::new();
        let mut slugs = list_markdown_files(&self.topics_dir).await?;
        slugs.sort_by_key(|path| parse_topic_file_name(path).map(|(slug, _)| slug));

        let mut seen = HashSet::new();
        for path in slugs {
            let Some((slug, _)) = parse_topic_file_name(&path) else {
                continue;
            };
            if !seen.insert(slug.clone()) {
                continue;
            }
            let paths = self.topic_shards(&slug).await?;
            let mut lines = Vec::new();
            let mut last_timestamp: Option<u64> = None;
            for shard in &paths {
                let contents = read_trimmed(shard).await?;
                for line in contents.lines().map(str::trim).filter(|line| line.starts_with("- [")) {
                    lines.push(line.to_string());
                    if let Some(timestamp) = parse_topic_timestamp(line) {
                        last_timestamp = Some(last_timestamp.map_or(timestamp, |current| current.max(timestamp)));
                    }
                }
            }
            groups.push(TopicGroup {
                slug,
                paths,
                lines,
                last_timestamp,
            });
        }

        Ok(groups)
    }

    async fn read_recent_daily_evidence(&self) -> Result<Vec<DailyEvidence>> {
        let mut daily_paths = list_markdown_files(&self.daily_dir).await?;
        daily_paths.sort_by(|left, right| right.cmp(left));
        let mut evidence = Vec::new();

        for path in daily_paths.into_iter().take(DREAM_DAILY_LIMIT) {
            let contents = read_trimmed(&path).await?;
            let mut tokens = BTreeSet::new();
            for line in contents.lines() {
                if let Some((_, _, body)) = parse_daily_line(line) {
                    tokens.extend(tokenize(body));
                }
            }
            evidence.push(DailyEvidence { path, tokens });
        }

        Ok(evidence)
    }
}

fn select_canonical_group(groups: &[TopicGroup]) -> &TopicGroup {
    groups
        .iter()
        .max_by(|left, right| {
            left.last_timestamp
                .cmp(&right.last_timestamp)
                .then_with(|| right.slug.cmp(&left.slug))
        })
        .unwrap_or(&groups[0])
}

fn dedupe_lines<I>(lines: I) -> Vec<String>
where
    I: IntoIterator<Item = String>,
{
    let mut seen = HashSet::new();
    let mut unique = Vec::new();
    for line in lines {
        if seen.insert(line.clone()) {
            unique.push(line);
        }
    }
    unique
}

fn duplicate_overlap(left: &str, right: &str) -> usize {
    let left_tokens = tokenize(left);
    let right_tokens = tokenize(right);
    token_overlap_percent(&left_tokens, &right_tokens)
}

fn has_recent_daily_evidence(slug: &str, daily_tokens: &BTreeSet<String>) -> bool {
    let slug_tokens = tokenize(slug);
    if slug_tokens.is_empty() {
        return false;
    }
    let shared = slug_tokens
        .iter()
        .filter(|token| daily_tokens.contains(*token))
        .count();
    shared * 100 / slug_tokens.len() > DREAM_DUPLICATE_THRESHOLD
}

fn token_overlap_percent(left: &BTreeSet<String>, right: &BTreeSet<String>) -> usize {
    if left.is_empty() || right.is_empty() {
        return 0;
    }
    let shared = left.iter().filter(|token| right.contains(*token)).count();
    let base = left.len().max(right.len());
    shared * 100 / base
}

fn is_stale_topic(last_timestamp: Option<u64>, has_recent_daily_evidence: bool) -> bool {
    if has_recent_daily_evidence {
        return false;
    }
    let Some(last_timestamp) = last_timestamp else {
        return false;
    };
    let age = Local::now().timestamp().saturating_sub(last_timestamp as i64);
    age >= Duration::days(DREAM_STALE_DAYS).num_seconds()
}

fn shard_topic_lines(slug: &str, lines: &[String]) -> Vec<String> {
    if lines.is_empty() {
        return vec![format!("# Topic: {slug}")];
    }

    let header = format!("# Topic: {slug}");
    let header_tokens = estimate_tokens(&header);
    let mut shards = Vec::new();
    let mut current = vec![header.clone()];
    let mut current_tokens = header_tokens;
    for line in lines {
        let line_tokens = estimate_tokens(line);
        if current.len() > 1 && current_tokens.saturating_add(line_tokens) > DREAM_TOKEN_BUDGET {
            shards.push(current.join("\n"));
            current = vec![header.clone(), line.clone()];
            current_tokens = header_tokens.saturating_add(line_tokens);
        } else {
            current.push(line.clone());
            current_tokens = current_tokens.saturating_add(line_tokens);
        }
    }
    shards.push(current.join("\n"));
    shards
}

async fn file_mtime_secs(path: &Path) -> Result<Option<u64>> {
    if !path.exists() {
        return Ok(None);
    }
    let metadata = fs::metadata(path)
        .await
        .map_err(|error| storage_error(format!("Failed to read metadata for {}: {error}", path.display())))?;
    let modified = metadata
        .modified()
        .map_err(|error| storage_error(format!("Failed to read modified time for {}: {error}", path.display())))?;
    let seconds = modified
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|error| storage_error(format!("Invalid modified time for {}: {error}", path.display())))?
        .as_secs();
    Ok(Some(seconds))
}

fn parse_topic_timestamp(line: &str) -> Option<u64> {
    let prefix = line.strip_prefix("- [")?.split(']').next()?;
    let naive = NaiveDateTime::parse_from_str(prefix, "%Y-%m-%d %H:%M").ok()?;
    let local = Local.from_local_datetime(&naive).single()?;
    Some(local.timestamp() as u64)
}

fn tokenize(value: &str) -> BTreeSet<String> {
    normalize_text(value)
        .split_whitespace()
        .filter(|token| !token.is_empty())
        .map(str::to_string)
        .collect()
}
