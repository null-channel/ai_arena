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

pub fn build_agents(configs: Vec<AIAgentConfig>) -> Vec<AIAgent> {
    // Load secrets manager (will be empty if file doesn't exist, falls back to env vars)
    let secrets_manager = SecretsManager::load().unwrap_or_else(|e| {
        eprintln!(
            "Warning: Could not load secrets file: {}. Falling back to environment variables.",
            e
        );
        SecretsManager::load_from_path(std::path::Path::new("/dev/null")).unwrap()
    });

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
                    let api_key = secrets_manager
                        .resolve_openai_key(secret_profile)
                        .expect("Failed to resolve OpenAI API key");
                    AIAgent::OpenAI(
                        OpenAIAgent::new(&name, &cfg.model, &api_key).expect("create openai agent"),
                    )
                }
                AgentKind::Anthropic => {
                    let name = format!("Anthropic_{}", i + 1);
                    let key = secrets_manager
                        .resolve_anthropic_key(secret_profile)
                        .expect("Failed to resolve Anthropic API key");
                    let mdl = anthropic::Client::new(key.as_str());
                    let agent = mdl
                        .agent(CLAUDE_3_7_SONNET)
                        .preamble("Be precise and concise.")
                        .temperature(cfg.temp as f64)
                        .build();
                    AIAgent::Anthropic(
                        AnthropicAgent::new(&name, agent).expect("create anthropic agent"),
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
