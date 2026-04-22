use std::collections::HashMap;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

const DEFAULT_IDENTITY_NAME: &str = "Forja";
const DEFAULT_TONE: &str = "formal";
const DEFAULT_ROLE: &str = "AI assistant";
const DEFAULT_USER_NAME: &str = "User";
const DOCUMENT_SEPARATOR: &str = "---\n";
const FRONTMATTER_END: &str = "\n---\n";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdentityProfile {
    pub name: String,
    pub role: String,
    pub tone: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserProfile {
    pub name: String,
}

impl IdentityProfile {
    fn from_document(raw: &str) -> io::Result<Self> {
        let (frontmatter, _) = parse_frontmatter_document(raw).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "identity.md frontmatter could not be parsed",
            )
        })?;

        Ok(Self {
            name: required_frontmatter_value(&frontmatter, "name")?,
            role: required_frontmatter_value(&frontmatter, "role")?,
            tone: required_frontmatter_value(&frontmatter, "tone")?,
        })
    }

    fn to_document(&self) -> String {
        let name = sanitize_frontmatter_value(&self.name);
        let role = sanitize_frontmatter_value(&self.role);
        let tone = sanitize_frontmatter_value(&self.tone);
        format!("{DOCUMENT_SEPARATOR}name: {name}\nrole: {role}\ntone: {tone}{FRONTMATTER_END}")
    }
}

impl UserProfile {
    fn from_optional_document(document: Option<UserDocument>) -> Self {
        document
            .and_then(|doc| doc.profile)
            .unwrap_or_else(|| Self {
                name: DEFAULT_USER_NAME.to_string(),
            })
    }

    fn to_document(&self, preserved_body: Option<&str>) -> String {
        let name = sanitize_frontmatter_value(&self.name);
        let mut content = format!("{DOCUMENT_SEPARATOR}name: {name}{FRONTMATTER_END}");

        if let Some(body) = preserved_body {
            let body = body.trim();
            if !body.is_empty() {
                content.push('\n');
                content.push_str(body);
                content.push('\n');
            }
        }

        content
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BootstrapProfile {
    pub identity: IdentityProfile,
    pub user: UserProfile,
}

#[derive(Debug, Clone)]
pub struct BootstrapOutcome {
    pub profile: BootstrapProfile,
    pub greeting: Option<String>,
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
    profile: Option<UserProfile>,
    body: Option<String>,
    raw_content: String,
}

enum OnboardingMode {
    Initial,
    Reset,
}

struct OnboardingDefaults {
    identity_name: String,
    tone: String,
    role: String,
    user_name: Option<String>,
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

pub fn default_paths() -> BootstrapPaths {
    let home_dir = std::env::var("FORJA_HOME_DIR")
        .map(PathBuf::from)
        .ok()
        .or_else(dirs_next::home_dir)
        .unwrap_or_else(|| PathBuf::from("."));
    BootstrapPaths::from_home(home_dir)
}

pub fn ensure_bootstrap(paths: &BootstrapPaths) -> io::Result<BootstrapOutcome> {
    if let Some(profile) = load_existing_profile(paths)? {
        return Ok(BootstrapOutcome {
            profile,
            greeting: None,
        });
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
        "Bootstrap identity and user profile. Apply these rules before any project prompt.\n\n{}",
        sections.join("\n\n")
    ))
}

fn run_onboarding(paths: &BootstrapPaths, mode: OnboardingMode) -> io::Result<BootstrapOutcome> {
    std::fs::create_dir_all(&paths.forja_dir)?;

    let existing_profile = load_existing_profile(paths).ok().flatten();
    let preserved_user_body = preserved_user_body(paths)?;
    let defaults = onboarding_defaults(existing_profile.as_ref(), &mode);

    let identity_name = prompt_with_default("What should I call myself?", &defaults.identity_name)?;
    let user_name = prompt_required("How should I address you?", defaults.user_name.as_deref())?;
    let tone = prompt_with_default(
        "What speaking style should I use? (formal/casual)",
        &defaults.tone,
    )?;
    let role = prompt_with_default("What is my primary role?", &defaults.role)?;

    let profile = BootstrapProfile {
        identity: IdentityProfile {
            name: identity_name,
            role,
            tone,
        },
        user: UserProfile { name: user_name },
    };

    write_identity_file(paths, &profile.identity)?;
    write_user_file(paths, &profile.user, preserved_user_body.as_deref())?;

    if !cfg!(windows)
        && paths.legacy_user_prompt_path != paths.user_path
        && paths.legacy_user_prompt_path.exists()
    {
        let _ = std::fs::remove_file(&paths.legacy_user_prompt_path);
    }

    Ok(BootstrapOutcome {
        greeting: None,
        profile,
    })
}

fn preserved_user_body(paths: &BootstrapPaths) -> io::Result<Option<String>> {
    Ok(load_user_document(paths)?.and_then(|doc| doc.body))
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

    if let Some((frontmatter, body)) = parse_frontmatter_document(&raw_content) {
        let profile = frontmatter.get("name").map(|name| UserProfile {
            name: name.to_string(),
        });
        let body = normalized_optional_body(body);

        return Ok(Some(UserDocument {
            profile,
            body,
            raw_content,
        }));
    }

    let body = normalized_optional_body(raw_content.clone());
    Ok(Some(UserDocument {
        profile: None,
        body,
        raw_content,
    }))
}

fn write_identity_file(paths: &BootstrapPaths, identity: &IdentityProfile) -> io::Result<()> {
    std::fs::write(&paths.identity_path, identity.to_document())
}

fn write_user_file(
    paths: &BootstrapPaths,
    user: &UserProfile,
    preserved_body: Option<&str>,
) -> io::Result<()> {
    std::fs::write(&paths.user_path, user.to_document(preserved_body))
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

fn required_frontmatter_value(values: &HashMap<String, String>, key: &str) -> io::Result<String> {
    values
        .get(key)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("missing required frontmatter key: {key}"),
            )
        })
}

fn parse_frontmatter_document(content: &str) -> Option<(HashMap<String, String>, String)> {
    let normalized = content.replace("\r\n", "\n");
    let stripped = normalized.strip_prefix(DOCUMENT_SEPARATOR)?;
    let (frontmatter, body) = stripped.split_once(FRONTMATTER_END)?;

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

fn normalized_optional_body(body: impl Into<String>) -> Option<String> {
    let body = body.into();
    let trimmed = body.trim().to_string();
    if trimmed.is_empty() {
        return None;
    }

    Some(trimmed)
}

fn sanitize_frontmatter_value(value: &str) -> String {
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_string()
}

fn load_existing_profile(paths: &BootstrapPaths) -> io::Result<Option<BootstrapProfile>> {
    let Some(identity) = read_identity_profile(paths)? else {
        return Ok(None);
    };

    Ok(Some(BootstrapProfile {
        identity,
        user: UserProfile::from_optional_document(load_user_document(paths)?),
    }))
}

fn read_identity_profile(paths: &BootstrapPaths) -> io::Result<Option<IdentityProfile>> {
    if !paths.identity_path.exists() {
        return Ok(None);
    }

    let identity_raw = std::fs::read_to_string(&paths.identity_path)?;
    IdentityProfile::from_document(&identity_raw).map(Some)
}

fn onboarding_defaults(
    existing_profile: Option<&BootstrapProfile>,
    mode: &OnboardingMode,
) -> OnboardingDefaults {
    OnboardingDefaults {
        identity_name: existing_profile
            .map(|profile| profile.identity.name.clone())
            .unwrap_or_else(|| DEFAULT_IDENTITY_NAME.to_string()),
        tone: existing_profile
            .map(|profile| profile.identity.tone.clone())
            .unwrap_or_else(|| DEFAULT_TONE.to_string()),
        role: existing_profile
            .map(|profile| profile.identity.role.clone())
            .unwrap_or_else(|| DEFAULT_ROLE.to_string()),
        user_name: match mode {
            OnboardingMode::Initial => None,
            OnboardingMode::Reset => existing_profile.map(|profile| profile.user.name.clone()),
        },
    }
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
    fn compose_prompt_prefix_includes_identity_and_user_files() {
        let home_dir = unique_temp_dir("bootstrap_prompt");
        let paths = BootstrapPaths::from_home(&home_dir);
        std::fs::create_dir_all(&paths.forja_dir).unwrap();
        std::fs::write(
            &paths.identity_path,
            "---\nname: Forja\nrole: AI assistant\ntone: formal\n---\n",
        )
        .unwrap();
        std::fs::write(&paths.user_path, "---\nname: User\n---\n").unwrap();

        let prompt = compose_system_prompt_prefix(&paths).unwrap();

        assert!(prompt.contains("[identity.md]"));
        assert!(prompt.contains("name: Forja"));
        assert!(prompt.contains("[user.md]"));
        assert!(prompt.contains("name: User"));

        let _ = std::fs::remove_dir_all(&home_dir);
    }

    #[test]
    fn preserved_user_body_survives_frontmatter_round_trip() {
        let home_dir = unique_temp_dir("bootstrap_user_body");
        let paths = BootstrapPaths::from_home(&home_dir);
        std::fs::create_dir_all(&paths.forja_dir).unwrap();
        std::fs::write(
            &paths.user_path,
            "---\nname: User\n---\n\nKeep legacy prompt",
        )
        .unwrap();

        let body = preserved_user_body(&paths).unwrap();

        assert_eq!(body.as_deref(), Some("Keep legacy prompt"));

        let _ = std::fs::remove_dir_all(&home_dir);
    }

    #[test]
    fn ensure_bootstrap_uses_default_user_when_identity_exists_without_user_file() {
        let home_dir = unique_temp_dir("bootstrap_identity_only");
        let paths = BootstrapPaths::from_home(&home_dir);
        std::fs::create_dir_all(&paths.forja_dir).unwrap();
        std::fs::write(
            &paths.identity_path,
            "---\nname: Forja\nrole: AI assistant\ntone: formal\n---\n",
        )
        .unwrap();

        let outcome = ensure_bootstrap(&paths).unwrap();

        assert_eq!(outcome.profile.identity.name, "Forja");
        assert_eq!(outcome.profile.user.name, "User");
        assert!(outcome.greeting.is_none());

        let _ = std::fs::remove_dir_all(&home_dir);
    }
}
