use crate::agent::{AIAgent, AgentError, AgentResult};
use crate::agents::{anthropic::AnthropicAgent, ollama::OllamaAgent, openai::OpenAIAgent};
use crate::secrets::SecretsManager;
use clap::ValueEnum;
use rig::prelude::*;
use rig::providers::anthropic;

#[derive(
    Copy,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    ValueEnum,
    Debug,
    serde::Deserialize,
    serde::Serialize,
)]
pub enum AgentKind {
    #[value(name = "OpenAI", alias = "open-ai", alias = "openai")]
    OpenAI,
    #[value(name = "Anthropic", alias = "anthropic")]
    Anthropic,
    #[value(name = "Ollama", alias = "ollama")]
    Ollama,
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize, clap::Args)]
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

impl AIAgentConfig {
    fn validate(&self) -> AgentResult<()> {
        if self.model.trim().is_empty() {
            return Err(AgentError::InvalidRequest("model cannot be empty".into()));
        }
        if !self.temp.is_finite() || !(0.0..=2.0).contains(&self.temp) {
            return Err(AgentError::InvalidRequest(format!(
                "temperature must be finite and between 0.0 and 2.0, got {}",
                self.temp
            )));
        }
        Ok(())
    }
}

pub fn build_agents(configs: Vec<AIAgentConfig>) -> AgentResult<Vec<AIAgent>> {
    let secrets_manager =
        SecretsManager::load().map_err(|e| AgentError::Internal(format!("load secrets: {e}")))?;

    configs
        .into_iter()
        .enumerate()
        .map(|(i, cfg)| {
            cfg.validate()?;
            let secret_profile = cfg.secret_profile.as_deref();
            let agent = match cfg.agent {
                AgentKind::OpenAI => {
                    let name = format!("OpenAI_{}", i + 1);
                    let api_key = secrets_manager
                        .resolve_openai_key(secret_profile)
                        .map_err(|e| AgentError::InvalidRequest(e.to_string()))?;
                    AIAgent::OpenAI(OpenAIAgent::new(
                        &name,
                        &cfg.model,
                        &api_key,
                        cfg.temp,
                        cfg.seed,
                    )?)
                }
                AgentKind::Anthropic => {
                    let name = format!("Anthropic_{}", i + 1);
                    let key = secrets_manager
                        .resolve_anthropic_key(secret_profile)
                        .map_err(|e| AgentError::InvalidRequest(e.to_string()))?;
                    let mdl = anthropic::Client::new(key.as_str());
                    let agent = mdl
                        .agent(&cfg.model)
                        .preamble("You are a game-playing AI. Respond only with strict JSON matching the expected schema. Do not include markdown or text outside the JSON object.")
                        .temperature(cfg.temp as f64)
                        .build();
                    AIAgent::Anthropic(AnthropicAgent::new(&name, agent)?)
                }
                AgentKind::Ollama => {
                    let name = format!("Ollama_{}", i + 1);
                    let base_url = secrets_manager
                        .resolve_ollama_base_url(secret_profile)
                        .map_err(|e| AgentError::InvalidRequest(e.to_string()))?;
                    AIAgent::Ollama(OllamaAgent::new(
                        &name,
                        &cfg.model,
                        &base_url,
                        cfg.temp,
                        cfg.seed,
                    )?)
                }
            };
            Ok(agent)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(temp: f32, model: &str) -> AIAgentConfig {
        AIAgentConfig {
            model: model.to_owned(),
            temp,
            seed: Some(42),
            agent: AgentKind::Ollama,
            secret_profile: None,
        }
    }

    #[test]
    fn validates_agent_configuration() {
        assert!(config(0.0, "model").validate().is_ok());
        assert!(config(2.0, "model").validate().is_ok());
        assert!(config(f32::NAN, "model").validate().is_err());
        assert!(config(-0.1, "model").validate().is_err());
        assert!(config(2.1, "model").validate().is_err());
        assert!(config(0.7, "  ").validate().is_err());
    }
}
