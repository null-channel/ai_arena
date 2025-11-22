use crate::agent::AIAgent;
use crate::agents::{anthropic::AnthropicAgent, ollama::OllamaAgent, openai::OpenAIAgent};
use crate::secrets::SecretsManager;
use clap::ValueEnum;
use rig::prelude::*;
use rig::providers::anthropic::{self, CLAUDE_3_7_SONNET};

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum, Debug, serde::Deserialize)]
pub enum AgentKind {
    OpenAI,
    Anthropic,
    Ollama,
}

#[derive(Clone, Debug, serde::Deserialize, clap::Args)]
pub struct AIAgentConfig {
    pub model: String,
    pub temp: f32,
    pub seed: Option<u64>,
    #[arg(value_enum)]
    pub agent: AgentKind,
    /// Secret profile name to use for API keys (optional, falls back to environment variables)
    #[arg(long)]
    pub secret_profile: Option<String>,
}

pub fn build_agents(configs: Vec<AIAgentConfig>) -> Vec<AIAgent> {
    // Load secrets manager (will be empty if file doesn't exist, falls back to env vars)
    let secrets_manager = SecretsManager::load().unwrap_or_else(|e| {
        eprintln!("Warning: Could not load secrets file: {}. Falling back to environment variables.", e);
        SecretsManager::load_from_path(std::path::Path::new("/dev/null")).unwrap()
    });

    configs
        .into_iter()
        .enumerate()
        .map(|(i, cfg)| {
            let secret_profile = cfg.secret_profile.as_deref();
            match cfg.agent {
                AgentKind::OpenAI => {
                    let name = format!("OpenAI_{}", i + 1);
                    let api_key = secrets_manager
                        .resolve_openai_key(secret_profile)
                        .expect("Failed to resolve OpenAI API key");
                    AIAgent::OpenAI(OpenAIAgent::new(&name, &cfg.model, &api_key).expect("create openai agent"))
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
                    AIAgent::Anthropic(AnthropicAgent::new(&name, agent).expect("create anthropic agent"))
                }
                AgentKind::Ollama => {
                    let name = format!("Ollama_{}", i + 1);
                    let base_url = secrets_manager
                        .resolve_ollama_base_url(secret_profile)
                        .expect("Failed to resolve Ollama base URL");
                    AIAgent::Ollama(
                        OllamaAgent::new(&name, &cfg.model, &base_url, cfg.temp)
                            .expect("create ollama agent"),
                    )
                }
            }
        })
        .collect()
}

