use serde::{Deserialize, Serialize};
use std::{env, io, path::PathBuf};
use tokio::fs;
use uuid::Uuid;

pub(in crate::server) const DEFAULT_URL: &str = "http://127.0.0.1:8787";

#[derive(Deserialize, Serialize)]
pub(in crate::server) struct ChatSettings {
    pub url: String,
}

impl ChatSettings {
    fn path() -> Result<PathBuf, String> {
        dirs::config_dir()
            .map(|p| p.join("ai_arena/turing-chat.toml"))
            .ok_or_else(|| "Cannot locate the Arena configuration directory.".into())
    }

    pub fn new(raw: &str) -> Result<Self, String> {
        let url = reqwest::Url::parse(raw.trim())
            .ok()
            .filter(|u| {
                matches!(u.scheme(), "https" | "http")
                    && u.host().is_some()
                    && u.username().is_empty()
                    && u.password().is_none()
                    && u.query().is_none()
                    && u.fragment().is_none()
            })
            .ok_or(
                "Enter an HTTP or HTTPS service URL without credentials, a query or a fragment.",
            )?;
        Ok(Self {
            url: url.as_str().trim_end_matches('/').to_owned(),
        })
    }

    pub async fn load() -> Result<Self, String> {
        match fs::read_to_string(Self::path()?).await {
            Ok(data) => {
                let settings: Self = toml::from_str(&data)
                    .map_err(|_| "Cannot parse turing-chat.toml. Save the chat settings again.")?;
                Self::new(&settings.url)
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                Self::new(&env::var("ARENA_TWITCH_URL").unwrap_or_else(|_| DEFAULT_URL.into()))
            }
            Err(_) => Err("Cannot read turing-chat.toml. Check its permissions.".into()),
        }
    }

    pub async fn save(&self) -> Result<(), String> {
        let path = Self::path()?;
        let tmp = path.with_extension(format!("{}.tmp", Uuid::new_v4()));
        let data = toml::to_string(self).map_err(|e| e.to_string())?;
        let write = async {
            fs::create_dir_all(path.parent().unwrap()).await?;
            fs::write(&tmp, data).await?;
            fs::rename(&tmp, path).await
        }
        .await;
        if write.is_err() {
            let _ = fs::remove_file(tmp).await;
            return Err(
                "Cannot save the service URL. Check the Arena configuration directory permissions."
                    .into(),
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn normalizes_service_paths_and_rejects_credentials() {
        assert_eq!(
            ChatSettings::new(" https://bridge.example/twitch/ ")
                .unwrap()
                .url,
            "https://bridge.example/twitch"
        );
        for url in [
            "https://user:token@bridge.example",
            "https://bridge.example/?token=x",
            "file:///tmp/settings",
        ] {
            assert!(ChatSettings::new(url).is_err());
        }
    }
}
