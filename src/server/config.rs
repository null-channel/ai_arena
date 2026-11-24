use std::path::PathBuf;

use crate::agent_config::{AIAgentConfig, AgentKind};
use crate::storage::RunStore;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(in crate::server) enum RunMode {
    Single,
    Multi,
    Matrix,
}

impl RunMode {
    pub fn path(self) -> &'static str {
        match self {
            Self::Single => "/",
            Self::Multi => "/multi",
            Self::Matrix => "/matrix",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Single => "Single",
            Self::Multi => "Multi",
            Self::Matrix => "Matrix",
        }
    }
}

/// Server bind and static file options.
#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    pub static_dir: Option<PathBuf>,
    pub db_path: PathBuf,
    pub form_defaults: FormDefaults,
    pub auth_token: Option<String>,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".to_string(),
            port: 3000,
            static_dir: None,
            db_path: RunStore::default_path(),
            form_defaults: FormDefaults::default(),
            auth_token: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct FormDefaults {
    pub game_type: String,
    pub agent_one: AgentFormDefaults,
    pub agent_two: AgentFormDefaults,
}

impl Default for FormDefaults {
    fn default() -> Self {
        Self {
            game_type: "TicTacToe".to_string(),
            agent_one: AgentFormDefaults {
                kind: AgentKind::OpenAI,
                model: "gpt-5.4".to_string(),
                temp: 0.7,
                seed: Some(100),
                secret_profile: Some("openai.general".to_string()),
                api_endpoint: Some("http://192.168.1.104:11434".to_string()),
                prompt_file: None,
            },
            agent_two: AgentFormDefaults {
                kind: AgentKind::Ollama,
                model: "qwen3.6:latest".to_string(),
                temp: 0.7,
                seed: Some(100),
                secret_profile: Some("openai.general".to_string()),
                api_endpoint: Some("http://192.168.1.104:11434".to_string()),
                prompt_file: None,
            },
        }
    }
}

#[derive(Debug, Clone)]
pub struct AgentFormDefaults {
    pub kind: AgentKind,
    pub model: String,
    pub temp: f32,
    pub seed: Option<u64>,
    pub secret_profile: Option<String>,
    pub api_endpoint: Option<String>,
    pub prompt_file: Option<String>,
}

impl AgentFormDefaults {
    pub(in crate::server) fn into_config(self) -> Result<AIAgentConfig, String> {
        let model = self.model.trim().to_string();
        if model.is_empty() {
            return Err("Enter a model name for each player.".into());
        }
        let temperature = if self.kind.supports_temperature(&model) {
            self.temp
        } else {
            0.7
        };
        if !(0.0..=2.0).contains(&temperature) {
            return Err("Temperature must be between 0 and 2.".into());
        }
        Ok(AIAgentConfig {
            model,
            agent: self.kind,
            temp: temperature,
            seed: self.seed,
            secret_profile: self
                .secret_profile
                .filter(|_| self.kind != AgentKind::Ollama),
            api_endpoint: (self.kind == AgentKind::Ollama).then(|| {
                self.api_endpoint
                    .unwrap_or_else(|| "http://localhost:11434".into())
            }),
            prompt_file: self.prompt_file,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::ServerConfig;
    use std::net::SocketAddr;

    #[test]
    fn default_server_config_uses_a_local_address() {
        let config = ServerConfig::default();
        let addr: SocketAddr = format!("{}:{}", config.host, config.port).parse().unwrap();
        assert!(addr.ip().is_loopback());
        assert_eq!(addr.port(), 3000);
    }
}
