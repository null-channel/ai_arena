use serde::{Deserialize, Serialize};
use std::{
    env, fs, io,
    path::{Path, PathBuf},
};
use uuid::Uuid;

pub(in crate::server) const PROVIDERS: &[&str] = &[
    "anthropic",
    "deepseek",
    "gemini",
    "groq",
    "mistral",
    "ollama",
    "openai",
    "openrouter",
    "perplexity",
    "together",
    "xai",
];

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Catalog {
    models: Vec<String>,
}

pub(in crate::server) fn path() -> PathBuf {
    env::var_os("ARENA_TOOLBENCH_MODELS")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            dirs::config_dir()
                .unwrap_or_else(|| PathBuf::from(".config"))
                .join("ai_arena/toolbench-models.toml")
        })
}

pub(in crate::server) fn model_id(provider: &str, model: &str) -> Result<String, String> {
    let provider = provider.trim().to_ascii_lowercase();
    if !PROVIDERS.contains(&provider.as_str()) {
        return Err(format!("Unsupported Toolbench provider: {provider}"));
    }
    let model = model.trim();
    if model.is_empty() || model.contains(|c: char| c.is_whitespace() || c.is_control() || c == ',')
    {
        return Err("Enter a model ID without spaces or commas.".into());
    }
    Ok(format!("{provider}:{model}"))
}

fn parse(source: &str) -> io::Result<Vec<String>> {
    let catalog: Catalog = toml::from_str(source).map_err(io::Error::other)?;
    let mut models = Vec::new();
    for entry in catalog.models {
        let (provider, id) = entry
            .split_once(':')
            .ok_or_else(|| io::Error::other("Expected provider:model in models"))?;
        models.push(model_id(provider, id).map_err(io::Error::other)?);
    }
    models.sort();
    models.dedup();
    Ok(models)
}

pub(in crate::server) async fn list(path: &Path) -> io::Result<Vec<String>> {
    match tokio::fs::read_to_string(path).await {
        Ok(text) => parse(&text),
        Err(e) if e.kind() == io::ErrorKind::NotFound => parse(include_str!("models.toml")),
        Err(e) => Err(e),
    }
}

// Serialize read-modify-write across Arena instances sharing the same config.
// Readers see the old or the new file, including while a save is in progress.
pub(in crate::server) async fn add(path: PathBuf, model: String) -> io::Result<Vec<String>> {
    tokio::task::spawn_blocking(move || {
        let parent = path.parent().unwrap_or(Path::new("."));
        fs::create_dir_all(parent)?;
        let lock = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path.with_added_extension("lock"))?;
        lock.lock()?;
        let (mut models, exists) = match fs::read_to_string(&path) {
            Ok(text) => (parse(&text)?, true),
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                (parse(include_str!("models.toml"))?, false)
            }
            Err(e) => return Err(e),
        };
        match models.binary_search(&model) {
            Ok(_) if exists => return Ok(models),
            Ok(_) => (),
            Err(i) => models.insert(i, model),
        }
        let catalog = Catalog { models };
        let data = toml::to_string_pretty(&catalog).map_err(io::Error::other)?;
        let tmp = path.with_added_extension(Uuid::new_v4().to_string());
        fs::write(&tmp, data)
            .and_then(|_| fs::rename(&tmp, &path))
            .inspect_err(|_| {
                let _ = fs::remove_file(&tmp);
            })?;
        Ok(catalog.models)
    })
    .await
    .map_err(io::Error::other)?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn concurrent_additions_survive_reload_and_invalid_files_are_preserved() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config/models.toml");
        let defaults = list(&path).await.unwrap();
        let tagged = model_id("Ollama", "sample:7b").unwrap();
        let routed = model_id("openrouter", "vendor/model").unwrap();
        let (a, b) = tokio::join!(
            add(path.clone(), tagged.clone()),
            add(path.clone(), routed.clone())
        );
        a.unwrap();
        b.unwrap();
        add(path.clone(), tagged.clone()).await.unwrap();
        let saved = list(&path).await.unwrap();
        assert_eq!(saved.len(), defaults.len() + 2);
        assert!(saved.contains(&tagged) && saved.contains(&routed));
        fs::write(&path, "invalid toml").unwrap();
        assert!(add(path.clone(), tagged).await.is_err());
        assert_eq!(fs::read_to_string(path).unwrap(), "invalid toml");
        assert!(model_id("unknown", "case").is_err());
        assert!(model_id("ollama", "a,b").is_err());
    }
}
