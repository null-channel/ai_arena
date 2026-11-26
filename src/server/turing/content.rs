use super::Answer;
use serde::Deserialize;
use std::{
    io,
    path::{Path, PathBuf},
};

#[derive(Clone)]
pub(in crate::server) struct Sample {
    pub id: String,
    pub title: String,
    pub answer: Answer,
    pub path: PathBuf,
    pub mime: &'static str,
}

#[derive(Deserialize)]
struct Metadata {
    id: String,
    title: String,
    true_source: Answer,
    #[serde(default)]
    display_file: String,
    #[serde(default)]
    transcript_file: String,
}

pub(super) fn library(root: &Path) -> io::Result<Vec<Sample>> {
    let root = root.canonicalize()?;
    let mut samples = Vec::new();
    for category in ["text", "art_visual", "voice", "music"] {
        let directory = root.join(category);
        if !directory.is_dir() {
            continue;
        }
        for entry in std::fs::read_dir(directory)? {
            let dir = entry?.path().canonicalize()?;
            let metadata = dir.join("metadata.json");
            if !metadata.is_file() {
                continue;
            }
            if !dir.starts_with(&root) || !metadata.canonicalize()?.starts_with(&dir) {
                return Err(io::Error::other(
                    "Artifact must be inside the content library",
                ));
            }
            let item: Metadata = serde_json::from_slice(&std::fs::read(metadata)?)?;
            let file = if item.display_file.is_empty() {
                &item.transcript_file
            } else {
                &item.display_file
            };
            if file.is_empty() {
                continue;
            }
            let path = dir.join(file).canonicalize()?;
            if !path.starts_with(&dir) || !path.is_file() {
                return Err(io::Error::other("Invalid artifact file"));
            }
            let mime = match path
                .extension()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_ascii_lowercase()
                .as_str()
            {
                "txt" | "md" => "text/plain",
                "png" => "image/png",
                "jpg" | "jpeg" => "image/jpeg",
                "webp" => "image/webp",
                "gif" => "image/gif",
                "svg" => "image/svg+xml",
                "mp3" => "audio/mpeg",
                "wav" => "audio/wav",
                "ogg" => "audio/ogg",
                "m4a" => "audio/mp4",
                "flac" => "audio/flac",
                _ => continue,
            };
            samples.push(Sample {
                id: item.id,
                title: item.title,
                answer: item.true_source,
                path,
                mime,
            });
        }
    }
    samples.sort_by(|a, b| a.id.cmp(&b.id));
    if samples.windows(2).any(|p| p[0].id == p[1].id) {
        return Err(io::Error::other("Artifact IDs must be unique"));
    }
    Ok(samples)
}
