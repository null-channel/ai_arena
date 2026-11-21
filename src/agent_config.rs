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
        .map(|(i, cfg)| {
            let secret_profile = cfg.secret_profile.as_deref().unwrap_or("default");
            let custom_prompt = load_prompt_file(cfg.prompt_file.as_deref());
            println!("AgentConfig: Building agent {} with secret_profile: '{}'", i, secret_profile);
            match cfg.agent {
                AgentKind::OpenAI => {
                    let name = format!("OpenAI_{}", i + 1);
                    // Try secrets first:
                    // 1. secrets.openai.<profile>.api_key
                    // 2. openai.<profile>.api_key
                    // 3. secrets.openai.<profile> (if value is string)
                    // 4. openai.<profile> (if value is string)
                    // 5. <profile> (if value is string, legacy support for top-level keys)
                    let key_path_1 = format!("secrets.openai.{}.api_key", secret_profile);
                    let key_path_2 = format!("openai.{}.api_key", secret_profile);
                    let key_path_3 = format!("secrets.openai.{}", secret_profile);
                    let key_path_4 = format!("openai.{}", secret_profile);
                    println!("secret profile: {}", secret_profile);
                    let api_key = secrets_manager.get(&key_path_1)
                        .or_else(|| secrets_manager.get(&key_path_2))
                        .or_else(|| secrets_manager.get(&key_path_3))
                        .or_else(|| secrets_manager.get(&key_path_4))
                        .or_else(|| secrets_manager.get(secret_profile))
                        .or_else(|| std::env::var("OPENAI_API_KEY").ok())
                        .expect("Failed to resolve OpenAI API key (check secrets or OPENAI_API_KEY env var)");

                    AIAgent::OpenAI(OpenAIAgent::new(&name, &cfg.model, &api_key, custom_prompt).expect("create openai agent"))
                }
                AgentKind::Anthropic => {
                    let name = format!("Anthropic_{}", i + 1);
                    let key_path_1 = format!("secrets.anthropic.{}.api_key", secret_profile);
                    let key_path_2 = format!("anthropic.{}.api_key", secret_profile);
                    let key_path_3 = format!("secrets.anthropic.{}", secret_profile);
                    let key_path_4 = format!("anthropic.{}", secret_profile);

                    let key = secrets_manager.get(&key_path_1)
                        .or_else(|| secrets_manager.get(&key_path_2))
                        .or_else(|| secrets_manager.get(&key_path_3))
                        .or_else(|| secrets_manager.get(&key_path_4))
                        .or_else(|| secrets_manager.get(secret_profile))
                        .or_else(|| std::env::var("ANTHROPIC_API_KEY").ok())
                        .expect("Failed to resolve Anthropic API key (check secrets or ANTHROPIC_API_KEY env var)");

                    AIAgent::Anthropic(
                        AnthropicAgent::new(&name, &cfg.model, &key, cfg.temp, custom_prompt)
                            .expect("create anthropic agent"),
                    )
                }
                AgentKind::Gemini => {
                    let name = format!("Gemini_{}", i + 1);
                    let key_path_1 = format!("secrets.gemini.{}.api_key", secret_profile);
                    let key_path_2 = format!("gemini.{}.api_key", secret_profile);
                    let key_path_3 = format!("secrets.gemini.{}", secret_profile);
                    let key_path_4 = format!("gemini.{}", secret_profile);

                    let key = secrets_manager.get(&key_path_1)
                        .or_else(|| secrets_manager.get(&key_path_2))
                        .or_else(|| secrets_manager.get(&key_path_3))
                        .or_else(|| secrets_manager.get(&key_path_4))
                        .or_else(|| secrets_manager.get(secret_profile))
                        .or_else(|| std::env::var("GEMINI_API_KEY").ok())
                        .expect("Failed to resolve Gemini API key (check secrets or GEMINI_API_KEY env var)");

                    AIAgent::Gemini(
                        GeminiAgent::new(&name, &cfg.model, &key, cfg.temp, custom_prompt)
                            .expect("create gemini agent"),
                    )
                }
                AgentKind::Ollama => {
                    let name = format!("Ollama_{}", i + 1);
                    let base_url = cfg.api_endpoint.expect("Expected an API endpoint.");

                    AIAgent::Ollama(
                        OllamaAgent::new(&name, &cfg.model, base_url, cfg.temp, custom_prompt)
                            .expect("create ollama agent"),
                    )
                }
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::secrets::{SecretMode, SecretsManager};
    use std::path::PathBuf;

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
