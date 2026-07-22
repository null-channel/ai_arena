use std::sync::Arc;
use std::time::Duration;

use crate::agent::{AIAgent, AgentError, AgentResult};
use crate::agent_runtime::ManagedAgent;
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

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
pub struct AIAgentConfig {
    pub model: String,
    pub temp: f32,
    pub seed: Option<u64>,
    pub agent: AgentKind,
    /// Secret profile name to use for API keys (optional, falls back to environment variables)
    pub secret_profile: Option<String>,
    #[serde(default)]
    pub runtime: AgentRuntimeConfig,
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize, clap::Args)]
pub struct AgentRuntimeConfig {
    /// Maximum duration of one provider request, including response parsing.
    #[arg(long, default_value_t = 60_000, value_parser = clap::value_parser!(u64).range(1..))]
    #[serde(default = "default_request_timeout_ms")]
    pub request_timeout_ms: u64,
    /// Number of retries after transient provider failures or timeouts.
    #[arg(long, default_value_t = 2)]
    #[serde(default = "default_max_retries")]
    pub max_retries: u32,
    /// Initial retry delay; later delays use exponential backoff.
    #[arg(long, default_value_t = 250)]
    #[serde(default = "default_retry_backoff_ms")]
    pub retry_backoff_ms: u64,
    /// Optional cumulative provider-reported token limit per agent and match.
    #[arg(long, value_parser = clap::value_parser!(u64).range(1..))]
    #[serde(default)]
    pub max_total_tokens: Option<u64>,
    /// Maximum provider requests in flight within one match.
    #[arg(long, default_value_t = 2, value_parser = clap::value_parser!(u32).range(1..))]
    #[serde(default = "default_max_concurrent_requests")]
    pub max_concurrent_requests: u32,
}

const fn default_request_timeout_ms() -> u64 {
    60_000
}

const fn default_max_retries() -> u32 {
    2
}

const fn default_retry_backoff_ms() -> u64 {
    250
}

const fn default_max_concurrent_requests() -> u32 {
    2
}

impl Default for AgentRuntimeConfig {
    fn default() -> Self {
        Self {
            request_timeout_ms: default_request_timeout_ms(),
            max_retries: default_max_retries(),
            retry_backoff_ms: default_retry_backoff_ms(),
            max_total_tokens: None,
            max_concurrent_requests: default_max_concurrent_requests(),
        }
    }
}

impl AgentRuntimeConfig {
    pub(crate) fn validate(&self) -> AgentResult<()> {
        if self.request_timeout_ms == 0 {
            return Err(AgentError::InvalidRequest(
                "request timeout must be greater than zero".into(),
            ));
        }
        if self.max_concurrent_requests == 0 {
            return Err(AgentError::InvalidRequest(
                "max concurrent requests must be greater than zero".into(),
            ));
        }
        if self.max_total_tokens == Some(0) {
            return Err(AgentError::InvalidRequest(
                "max total tokens must be greater than zero".into(),
            ));
        }
        Ok(())
    }
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
        self.runtime.validate()?;
        if self.agent == AgentKind::Anthropic && self.runtime.max_total_tokens.is_some() {
            return Err(AgentError::InvalidRequest(
                "Anthropic token budgets are unavailable through the current connector".into(),
            ));
        }
        Ok(())
    }

    fn display_name(&self) -> String {
        let seed = self
            .seed
            .map_or_else(|| "none".to_owned(), |seed| seed.to_string());
        format!(
            "{:?}:{} [temp={}, seed={seed}]",
            self.agent, self.model, self.temp
        )
    }
}

pub fn build_agents(configs: Vec<AIAgentConfig>) -> AgentResult<Vec<ManagedAgent<AIAgent>>> {
    for config in &configs {
        config.validate()?;
    }
    let concurrency = configs
        .first()
        .map(|config| config.runtime.max_concurrent_requests)
        .unwrap_or(default_max_concurrent_requests());
    if configs
        .iter()
        .any(|config| config.runtime.max_concurrent_requests != concurrency)
    {
        return Err(AgentError::InvalidRequest(
            "all agents in a match must use the same concurrency limit".into(),
        ));
    }
    let concurrency = usize::try_from(concurrency)
        .map_err(|_| AgentError::InvalidRequest("concurrency limit is too large".into()))?;
    let limiter = Arc::new(tokio::sync::Semaphore::new(concurrency));
    let secrets_manager =
        SecretsManager::load().map_err(|e| AgentError::Internal(format!("load secrets: {e}")))?;

    configs
        .into_iter()
        .map(|cfg| {
            let secret_profile = cfg.secret_profile.as_deref();
            let name = cfg.display_name();
            let agent = match cfg.agent {
                AgentKind::OpenAI => {
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
            Ok(ManagedAgent::new(
                agent,
                Duration::from_millis(cfg.runtime.request_timeout_ms),
                cfg.runtime.max_retries,
                Duration::from_millis(cfg.runtime.retry_backoff_ms),
                cfg.runtime.max_total_tokens,
                Arc::clone(&limiter),
            ))
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
            runtime: AgentRuntimeConfig::default(),
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
        assert_eq!(
            config(0.7, "model").display_name(),
            "Ollama:model [temp=0.7, seed=42]"
        );

        let mut invalid_runtime = config(0.7, "model");
        invalid_runtime.runtime.request_timeout_ms = 0;
        assert!(invalid_runtime.validate().is_err());

        let mut unsupported_budget = config(0.7, "model");
        unsupported_budget.agent = AgentKind::Anthropic;
        unsupported_budget.runtime.max_total_tokens = Some(1_000);
        assert!(unsupported_budget.validate().is_err());
    }
}
