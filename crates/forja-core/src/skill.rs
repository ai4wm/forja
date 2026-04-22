use crate::error::{ForjaError, Result};
use chrono::Utc;
use rusqlite::{Connection, params};
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

const SKILL_FILE_NAME: &str = "SKILL.md";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillDefinition {
    pub name: String,
    pub trigger: String,
    pub description: String,
    pub body: String,
    pub source_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillSummary {
    pub name: String,
    pub trigger: String,
    pub description: String,
    pub source_path: PathBuf,
    pub success_count: u32,
    pub failure_count: u32,
    pub last_error: Option<String>,
    pub suggestion: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillExecutionRecord {
    pub skill_name: String,
    pub success_count: u32,
    pub failure_count: u32,
    pub last_run: Option<String>,
    pub last_error: Option<String>,
    pub suggestion: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillStep {
    pub language: String,
    pub command: String,
}

#[derive(Deserialize)]
struct SkillFrontmatter {
    name: String,
    trigger: String,
    description: String,
}

#[derive(Clone)]
pub struct SkillRegistry {
    db: Arc<Mutex<Connection>>,
    skills: Arc<Mutex<HashMap<String, SkillDefinition>>>,
}

impl SkillRegistry {
    pub fn new(db_path: &Path, skill_roots: &[PathBuf]) -> Result<Self> {
        let connection =
            Connection::open(db_path).map_err(|error| ForjaError::Storage(error.to_string()))?;
        connection
            .execute(
                "CREATE TABLE IF NOT EXISTS skill_runs (
                    skill_name TEXT PRIMARY KEY,
                    success_count INTEGER DEFAULT 0,
                    failure_count INTEGER DEFAULT 0,
                    last_run TEXT,
                    last_error TEXT,
                    suggestion TEXT
                )",
                [],
            )
            .map_err(|error| ForjaError::Storage(error.to_string()))?;

        let registry = Self {
            db: Arc::new(Mutex::new(connection)),
            skills: Arc::new(Mutex::new(HashMap::new())),
        };
        registry.reload(skill_roots)?;
        Ok(registry)
    }

    pub fn reload(&self, skill_roots: &[PathBuf]) -> Result<()> {
        let mut loaded = HashMap::new();
        for root in skill_roots {
            if !root.exists() {
                continue;
            }
            collect_skill_files(root, &mut loaded)?;
        }

        let mut skills = self
            .skills
            .lock()
            .map_err(|error| ForjaError::Storage(error.to_string()))?;
        *skills = loaded;
        Ok(())
    }

    pub fn list_skills(&self) -> Result<Vec<SkillSummary>> {
        let skills = self
            .skills
            .lock()
            .map_err(|error| ForjaError::Storage(error.to_string()))?;
        let mut names = skills.keys().cloned().collect::<Vec<_>>();
        names.sort();

        let mut summaries = Vec::new();
        for name in names {
            let definition = skills.get(&name).expect("skill key must exist");
            let record = self.load_record(&definition.name)?;
            summaries.push(SkillSummary {
                name: definition.name.clone(),
                trigger: definition.trigger.clone(),
                description: definition.description.clone(),
                source_path: definition.source_path.clone(),
                success_count: record.success_count,
                failure_count: record.failure_count,
                last_error: record.last_error,
                suggestion: record.suggestion,
            });
        }

        Ok(summaries)
    }

    pub fn find_by_name(&self, name: &str) -> Result<Option<SkillDefinition>> {
        let normalized = normalize_skill_name(name);
        let skills = self
            .skills
            .lock()
            .map_err(|error| ForjaError::Storage(error.to_string()))?;

        Ok(skills
            .values()
            .find(|skill| {
                normalize_skill_name(&skill.name) == normalized
                    || normalize_skill_name(&skill.trigger) == normalized
            })
            .cloned())
    }

    pub fn match_trigger(&self, input: &str) -> Result<Option<SkillDefinition>> {
        let normalized = input.trim().to_lowercase();
        if normalized.is_empty() {
            return Ok(None);
        }

        let skills = self
            .skills
            .lock()
            .map_err(|error| ForjaError::Storage(error.to_string()))?;
        Ok(skills
            .values()
            .find(|skill| normalized.contains(&skill.trigger.to_lowercase()))
            .cloned())
    }

    pub fn extract_shell_steps(&self, skill: &SkillDefinition) -> Vec<SkillStep> {
        extract_shell_steps(&skill.body)
    }

    pub fn record_success(&self, name: &str) -> Result<()> {
        let connection = self.lock_connection()?;
        connection
            .execute(
                "INSERT INTO skill_runs (skill_name, success_count, failure_count, last_run, last_error, suggestion)
                 VALUES (?1, 1, 0, ?2, NULL, NULL)
                 ON CONFLICT(skill_name) DO UPDATE SET
                    success_count = success_count + 1,
                    last_run = excluded.last_run,
                    last_error = NULL,
                    suggestion = NULL",
                params![name, Utc::now().to_rfc3339()],
            )
            .map_err(|error| ForjaError::Storage(error.to_string()))?;
        Ok(())
    }

    pub fn record_failure(&self, name: &str, error: &str) -> Result<()> {
        let mut record = self.load_record(name)?;
        record.failure_count += 1;
        record.last_run = Some(Utc::now().to_rfc3339());
        record.last_error = Some(error.to_string());
        record.suggestion = if record.failure_count >= 2 {
            Some(
                "Review the trigger wording, validate prerequisites, and tighten the shell steps before retrying."
                    .to_string(),
            )
        } else {
            None
        };

        let connection = self.lock_connection()?;
        connection
            .execute(
                "INSERT INTO skill_runs (skill_name, success_count, failure_count, last_run, last_error, suggestion)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                 ON CONFLICT(skill_name) DO UPDATE SET
                    success_count = excluded.success_count,
                    failure_count = excluded.failure_count,
                    last_run = excluded.last_run,
                    last_error = excluded.last_error,
                    suggestion = excluded.suggestion",
                params![
                    name,
                    record.success_count,
                    record.failure_count,
                    record.last_run,
                    record.last_error,
                    record.suggestion
                ],
            )
            .map_err(|error| ForjaError::Storage(error.to_string()))?;
        Ok(())
    }

    pub fn improvement_suggestion(&self, name: &str) -> Result<Option<String>> {
        Ok(self.load_record(name)?.suggestion)
    }

    fn load_record(&self, name: &str) -> Result<SkillExecutionRecord> {
        let connection = self.lock_connection()?;
        let mut statement = connection
            .prepare(
                "SELECT success_count, failure_count, last_run, last_error, suggestion
                 FROM skill_runs
                 WHERE skill_name = ?1",
            )
            .map_err(|error| ForjaError::Storage(error.to_string()))?;
        let row = statement.query_row([name], |row| {
            Ok(SkillExecutionRecord {
                skill_name: name.to_string(),
                success_count: row.get::<_, i64>(0)? as u32,
                failure_count: row.get::<_, i64>(1)? as u32,
                last_run: row.get(2)?,
                last_error: row.get(3)?,
                suggestion: row.get(4)?,
            })
        });

        match row {
            Ok(record) => Ok(record),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(SkillExecutionRecord {
                skill_name: name.to_string(),
                success_count: 0,
                failure_count: 0,
                last_run: None,
                last_error: None,
                suggestion: None,
            }),
            Err(error) => Err(ForjaError::Storage(error.to_string())),
        }
    }

    fn lock_connection(&self) -> Result<std::sync::MutexGuard<'_, Connection>> {
        self.db
            .lock()
            .map_err(|error| ForjaError::Storage(error.to_string()))
    }
}

pub fn default_skill_roots() -> Vec<PathBuf> {
    let home_root = std::env::var("FORJA_HOME_DIR")
        .map(PathBuf::from)
        .ok()
        .or_else(|| std::env::var("HOME").ok().map(PathBuf::from))
        .or_else(|| std::env::var("USERPROFILE").ok().map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".forja")
        .join("skills");
    let project_root = std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join(".forja")
        .join("skills");

    vec![home_root, project_root]
}

fn collect_skill_files(root: &Path, skills: &mut HashMap<String, SkillDefinition>) -> Result<()> {
    for entry in fs::read_dir(root).map_err(|error| ForjaError::Storage(error.to_string()))? {
        let entry = entry.map_err(|error| ForjaError::Storage(error.to_string()))?;
        let path = entry.path();
        if path.is_dir() {
            collect_skill_files(&path, skills)?;
            continue;
        }

        if path.file_name().and_then(|name| name.to_str()) != Some(SKILL_FILE_NAME) {
            continue;
        }

        let skill = parse_skill_file(&path)?;
        skills.insert(normalize_skill_name(&skill.name), skill);
    }

    Ok(())
}

fn parse_skill_file(path: &Path) -> Result<SkillDefinition> {
    let raw = fs::read_to_string(path).map_err(|error| ForjaError::Storage(error.to_string()))?;
    let (frontmatter, body) = split_frontmatter(&raw).ok_or_else(|| {
        ForjaError::Storage(format!(
            "Skill file '{}' is missing YAML frontmatter",
            path.display()
        ))
    })?;
    let parsed: SkillFrontmatter = serde_yaml::from_str(&frontmatter)
        .map_err(|error| ForjaError::Storage(error.to_string()))?;

    Ok(SkillDefinition {
        name: parsed.name.trim().to_string(),
        trigger: parsed.trigger.trim().to_string(),
        description: parsed.description.trim().to_string(),
        body: body.trim().to_string(),
        source_path: path.to_path_buf(),
    })
}

fn split_frontmatter(content: &str) -> Option<(String, String)> {
    let normalized = content.replace("\r\n", "\n");
    let stripped = normalized.strip_prefix("---\n")?;
    let (frontmatter, body) = stripped.split_once("\n---\n")?;
    Some((frontmatter.to_string(), body.to_string()))
}

fn extract_shell_steps(body: &str) -> Vec<SkillStep> {
    let mut steps = Vec::new();
    let mut current_language = None::<String>;
    let mut current_lines = Vec::new();

    for line in body.lines() {
        if let Some(language) = line.strip_prefix("```") {
            if current_language.is_none() {
                let language = language.trim().to_lowercase();
                if is_shell_language(&language) {
                    current_language = Some(language);
                    current_lines.clear();
                } else {
                    current_language = Some(String::new());
                    current_lines.clear();
                }
                continue;
            }

            let language = current_language.take().unwrap_or_default();
            if !language.is_empty() {
                let command = current_lines.join("\n").trim().to_string();
                if !command.is_empty() {
                    steps.push(SkillStep { language, command });
                }
            }
            current_lines.clear();
            continue;
        }

        if current_language.is_some() {
            current_lines.push(line.to_string());
        }
    }

    steps
}

fn is_shell_language(language: &str) -> bool {
    matches!(
        language,
        "" | "sh" | "shell" | "bash" | "pwsh" | "powershell" | "cmd"
    )
}

fn normalize_skill_name(value: &str) -> String {
    value.trim().to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        std::env::temp_dir().join(format!("forja_skill_{name}_{nanos}"))
    }

    #[test]
    fn skill_registry_loads_skill_markdown_files() {
        let root = temp_dir("load");
        let db_path = root.join("audit.db");
        let skill_dir = root.join("skills").join("demo");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(
            skill_dir.join("SKILL.md"),
            "---\nname: Demo Skill\ntrigger: deploy checklist\ndescription: Run a deploy checklist\n---\n\n```sh\necho hello\n```",
        )
        .unwrap();

        let registry = SkillRegistry::new(&db_path, &[root.join("skills")]).unwrap();
        let skills = registry.list_skills().unwrap();

        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].name, "Demo Skill");
        assert_eq!(skills[0].trigger, "deploy checklist");

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn skill_registry_matches_triggers_case_insensitively() {
        let root = temp_dir("trigger");
        let db_path = root.join("audit.db");
        let skill_dir = root.join("skills");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(
            skill_dir.join("SKILL.md"),
            "---\nname: Release Skill\ntrigger: release checklist\ndescription: Prepare a release\n---\n\n```sh\necho release\n```",
        )
        .unwrap();

        let registry = SkillRegistry::new(&db_path, &[skill_dir.clone()]).unwrap();
        let matched = registry
            .match_trigger("Please run the RELEASE CHECKLIST now")
            .unwrap()
            .unwrap();

        assert_eq!(matched.name, "Release Skill");

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn extract_shell_steps_reads_shell_code_blocks() {
        let body = "Intro\n```sh\necho one\n```\n```powershell\nGet-Date\n```";

        let steps = extract_shell_steps(body);

        assert_eq!(steps.len(), 2);
        assert_eq!(steps[0].command, "echo one");
        assert_eq!(steps[1].command, "Get-Date");
    }

    #[test]
    fn record_failure_generates_basic_suggestion_after_repeats() {
        let root = temp_dir("failure");
        let db_path = root.join("audit.db");
        fs::create_dir_all(&root).unwrap();
        let registry = SkillRegistry::new(&db_path, &[]).unwrap();

        registry
            .record_failure("Demo Skill", "first error")
            .unwrap();
        assert!(
            registry
                .improvement_suggestion("Demo Skill")
                .unwrap()
                .is_none()
        );

        registry
            .record_failure("Demo Skill", "second error")
            .unwrap();
        assert!(
            registry
                .improvement_suggestion("Demo Skill")
                .unwrap()
                .is_some()
        );

        let _ = fs::remove_dir_all(root);
    }
}
