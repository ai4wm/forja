use std::io;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

pub const DEFAULT_BASE: &str = super::base::DEFAULT_BASE_PROMPT;
pub const DEFAULT_CODER: &str = super::coder::DEFAULT_CODER_PROMPT;
pub const DEFAULT_WRITER: &str = super::writer::DEFAULT_WRITER_PROMPT;
pub const DEFAULT_ASSISTANT: &str = super::assistant::DEFAULT_ASSISTANT_PROMPT;
pub const DEFAULT_ANALYST: &str = super::analyst::DEFAULT_ANALYST_PROMPT;
pub const DEFAULT_THINK_MIN: &str = super::think::DEFAULT_THINK_MIN;
pub const DEFAULT_THINK_MAX: &str = super::think::DEFAULT_THINK_MAX;
pub const DEFAULT_MEMORY_RULES: &str = "## Mandatory Rules (NEVER violate)\n1. You have a rolling memory system. The records below are real past conversations.\n2. When asked \"do you remember?\", if the information exists below, answer \"Yes, I remember.\"\n3. NEVER use phrases like \"current session\", \"provided in this conversation\", or \"I cannot browse past records.\"\n4. Only say \"I don't have that in my records\" if the information is truly absent below.\n5. Do NOT downplay your memory capabilities. The records below ARE your memory.";

static PROMPT_LOADER: OnceLock<PromptLoader> = OnceLock::new();

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PromptLoader {
    prompts_dir: PathBuf,
}

impl PromptLoader {
    pub fn new(prompts_dir: &Path) -> Self {
        Self {
            prompts_dir: prompts_dir.to_path_buf(),
        }
    }

    pub fn ensure_default_files(&self) -> io::Result<()> {
        std::fs::create_dir_all(&self.prompts_dir)?;
        std::fs::create_dir_all(self.prompts_dir.join("roles"))?;
        std::fs::create_dir_all(self.prompts_dir.join("think"))?;

        for (relative_path, contents) in [
            ("base.md", DEFAULT_BASE),
            ("memory-rules.md", DEFAULT_MEMORY_RULES),
            ("roles/coder.md", DEFAULT_CODER),
            ("roles/writer.md", DEFAULT_WRITER),
            ("roles/assistant.md", DEFAULT_ASSISTANT),
            ("roles/analyst.md", DEFAULT_ANALYST),
            ("think/min.md", DEFAULT_THINK_MIN),
            ("think/max.md", DEFAULT_THINK_MAX),
        ] {
            write_missing_file(&self.prompts_dir.join(relative_path), contents)?;
        }

        Ok(())
    }

    pub fn load_base(&self, assistant_name: &str, user_title: &str) -> String {
        render_base_prompt(
            &self.load_or_default("base.md", DEFAULT_BASE),
            assistant_name,
            user_title,
        )
    }

    pub fn load_role(&self, role: &str) -> String {
        let relative_path = format!("roles/{role}.md");
        self.load_or_default(
            &relative_path,
            match role {
                "coder" => DEFAULT_CODER,
                "writer" => DEFAULT_WRITER,
                "assistant" => DEFAULT_ASSISTANT,
                "analyst" => DEFAULT_ANALYST,
                _ => "",
            },
        )
    }

    pub fn load_think(&self, level: &str) -> String {
        let relative_path = format!("think/{level}.md");
        self.load_or_default(
            &relative_path,
            match level {
                "min" => DEFAULT_THINK_MIN,
                "max" => DEFAULT_THINK_MAX,
                _ => "",
            },
        )
    }

    pub fn load_memory_rules(&self) -> String {
        self.load_or_default("memory-rules.md", DEFAULT_MEMORY_RULES)
    }

    pub fn load_file(&self, relative_path: &str) -> Option<String> {
        std::fs::read_to_string(self.prompts_dir.join(relative_path)).ok()
    }

    fn load_or_default(&self, relative_path: &str, default: &str) -> String {
        self.load_file(relative_path)
            .unwrap_or_else(|| default.to_string())
    }
}

pub fn install_prompt_loader(loader: PromptLoader) -> Result<(), PromptLoader> {
    PROMPT_LOADER.set(loader)
}

pub fn prompt_loader() -> &'static PromptLoader {
    PROMPT_LOADER.get_or_init(|| PromptLoader::new(Path::new(".forja/prompts")))
}

fn render_base_prompt(template: &str, assistant_name: &str, user_title: &str) -> String {
    template
        .replace("{assistant_name}", assistant_name)
        .replace("{user_title}", user_title)
}

fn write_missing_file(path: &Path, contents: &str) -> io::Result<()> {
    if path.exists() {
        return Ok(());
    }

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    std::fs::write(path, contents)
}
