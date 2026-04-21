use serde_json::Value;
use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};
use std::io;
use std::path::{Path, PathBuf};

pub(crate) const LOCAL_PROVIDER: &str = "llama_cpp";
const MODEL_EXTENSIONS: &[&str] = &["gguf", "ggml", "bin"];
const DEFAULT_HF_BRANCH: &str = "main";
const LOCAL_PORT_BASE: u16 = 38000;
const LOCAL_PORT_RANGE: u16 = 1000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LocalModel {
    pub(crate) model_id: String,
    pub(crate) display_name: String,
    pub(crate) path: PathBuf,
    pub(crate) aliases: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct HuggingFaceRepo {
    pub(crate) repo_id: String,
    pub(crate) filename: Option<String>,
}

pub(crate) fn forja_home_dir() -> PathBuf {
    std::env::var("FORJA_HOME_DIR")
        .map(PathBuf::from)
        .ok()
        .or_else(dirs_next::home_dir)
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".forja")
}

pub(crate) fn models_dir() -> PathBuf {
    forja_home_dir().join("models")
}

pub(crate) fn ensure_models_dir() -> io::Result<PathBuf> {
    let dir = models_dir();
    fs::create_dir_all(&dir)?;
    Ok(dir)
}

pub(crate) fn discover_local_models() -> io::Result<Vec<LocalModel>> {
    let root = models_dir();
    if !root.exists() {
        return Ok(Vec::new());
    }

    let mut models = Vec::new();
    collect_model_files(&root, &root, &mut models)?;
    models.sort_by(|left, right| left.model_id.cmp(&right.model_id));
    Ok(models)
}

pub(crate) fn has_local_models() -> bool {
    discover_local_models()
        .map(|models| !models.is_empty())
        .unwrap_or(false)
}

pub(crate) fn resolve_local_model(model_id: &str) -> io::Result<Option<LocalModel>> {
    let normalized = model_id.trim().replace('\\', "/").to_lowercase();
    if normalized.is_empty() {
        return Ok(None);
    }

    Ok(discover_local_models()?.into_iter().find(|model| {
        model.model_id.eq_ignore_ascii_case(&normalized)
            || model
                .aliases
                .iter()
                .any(|alias| alias.eq_ignore_ascii_case(&normalized))
            || model
                .path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.eq_ignore_ascii_case(model_id.trim()))
    }))
}

pub(crate) fn llama_cpp_base_url(model_id: &str) -> String {
    if let Ok(base_url) = std::env::var("FORJA_LLAMA_CPP_BASE_URL")
        && !base_url.trim().is_empty()
    {
        return base_url.trim_end_matches('/').to_string();
    }

    let mut hasher = DefaultHasher::new();
    model_id.to_lowercase().hash(&mut hasher);
    let offset = (hasher.finish() % u64::from(LOCAL_PORT_RANGE)) as u16;
    format!("http://127.0.0.1:{}/v1", LOCAL_PORT_BASE + offset)
}

pub(crate) fn parse_hf_repo(input: &str, explicit_filename: Option<&str>) -> Result<HuggingFaceRepo, String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err("Usage: /model fetch <owner/repo> [filename.gguf]".to_string());
    }

    let (repo_id, inline_filename) = if let Some((repo, file)) = trimmed.split_once("::") {
        (repo.trim().to_string(), Some(file.trim().to_string()))
    } else {
        (trimmed.to_string(), None)
    };

    if !repo_id.contains('/') {
        return Err("Hugging Face repo must look like owner/repo".to_string());
    }

    let filename = explicit_filename
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .or(inline_filename);

    Ok(HuggingFaceRepo { repo_id, filename })
}

pub(crate) async fn download_hugging_face_model<F>(
    repo: HuggingFaceRepo,
    mut progress: F,
) -> Result<LocalModel, String>
where
    F: FnMut(u64, Option<u64>),
{
    let client = reqwest::Client::new();
    let filename = match repo.filename {
        Some(filename) => filename,
        None => choose_repo_model_filename(
            &client,
            &repo.repo_id,
        )
        .await?,
    };
    let url = format!(
        "https://huggingface.co/{}/resolve/{}/{}?download=true",
        repo.repo_id,
        DEFAULT_HF_BRANCH,
        filename
    );

    let response = client
        .get(&url)
        .send()
        .await
        .map_err(|error| format!("Model download failed: {error}"))?;
    if !response.status().is_success() {
        return Err(format!(
            "Model download failed with HTTP {}",
            response.status()
        ));
    }

    let total = response.content_length();
    let repo_dir = ensure_models_dir()
        .map_err(|error| format!("Could not create models directory: {error}"))?
        .join(repo.repo_id.replace('/', "--"));
    fs::create_dir_all(&repo_dir)
        .map_err(|error| format!("Could not create repo directory: {error}"))?;
    let target_path = repo_dir.join(&filename);
    if let Some(parent) = target_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("Could not create nested model directory: {error}"))?;
    }

    let mut file = tokio::fs::File::create(&target_path)
        .await
        .map_err(|error| format!("Could not create model file: {error}"))?;
    let mut downloaded = 0u64;
    use tokio::io::AsyncWriteExt;

    let mut response = response;
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|error| format!("Model download failed: {error}"))?
    {
        file.write_all(&chunk)
            .await
            .map_err(|error| format!("Could not write model file: {error}"))?;
        downloaded += chunk.len() as u64;
        progress(downloaded, total);
    }

    file.flush()
        .await
        .map_err(|error| format!("Could not finalize model file: {error}"))?;

    build_local_model(&ensure_models_dir().map_err(|error| error.to_string())?, &target_path)
        .ok_or_else(|| "Downloaded file did not produce a valid local model entry".to_string())
}

async fn choose_repo_model_filename(
    client: &reqwest::Client,
    repo_id: &str,
) -> Result<String, String> {
    let endpoint = format!("https://huggingface.co/api/models/{repo_id}");
    let response = client
        .get(&endpoint)
        .send()
        .await
        .map_err(|error| format!("Could not query Hugging Face repo metadata: {error}"))?;
    if !response.status().is_success() {
        return Err(format!(
            "Could not query Hugging Face repo metadata: HTTP {}",
            response.status()
        ));
    }

    let payload: Value = response
        .json()
        .await
        .map_err(|error| format!("Could not parse Hugging Face repo metadata: {error}"))?;
    select_downloadable_filename(&payload)
        .ok_or_else(|| "No downloadable GGUF/GGML model file was found in the repo".to_string())
}

fn select_downloadable_filename(payload: &Value) -> Option<String> {
    payload
        .get("siblings")?
        .as_array()?
        .iter()
        .filter_map(|entry| entry.get("rfilename").and_then(Value::as_str))
        .find(|filename| has_model_extension(filename))
        .map(|filename| filename.to_string())
}

fn collect_model_files(root: &Path, directory: &Path, models: &mut Vec<LocalModel>) -> io::Result<()> {
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_model_files(root, &path, models)?;
            continue;
        }

        if let Some(model) = build_local_model(root, &path) {
            models.push(model);
        }
    }

    Ok(())
}

fn build_local_model(root: &Path, path: &Path) -> Option<LocalModel> {
    if !has_model_extension(path.to_string_lossy().as_ref()) {
        return None;
    }

    let relative = path.strip_prefix(root).ok()?;
    let model_id = relative
        .to_string_lossy()
        .replace('\\', "/")
        .trim()
        .to_string();
    let file_name = path.file_name()?.to_string_lossy().to_string();
    let file_stem = path.file_stem()?.to_string_lossy().to_string();

    Some(LocalModel {
        model_id: model_id.to_lowercase(),
        display_name: format!("{file_stem} (llama.cpp local)"),
        path: path.to_path_buf(),
        aliases: vec![file_stem.to_lowercase(), file_name.to_lowercase()],
    })
}

fn has_model_extension(value: &str) -> bool {
    let normalized = value.to_lowercase();
    MODEL_EXTENSIONS.iter().any(|extension| normalized.ends_with(extension))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with_temp_home<T>(name: &str, test: T)
    where
        T: FnOnce(PathBuf),
    {
        let _guard = crate::test_support::env_lock().lock().unwrap();
        let temp_dir = std::env::temp_dir().join(format!(
            "forja_phase20_{}_{}",
            name,
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        let original = std::env::var("FORJA_HOME_DIR").ok();
        unsafe {
            std::env::set_var("FORJA_HOME_DIR", &temp_dir);
        }
        test(temp_dir.clone());
        if let Some(original) = original {
            unsafe { std::env::set_var("FORJA_HOME_DIR", original) };
        } else {
            unsafe { std::env::remove_var("FORJA_HOME_DIR") };
        }
        let _ = fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn discover_local_models_scans_nested_gguf_files() {
        with_temp_home("discover", |home_dir| {
            let repo_dir = home_dir.join(".forja").join("models").join("owner--repo");
            fs::create_dir_all(&repo_dir).unwrap();
            fs::write(repo_dir.join("tiny.gguf"), b"model").unwrap();

            let models = discover_local_models().unwrap();

            assert_eq!(models.len(), 1);
            assert_eq!(models[0].model_id, "owner--repo/tiny.gguf");
            assert_eq!(models[0].aliases[0], "tiny");
        });
    }

    #[test]
    fn parse_hf_repo_accepts_inline_filename() {
        let spec = parse_hf_repo("owner/repo::model.gguf", None).unwrap();

        assert_eq!(spec.repo_id, "owner/repo");
        assert_eq!(spec.filename.as_deref(), Some("model.gguf"));
    }

    #[test]
    fn select_downloadable_filename_prefers_model_files() {
        let payload = serde_json::json!({
            "siblings": [
                { "rfilename": "README.md" },
                { "rfilename": "quantized/model.gguf" }
            ]
        });

        let filename = select_downloadable_filename(&payload);

        assert_eq!(filename.as_deref(), Some("quantized/model.gguf"));
    }
}
