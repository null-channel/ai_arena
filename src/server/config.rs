use std::path::PathBuf;

use crate::agent_config::AgentKind;
use crate::storage::RunStore;

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
