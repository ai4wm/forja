use crate::skill_eval::{SkillTestCase, SkillTestSuite};
use serde::Deserialize;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

static SKILL_SUMMARY: OnceLock<Mutex<String>> = OnceLock::new();
static ACTIVE_SKILL_CONTEXT: OnceLock<Mutex<Option<String>>> = OnceLock::new();

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Skill {
    pub name: String,
    pub description: String,
    pub triggers: Vec<String>,
    pub scripts: Vec<String>,
    pub env: Vec<String>,
    pub instructions: String,
    pub base_dir: PathBuf,
    pub tests: Vec<SkillTestCase>,
}

#[derive(Debug, Default)]
pub struct SkillLoader {
    skills_dir: PathBuf,
    skills: Vec<Skill>,
}

impl SkillLoader {
    pub fn new(skills_dir: PathBuf) -> Self {
        Self {
            skills_dir,
            skills: Vec::new(),
        }
    }

    pub fn load_all(&mut self) -> std::io::Result<Vec<Skill>> {
        fs::create_dir_all(&self.skills_dir)?;
        let mut skills = fs::read_dir(&self.skills_dir)?
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.path())
            .filter(|path| path.is_dir())
            .filter_map(|path| load_skill_dir(path).ok())
            .collect::<Vec<_>>();

        skills.sort_by(|left, right| left.name.cmp(&right.name));
        self.skills = skills;
        Ok(self.skills.clone())
    }

    pub fn find_by_trigger(&self, input: &str) -> Option<&Skill> {
        let normalized = normalize(input);
        self.skills.iter().find(|skill| {
            skill.triggers.iter().any(|trigger| {
                let trigger = normalize(trigger);
                normalized == trigger
                    || normalized.starts_with(&format!("{trigger} "))
                    || normalized.starts_with(&format!("please {trigger} "))
            })
        })
    }

    pub fn find_by_name(&self, name: &str) -> Option<&Skill> {
        let target = normalize(name);
        self.skills
            .iter()
            .find(|skill| normalize(&skill.name) == target)
    }

    pub fn skill_test_suite(&self, name: &str) -> Option<SkillTestSuite> {
        self.find_by_name(name).map(|skill| SkillTestSuite {
            skill_name: skill.name.clone(),
            cases: skill.tests.clone(),
        })
    }

    pub fn skills(&self) -> &[Skill] {
        &self.skills
    }

    pub fn skills_dir(&self) -> &Path {
        &self.skills_dir
    }

    pub fn summary(&self) -> String {
        if self.skills.is_empty() {
            return String::new();
        }

        let mut lines = vec!["Available skills:".to_string()];
        lines.extend(self.skills.iter().map(|skill| {
            format!(
                "- {}: {} [triggers: {}]",
                skill.name,
                skill.description,
                skill.triggers.join(", ")
            )
        }));
        lines.join("\n")
    }
}

pub fn default_skills_dir() -> PathBuf {
    let home_dir = std::env::var("FORJA_HOME_DIR")
        .map(PathBuf::from)
        .ok()
        .or_else(|| std::env::var("HOME").ok().map(PathBuf::from))
        .or_else(|| std::env::var("USERPROFILE").ok().map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from("."));
    skills_dir_from_home(&home_dir)
}

pub fn skills_dir_from_home(home_dir: &Path) -> PathBuf {
    home_dir.join(".forja").join("skills")
}

pub fn skill_catalog_summary() -> String {
    skill_summary_lock()
        .lock()
        .map(|summary| summary.clone())
        .unwrap_or_default()
}

pub fn set_skill_catalog_summary(summary: String) {
    if let Ok(mut current) = skill_summary_lock().lock() {
        *current = summary;
    }
}

pub fn active_skill_context() -> Option<String> {
    active_skill_context_lock()
        .lock()
        .ok()
        .and_then(|context| context.clone())
}

pub fn set_active_skill_context(context: String) {
    if let Ok(mut current) = active_skill_context_lock().lock() {
        *current = Some(context);
    }
}

pub fn clear_active_skill_context() {
    if let Ok(mut current) = active_skill_context_lock().lock() {
        *current = None;
    }
}

fn skill_summary_lock() -> &'static Mutex<String> {
    SKILL_SUMMARY.get_or_init(|| Mutex::new(String::new()))
}

fn active_skill_context_lock() -> &'static Mutex<Option<String>> {
    ACTIVE_SKILL_CONTEXT.get_or_init(|| Mutex::new(None))
}

fn load_skill_dir(path: PathBuf) -> std::io::Result<Skill> {
    let skill_doc_path = path.join("SKILL.md");
    let raw = fs::read_to_string(skill_doc_path)?;
    let parsed = parse_skill_document(&raw).ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "SKILL.md frontmatter could not be parsed",
        )
    })?;

    Ok(Skill {
        name: parsed.frontmatter.name,
        description: parsed.frontmatter.description,
        triggers: parsed.frontmatter.triggers,
        scripts: parsed.frontmatter.scripts,
        env: parsed.frontmatter.env,
        instructions: parsed.instructions.trim().to_string(),
        base_dir: path,
        tests: parsed.frontmatter.tests,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ParsedSkillDocument {
    frontmatter: SkillFrontmatter,
    instructions: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
struct SkillFrontmatter {
    name: String,
    description: String,
    #[serde(default)]
    triggers: Vec<String>,
    #[serde(default)]
    scripts: Vec<String>,
    #[serde(default)]
    env: Vec<String>,
    #[serde(default)]
    tests: Vec<SkillTestCase>,
}

fn parse_skill_document(content: &str) -> Option<ParsedSkillDocument> {
    let normalized = content.replace("\r\n", "\n");
    let stripped = normalized.strip_prefix("---\n")?;
    let (frontmatter_raw, body) = stripped.split_once("\n---\n")?;
    let frontmatter = serde_yaml::from_str::<SkillFrontmatter>(frontmatter_raw).ok()?;
    Some(ParsedSkillDocument {
        frontmatter,
        instructions: body.to_string(),
    })
}

fn normalize(value: &str) -> String {
    value
        .trim()
        .to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
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
        std::env::temp_dir().join(format!("forja_skill_{name}_{nanos}"))
    }

    #[test]
    fn loader_parses_skill_document() {
        let home_dir = unique_temp_dir("parse");
        let skills_dir = skills_dir_from_home(&home_dir);
        let skill_dir = skills_dir.join("hello-world");
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(
            skill_dir.join("SKILL.md"),
            "---\nname: hello-world\ndescription: Say hello\ntriggers:\n  - hello\nscripts:\n  - hello.sh\nenv:\n  - DEMO_KEY\ntests:\n  - name: hello output\n    input: hello\n    expected_contains:\n      - hello\n---\n\n# Hello\n\nRun hello.sh",
        )
        .unwrap();

        let mut loader = SkillLoader::new(skills_dir);
        let skills = loader.load_all().unwrap();

        assert_eq!(skills[0].name, "hello-world");
        assert_eq!(skills[0].triggers, vec!["hello"]);
        assert_eq!(skills[0].scripts, vec!["hello.sh"]);
        assert_eq!(skills[0].env, vec!["DEMO_KEY"]);
        assert_eq!(skills[0].tests.len(), 1);
        assert_eq!(skills[0].tests[0].name, "hello output");
        assert!(skills[0].instructions.contains("# Hello"));
    }

    #[test]
    fn loader_summary_lists_names_and_triggers() {
        let home_dir = unique_temp_dir("summary");
        let skills_dir = skills_dir_from_home(&home_dir);
        let skill_dir = skills_dir.join("hello-world");
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(
            skill_dir.join("SKILL.md"),
            "---\nname: hello-world\ndescription: Say hello\ntriggers:\n  - hello\nscripts:\n  - hello.sh\nenv:\n  - DEMO_KEY\n---\n\n# Hello",
        )
        .unwrap();

        let mut loader = SkillLoader::new(skills_dir);
        let _ = loader.load_all().unwrap();
        let summary = loader.summary();

        assert!(summary.contains("Available skills:"));
        assert!(summary.contains("hello-world: Say hello [triggers: hello]"));
    }
}
