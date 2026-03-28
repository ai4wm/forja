use std::collections::HashMap;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

const DEFAULT_ASSISTANT_NAME: &str = "Forja";
const DEFAULT_LANGUAGE: &str = "auto";
const DEFAULT_TONE: &str = "friendly";
const DEFAULT_USER_NAME: &str = "User";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdentityProfile {
    pub user_name: String,
    pub assistant_name: String,
    pub language: String,
    pub tone: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BootstrapProfile {
    pub identity: IdentityProfile,
}

#[derive(Debug, Clone)]
pub struct BootstrapOutcome {
    pub profile: BootstrapProfile,
}

#[derive(Debug, Clone)]
pub struct BootstrapPaths {
    pub forja_dir: PathBuf,
    pub identity_path: PathBuf,
    pub user_path: PathBuf,
    pub legacy_user_prompt_path: PathBuf,
}

#[derive(Debug, Clone)]
struct UserDocument {
    user_name: Option<String>,
    raw_content: String,
}

enum OnboardingMode {
    Initial,
    Reset,
}

impl BootstrapPaths {
    pub fn from_home(home_dir: impl AsRef<Path>) -> Self {
        let forja_dir = home_dir.as_ref().join(".forja");
        Self {
            identity_path: forja_dir.join("identity.md"),
            user_path: forja_dir.join("user.md"),
            legacy_user_prompt_path: forja_dir.join("USER.md"),
            forja_dir,
        }
    }
}

impl BootstrapProfile {
    pub fn greeting(&self) -> String {
        let user_name = &self.identity.user_name;
        let assistant_name = &self.identity.assistant_name;
        format!("Hello, {user_name}! I am {assistant_name}. How can I help?")
    }
}

pub fn default_paths() -> BootstrapPaths {
    let home_dir = std::env::var("FORJA_HOME_DIR")
        .map(PathBuf::from)
        .ok()
        .or_else(dirs_next::home_dir)
        .unwrap_or_else(|| PathBuf::from("."));
    BootstrapPaths::from_home(home_dir)
}

pub fn ensure_bootstrap(paths: &BootstrapPaths) -> io::Result<BootstrapOutcome> {
    if paths.identity_path.exists() {
        let profile = load_profile(paths)?;
        return Ok(BootstrapOutcome { profile });
    }

    run_onboarding(paths, OnboardingMode::Initial)
}

pub fn reset_bootstrap(paths: &BootstrapPaths) -> io::Result<BootstrapOutcome> {
    run_onboarding(paths, OnboardingMode::Reset)
}

pub fn compose_system_prompt_prefix(paths: &BootstrapPaths) -> io::Result<String> {
    let mut sections = Vec::new();

    if let Some(identity_raw) = read_trimmed_if_exists(&paths.identity_path)? {
        sections.push(format!("[identity.md]\n{identity_raw}"));
    }

    if let Some(user_doc) = load_user_document(paths)? {
        let raw = user_doc.raw_content.trim();
        if !raw.is_empty() {
            sections.push(format!("[user.md]\n{raw}"));
        }
    }

    if sections.is_empty() {
        return Ok(String::new());
    }

    Ok(format!(
        "Bootstrap identity profile. Apply these rules before any project prompt.\n\n{}",
        sections.join("\n\n")
    ))
}

fn load_profile(paths: &BootstrapPaths) -> io::Result<BootstrapProfile> {
    let identity_raw = std::fs::read_to_string(&paths.identity_path)?;
    let (frontmatter, _) = parse_frontmatter_document(&identity_raw).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "identity.md frontmatter could not be parsed",
        )
    })?;

    let user_doc = load_user_document(paths)?;
    let legacy_user_name = user_doc.as_ref().and_then(|doc| doc.user_name.clone());
    let legacy_assistant_name = frontmatter.get("name").cloned();
    let legacy_tone = frontmatter.get("tone").cloned();

    let profile = if frontmatter.contains_key("assistant_name") || frontmatter.contains_key("user_name")
    {
        IdentityProfile {
            user_name: required_frontmatter_value(&frontmatter, "user_name")?,
            assistant_name: required_frontmatter_value(&frontmatter, "assistant_name")?,
            language: optional_frontmatter_value(&frontmatter, "language")
                .unwrap_or_else(|| DEFAULT_LANGUAGE.to_string()),
            tone: optional_frontmatter_value(&frontmatter, "tone")
                .unwrap_or_else(|| DEFAULT_TONE.to_string()),
        }
    } else {
        IdentityProfile {
            user_name: legacy_user_name.unwrap_or_else(|| DEFAULT_USER_NAME.to_string()),
            assistant_name: legacy_assistant_name.unwrap_or_else(|| DEFAULT_ASSISTANT_NAME.to_string()),
            language: DEFAULT_LANGUAGE.to_string(),
            tone: legacy_tone.unwrap_or_else(|| DEFAULT_TONE.to_string()),
        }
    };

    Ok(BootstrapProfile { identity: profile })
}

fn run_onboarding(paths: &BootstrapPaths, mode: OnboardingMode) -> io::Result<BootstrapOutcome> {
    std::fs::create_dir_all(&paths.forja_dir)?;

    let existing_profile = load_profile(paths).ok();
    let default_user_name = match mode {
        OnboardingMode::Initial => existing_profile
            .as_ref()
            .map(|profile| profile.identity.user_name.as_str()),
        OnboardingMode::Reset => existing_profile
            .as_ref()
            .map(|profile| profile.identity.user_name.as_str()),
    };
    let default_assistant_name = existing_profile
        .as_ref()
        .map(|profile| profile.identity.assistant_name.as_str())
        .unwrap_or(DEFAULT_ASSISTANT_NAME);
    let default_language = existing_profile
        .as_ref()
        .map(|profile| profile.identity.language.as_str())
        .unwrap_or(DEFAULT_LANGUAGE);
    let default_tone = existing_profile
        .as_ref()
        .map(|profile| profile.identity.tone.as_str())
        .unwrap_or(DEFAULT_TONE);

    let user_name = prompt_required("What should I call you?", default_user_name)?;
    let assistant_name = prompt_with_default("What's my name?", default_assistant_name)?;
    let language = prompt_with_default("What language do you prefer?", default_language)?;
    let tone = prompt_with_default(
        "What tone do you prefer? (formal/casual/friendly)",
        default_tone,
    )?;

    let profile = BootstrapProfile {
        identity: IdentityProfile {
            user_name,
            assistant_name,
            language,
            tone,
        },
    };

    write_identity_file(paths, &profile.identity)?;

    Ok(BootstrapOutcome { profile })
}

fn load_user_document(paths: &BootstrapPaths) -> io::Result<Option<UserDocument>> {
    if let Some(doc) = read_user_document(&paths.user_path)? {
        return Ok(Some(doc));
    }

    if paths.legacy_user_prompt_path != paths.user_path {
        return read_user_document(&paths.legacy_user_prompt_path);
    }

    Ok(None)
}

fn read_user_document(path: &Path) -> io::Result<Option<UserDocument>> {
    if !path.exists() {
        return Ok(None);
    }

    let raw_content = std::fs::read_to_string(path)?;
    if let Some((frontmatter, _body)) = parse_frontmatter_document(&raw_content) {
        return Ok(Some(UserDocument {
            user_name: optional_frontmatter_value(&frontmatter, "name"),
            raw_content,
        }));
    }

    Ok(Some(UserDocument {
        user_name: None,
        raw_content,
    }))
}

fn write_identity_file(paths: &BootstrapPaths, identity: &IdentityProfile) -> io::Result<()> {
    let content = format!(
        "---\nuser_name: {}\nassistant_name: {}\nlanguage: {}\ntone: {}\n---\n",
        quote_yaml_value(&identity.user_name),
        quote_yaml_value(&identity.assistant_name),
        quote_yaml_value(&identity.language),
        quote_yaml_value(&identity.tone),
    );
    std::fs::write(&paths.identity_path, content)
}

fn prompt_with_default(question: &str, default: &str) -> io::Result<String> {
    print!("{question} [{default}] > ");
    std::io::stdout().flush()?;

    let mut line = String::new();
    std::io::stdin().read_line(&mut line)?;
    let trimmed = line.trim();

    if trimmed.is_empty() {
        return Ok(default.to_string());
    }

    Ok(trimmed.to_string())
}

fn prompt_required(question: &str, default: Option<&str>) -> io::Result<String> {
    loop {
        if let Some(default) = default {
            print!("{question} [{default}] > ");
        } else {
            print!("{question} > ");
        }
        std::io::stdout().flush()?;

        let mut line = String::new();
        std::io::stdin().read_line(&mut line)?;
        let trimmed = line.trim();

        if trimmed.is_empty() {
            if let Some(default) = default {
                return Ok(default.to_string());
            }
            continue;
        }

        return Ok(trimmed.to_string());
    }
}

fn read_trimmed_if_exists(path: &Path) -> io::Result<Option<String>> {
    if !path.exists() {
        return Ok(None);
    }

    let raw = std::fs::read_to_string(path)?;
    let trimmed = raw.trim().to_string();
    if trimmed.is_empty() {
        return Ok(None);
    }

    Ok(Some(trimmed))
}

fn required_frontmatter_value(
    values: &HashMap<String, String>,
    key: &str,
) -> io::Result<String> {
    optional_frontmatter_value(values, key).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("missing required frontmatter key: {key}"),
        )
    })
}

fn optional_frontmatter_value(values: &HashMap<String, String>, key: &str) -> Option<String> {
    values
        .get(key)
        .map(|value| value.trim().to_string())
        .map(|value| unquote_frontmatter_value(&value))
        .filter(|value| !value.is_empty())
}

fn parse_frontmatter_document(content: &str) -> Option<(HashMap<String, String>, String)> {
    let normalized = content.replace("\r\n", "\n");
    let stripped = normalized.strip_prefix("---\n")?;
    let (frontmatter, body) = stripped.split_once("\n---\n")?;

    let mut values = HashMap::new();
    for line in frontmatter.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let (key, value) = line.split_once(':')?;
        values.insert(key.trim().to_string(), value.trim().to_string());
    }

    Some((values, body.to_string()))
}

fn quote_yaml_value(value: &str) -> String {
    let escaped = value
        .replace('\\', "\\\\")
        .replace('"', "\\\"");
    format!("\"{escaped}\"")
}

fn unquote_frontmatter_value(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.len() >= 2 && trimmed.starts_with('"') && trimmed.ends_with('"') {
        return trimmed[1..trimmed.len() - 1]
            .replace("\\\"", "\"")
            .replace("\\\\", "\\");
    }

    trimmed.to_string()
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
        std::env::temp_dir().join(format!("forja_{name}_{nanos}"))
    }

    #[test]
    fn compose_prompt_prefix_includes_identity_file() {
        let home_dir = unique_temp_dir("bootstrap_prompt");
        let paths = BootstrapPaths::from_home(&home_dir);
        std::fs::create_dir_all(&paths.forja_dir).unwrap();
        std::fs::write(
            &paths.identity_path,
            "---\nuser_name: \"Owner\"\nassistant_name: \"Forja\"\nlanguage: \"auto\"\ntone: \"friendly\"\n---\n",
        )
        .unwrap();

        let prompt = compose_system_prompt_prefix(&paths).unwrap();

        assert!(prompt.contains("[identity.md]"));
        assert!(prompt.contains("assistant_name: \"Forja\""));
        assert!(prompt.contains("user_name: \"Owner\""));

        let _ = std::fs::remove_dir_all(&home_dir);
    }

    #[test]
    fn load_profile_reads_new_identity_frontmatter() {
        let home_dir = unique_temp_dir("bootstrap_identity_profile");
        let paths = BootstrapPaths::from_home(&home_dir);
        std::fs::create_dir_all(&paths.forja_dir).unwrap();
        std::fs::write(
            &paths.identity_path,
            "---\nuser_name: \"Owner\"\nassistant_name: \"Forja\"\nlanguage: \"auto\"\ntone: \"friendly\"\n---\n",
        )
        .unwrap();

        let profile = load_profile(&paths).unwrap();

        assert_eq!(profile.identity.user_name, "Owner");
        assert_eq!(profile.identity.assistant_name, "Forja");
        assert_eq!(profile.identity.language, "auto");
        assert_eq!(profile.identity.tone, "friendly");

        let _ = std::fs::remove_dir_all(&home_dir);
    }

    #[test]
    fn legacy_user_body_survives_in_prompt_prefix() {
        let home_dir = unique_temp_dir("bootstrap_user_body");
        let paths = BootstrapPaths::from_home(&home_dir);
        std::fs::create_dir_all(&paths.forja_dir).unwrap();
        std::fs::write(
            &paths.identity_path,
            "---\nuser_name: \"Owner\"\nassistant_name: \"Forja\"\nlanguage: \"auto\"\ntone: \"friendly\"\n---\n",
        )
        .unwrap();
        std::fs::write(&paths.user_path, "---\nname: Owner\n---\n\nKeep legacy prompt").unwrap();

        let prompt = compose_system_prompt_prefix(&paths).unwrap();

        assert!(prompt.contains("[user.md]"));
        assert!(prompt.contains("Keep legacy prompt"));

        let _ = std::fs::remove_dir_all(&home_dir);
    }
}
