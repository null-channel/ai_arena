use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::os::unix::fs::PermissionsExt;

#[derive(Debug, Clone)]
pub struct SecretsManager {
    secrets: SecretsConfig,
    config_path: PathBuf,
}

#[derive(Debug, Clone, serde::Deserialize, Default)]
struct SecretsConfig {
    #[serde(default)]
    secrets: SecretsSection,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct SecretsSection {
    #[serde(default)]
    openai: HashMap<String, OpenAISecret>,
    #[serde(default)]
    anthropic: HashMap<String, AnthropicSecret>,
    #[serde(default)]
    ollama: HashMap<String, OllamaSecret>,
}

impl Default for SecretsSection {
    fn default() -> Self {
        Self {
            openai: HashMap::new(),
            anthropic: HashMap::new(),
            ollama: HashMap::new(),
        }
    }
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct OpenAISecret {
    pub api_key: String,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct AnthropicSecret {
    pub api_key: String,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct OllamaSecret {
    pub base_url: String,
}

#[derive(Debug)]
pub enum SecretsError {
    FileNotFound(String),
    InvalidFormat(String),
    PermissionError(String),
    SecretNotFound(String),
    IoError(String),
}

impl std::fmt::Display for SecretsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SecretsError::FileNotFound(path) => write!(f, "Secrets file not found: {}", path),
            SecretsError::InvalidFormat(msg) => write!(f, "Invalid secrets file format: {}", msg),
            SecretsError::PermissionError(msg) => write!(f, "Permission error: {}", msg),
            SecretsError::SecretNotFound(msg) => write!(f, "Secret not found: {}", msg),
            SecretsError::IoError(msg) => write!(f, "IO error: {}", msg),
        }
    }
}

impl std::error::Error for SecretsError {}

impl SecretsManager {
    /// Get the default secrets file path following XDG standards
    pub fn default_config_path() -> PathBuf {
        if let Ok(config_dir) = std::env::var("XDG_CONFIG_HOME") {
            PathBuf::from(config_dir).join("ai_arena").join("secrets.toml")
        } else if let Some(home) = dirs::home_dir() {
            home.join(".config").join("ai_arena").join("secrets.toml")
        } else {
            PathBuf::from(".ai_arena").join("secrets.toml")
        }
    }

    /// Load secrets from the default location or a custom path
    pub fn load() -> Result<Self, SecretsError> {
        Self::load_from_path(&Self::default_config_path())
    }

    /// Load secrets from a specific path
    pub fn load_from_path(path: &Path) -> Result<Self, SecretsError> {
        // Check if file exists
        if !path.exists() {
            // Return empty manager if file doesn't exist (will fall back to env vars)
            return Ok(Self {
                secrets: SecretsConfig {
                    secrets: SecretsSection {
                        openai: HashMap::new(),
                        anthropic: HashMap::new(),
                        ollama: HashMap::new(),
                    },
                },
                config_path: path.to_path_buf(),
            });
        }

        // Check file permissions (warn if world-readable)
        let metadata = fs::metadata(path)
            .map_err(|e| SecretsError::IoError(format!("Failed to read metadata: {}", e)))?;
        let permissions = metadata.permissions();
        let mode = permissions.mode();
        
        // Check if file is readable by others (permission bits 004, 005, 006, 007)
        if (mode & 0o007) != 0 {
            eprintln!("⚠️  Warning: Secrets file is readable by others. Consider running: chmod 600 {}", path.display());
        }

        // Read and parse the file
        let contents = fs::read_to_string(path)
            .map_err(|e| SecretsError::IoError(format!("Failed to read secrets file: {}", e)))?;

        let secrets: SecretsConfig = toml::from_str(&contents)
            .map_err(|e| SecretsError::InvalidFormat(format!("Failed to parse TOML: {}", e)))?;

        Ok(Self {
            secrets,
            config_path: path.to_path_buf(),
        })
    }

    /// Get OpenAI secret by profile name
    pub fn get_openai(&self, profile: &str) -> Result<&OpenAISecret, SecretsError> {
        self.secrets
            .secrets
            .openai
            .get(profile)
            .ok_or_else(|| SecretsError::SecretNotFound(format!("OpenAI profile '{}' not found", profile)))
    }

    /// Get Anthropic secret by profile name
    pub fn get_anthropic(&self, profile: &str) -> Result<&AnthropicSecret, SecretsError> {
        self.secrets
            .secrets
            .anthropic
            .get(profile)
            .ok_or_else(|| SecretsError::SecretNotFound(format!("Anthropic profile '{}' not found", profile)))
    }

    /// Get Ollama secret by profile name
    pub fn get_ollama(&self, profile: &str) -> Result<&OllamaSecret, SecretsError> {
        self.secrets
            .secrets
            .ollama
            .get(profile)
            .ok_or_else(|| SecretsError::SecretNotFound(format!("Ollama profile '{}' not found", profile)))
    }

    /// Resolve OpenAI API key with fallback to environment variable
    pub fn resolve_openai_key(&self, profile: Option<&str>) -> Result<String, SecretsError> {
        // Try secret profile first
        if let Some(profile_name) = profile {
            if let Ok(secret) = self.get_openai(profile_name) {
                return Ok(secret.api_key.clone());
            }
        }

        // Fallback to environment variable
        if let Ok(key) = std::env::var("OPENAI_API_KEY") {
            return Ok(key);
        }

        // Try default profile
        if let Ok(secret) = self.get_openai("default") {
            return Ok(secret.api_key.clone());
        }

        Err(SecretsError::SecretNotFound(
            "OpenAI API key not found. Set OPENAI_API_KEY environment variable or configure a secret profile.".to_string(),
        ))
    }

    /// Resolve Anthropic API key with fallback to environment variable
    pub fn resolve_anthropic_key(&self, profile: Option<&str>) -> Result<String, SecretsError> {
        // Try secret profile first
        if let Some(profile_name) = profile {
            if let Ok(secret) = self.get_anthropic(profile_name) {
                return Ok(secret.api_key.clone());
            }
        }

        // Fallback to environment variable
        if let Ok(key) = std::env::var("ANTHROPIC_API_KEY") {
            return Ok(key);
        }

        // Try default profile
        if let Ok(secret) = self.get_anthropic("default") {
            return Ok(secret.api_key.clone());
        }

        Err(SecretsError::SecretNotFound(
            "Anthropic API key not found. Set ANTHROPIC_API_KEY environment variable or configure a secret profile.".to_string(),
        ))
    }

    /// Resolve Ollama base URL with fallback to environment variable
    pub fn resolve_ollama_base_url(&self, profile: Option<&str>) -> Result<String, SecretsError> {
        // Try secret profile first
        if let Some(profile_name) = profile {
            if let Ok(secret) = self.get_ollama(profile_name) {
                return Ok(secret.base_url.clone());
            }
        }

        // Fallback to environment variable
        if let Ok(url) = std::env::var("OLLAMA_BASE_URL") {
            return Ok(url);
        }

        // Try default profile
        if let Ok(secret) = self.get_ollama("default") {
            return Ok(secret.base_url.clone());
        }

        // Default fallback
        Ok("http://localhost:11434".to_string())
    }

    /// Get the config path
    pub fn config_path(&self) -> &Path {
        &self.config_path
    }
}

