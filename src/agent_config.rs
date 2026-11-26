use crate::agent::AIAgent;
use crate::agents::{
    anthropic::AnthropicAgent, gemini::GeminiAgent, ollama::OllamaAgent, openai::OpenAIAgent,
};
use crate::secrets::SecretsManager;
use clap::ValueEnum;

#[derive(
    Copy,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    ValueEnum,
    Debug,
    serde::Serialize,
    serde::Deserialize,
)]
pub enum AgentKind {
    OpenAI,
    Anthropic,
    Gemini,
    Ollama,
}

impl AgentKind {
    // Exact model IDs also match their dated snapshots. "*" means the adapter
    // accepts a temperature for custom model names (including local models).
    pub fn temperature_models(self) -> &'static [&'static str] {
        match self {
            Self::OpenAI => &[
                "gpt-4",
                "gpt-4-turbo",
                "gpt-4o",
                "gpt-4o-mini",
                "gpt-4.1",
                "gpt-4.1-mini",
                "gpt-4.1-nano",
                "gpt-5.2",
                "gpt-5.4",
            ],
            Self::Gemini => &[
                "gemini-2.5-pro",
                "gemini-2.5-flash",
                "gemini-2.5-flash-lite",
                "gemini-2.0-flash",
                "gemini-2.0-flash-001",
                "gemini-2.0-flash-lite",
                "gemini-2.0-flash-lite-001",
                "gemini-1.5-pro",
                "gemini-1.5-flash",
            ],
            Self::Ollama => &["*"],
            Self::Anthropic => &[],
        }
    }

    pub fn supports_temperature(self, model: &str) -> bool {
        self.temperature_models().iter().any(|&id| {
            id == "*"
                || model == id
                || model
                    .strip_prefix(id)
                    .and_then(|suffix| suffix.strip_prefix('-'))
                    .is_some_and(|date| {
                        date.len() == 10
                            && date.bytes().enumerate().all(|(i, ch)| {
                                if i == 4 || i == 7 {
                                    ch == b'-'
                                } else {
                                    ch.is_ascii_digit()
                                }
                            })
                    })
        })
    }
}

impl std::str::FromStr for AgentKind {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "OpenAI" => Ok(Self::OpenAI),
            "Anthropic" => Ok(Self::Anthropic),
            "Gemini" => Ok(Self::Gemini),
            "Ollama" => Ok(Self::Ollama),
            _ => Err(format!("Unsupported agent kind: {value}")),
        }
    }
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, clap::Args)]
pub struct AIAgentConfig {
    pub model: String,
    pub temp: f32,
    #[serde(default)]
    pub seed: Option<u64>,
    #[arg(value_enum)]
    pub agent: AgentKind,
    /// Secret profile name to use for API keys (optional, falls back to environment variables)
    #[arg(long)]
    #[serde(default)]
    pub secret_profile: Option<String>,
    #[serde(default)]
    pub api_endpoint: Option<String>,
    /// Optional UTF-8 file appended to the agent's system prompt. The file
    /// contents are never persisted in run history; only this path is stored.
    #[serde(default)]
    pub prompt_file: Option<String>,
}

fn load_prompt_file(path: Option<&str>) -> Option<String> {
    let path = path?.trim();
    if path.is_empty() {
        return None;
    }
    match std::fs::read_to_string(path) {
        Ok(prompt) => Some(prompt),
        Err(error) => {
            eprintln!("Warning: Could not read prompt file {}: {}", path, error);
            None
        }
    }
}

pub fn build_agents(configs: Vec<AIAgentConfig>, secrets: Option<&SecretsManager>) -> Vec<AIAgent> {
    // Use provided secrets manager or load from disk (fallback to env vars)
    let loaded_secrets;
    let secrets_manager = if let Some(s) = secrets {
        s
    } else {
        loaded_secrets = SecretsManager::load().unwrap_or_else(|e| {
            eprintln!(
                "Warning: Could not load secrets file: {}. Falling back to environment variables.",
                e
            );
            SecretsManager::load_from_path(std::path::Path::new("/dev/null")).unwrap()
        });
        &loaded_secrets
    };

    configs
        .into_iter()
        .enumerate()
        .map(|(i, cfg)| build_agent(i, cfg, secrets_manager).expect("create agent"))
        .collect()
}

pub fn build_agent(
    index: usize,
    cfg: AIAgentConfig,
    secrets: &SecretsManager,
) -> Result<AIAgent, String> {
    let name = format!("{:?}_{}", cfg.agent, index + 1);
    let prompt = load_prompt_file(cfg.prompt_file.as_deref());
    if cfg.agent == AgentKind::Ollama {
        let endpoint = cfg.api_endpoint.ok_or("Set the Ollama endpoint.")?;
        return OllamaAgent::new(name, cfg.model, endpoint, cfg.temp, prompt)
            .map(AIAgent::Ollama)
            .map_err(|e| e.to_string());
    }
    let provider = format!("{:?}", cfg.agent).to_ascii_lowercase();
    let profile = cfg.secret_profile.as_deref().unwrap_or("default");
    let env_key = format!("{}_API_KEY", provider.to_ascii_uppercase());
    let key = [
        format!("secrets.{provider}.{profile}.api_key"),
        format!("{provider}.{profile}.api_key"),
        format!("secrets.{provider}.{profile}"),
        format!("{provider}.{profile}"),
        profile.to_owned(),
    ]
    .iter()
    .find_map(|path| secrets.get(path))
    .or_else(|| std::env::var(&env_key).ok())
    .ok_or_else(|| {
        format!("No {provider} key for profile {profile}. Check Arena secrets or {env_key}.")
    })?;
    let temperature = cfg
        .agent
        .supports_temperature(&cfg.model)
        .then_some(cfg.temp);
    match cfg.agent {
        AgentKind::OpenAI => {
            OpenAIAgent::new(name, &cfg.model, key, temperature, prompt).map(AIAgent::OpenAI)
        }
        AgentKind::Anthropic => {
            AnthropicAgent::new(name, &cfg.model, key, prompt).map(AIAgent::Anthropic)
        }
        AgentKind::Gemini => {
            GeminiAgent::new(name, &cfg.model, key, temperature, prompt).map(AIAgent::Gemini)
        }
        AgentKind::Ollama => unreachable!(),
    }
    .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::secrets::{SecretMode, SecretsManager};
    use std::path::PathBuf;

    #[test]
    fn temperature_follows_the_provider_and_model() {
        assert!(!AgentKind::Anthropic.supports_temperature("claude-sonnet-4-6"));
        assert!(AgentKind::OpenAI.supports_temperature("gpt-4o-2024-08-06"));
        assert!(!AgentKind::OpenAI.supports_temperature("gpt-5-mini"));
        assert!(!AgentKind::OpenAI.supports_temperature("gpt-5.4-pro"));
        assert!(!AgentKind::Gemini.supports_temperature("gemini-3.6-flash"));
        assert!(AgentKind::Gemini.supports_temperature("gemini-2.5-flash"));
        assert!(AgentKind::Ollama.supports_temperature("local:custom"));
    }

    #[test]
    fn test_build_agents_with_profile() {
        let toml_str = r#"
            [openai]
            general = "sk-test-key"
        "#;
        let secrets: toml::Value = toml::from_str(toml_str).unwrap();
        let manager = SecretsManager {
            mode: SecretMode::Secret,
            secrets,
            config_path: PathBuf::from("dummy"),
            encryption_key: None,
            salt: None,
        };

        let config = AIAgentConfig {
            model: "gpt-4".to_string(),
            temp: 0.7,
            seed: None,
            agent: AgentKind::OpenAI,
            secret_profile: Some("openai.general".to_string()),
            api_endpoint: None,
            prompt_file: None,
        };

        let agents = build_agents(vec![config], Some(&manager));
        assert_eq!(agents.len(), 1);
        // If we get here, it didn't panic, so lookup succeeded.
    }

    #[test]
    fn reads_prompt_file_without_storing_its_contents_in_config() {
        let file = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(file.path(), "Prefer legal moves.").unwrap();
        assert_eq!(
            load_prompt_file(file.path().to_str()),
            Some("Prefer legal moves.".to_string())
        );
    }
}
