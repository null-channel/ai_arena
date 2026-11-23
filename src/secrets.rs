use std::collections::HashMap;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct SecretsManager {
    secrets: SecretsConfig,
}

#[derive(Debug, Clone, serde::Deserialize, Default)]
struct SecretsConfig {
    #[serde(default)]
    secrets: SecretsSection,
}

#[derive(Debug, Clone, serde::Deserialize, Default)]
struct SecretsSection {
    #[serde(default)]
    openai: HashMap<String, OpenAISecret>,
    #[serde(default)]
    anthropic: HashMap<String, AnthropicSecret>,
    #[serde(default)]
    ollama: HashMap<String, OllamaSecret>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct OpenAISecret {
    pub api_key: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum SecretMode {
    Secret,
    Env,
}

#[derive(Debug, Serialize, Deserialize)]
struct EncryptedFile {
    encrypted: bool,
    salt: Option<String>,
    nonce: Option<String>,
    data: String, // Base64 encoded ciphertext or plain TOML content if not encrypted
}

#[derive(Debug)]
pub enum SecretsError {
    InvalidFormat(String),
    SecretNotFound(String),
    IoError(String),
    EncryptionError(String),
    DecryptionError(String),
    SetSecretError(String),
}

impl std::fmt::Display for SecretsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SecretsError::InvalidFormat(msg) => write!(f, "Invalid secrets file format: {}", msg),
            SecretsError::SecretNotFound(msg) => write!(f, "Secret not found: {}", msg),
            SecretsError::IoError(msg) => write!(f, "IO error: {}", msg),
            SecretsError::EncryptionError(msg) => write!(f, "Encryption error: {}", msg),
            SecretsError::DecryptionError(msg) => write!(f, "Decryption error: {}", msg),
            SecretsError::SetSecretError(msg) => write!(f, "Set Secret error: {}", msg),
        }
    }
}

impl std::error::Error for SecretsError {}

impl SecretsManager {
    /// Get the default secrets file path following XDG standards
    pub fn default_config_path() -> PathBuf {
        if let Ok(config_dir) = std::env::var("XDG_CONFIG_HOME") {
            PathBuf::from(config_dir)
                .join("ai_arena")
                .join("secrets.toml")
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
        if !path.exists() {
            return Ok(Self {
                secrets: SecretsConfig {
                    secrets: SecretsSection {
                        openai: HashMap::new(),
                        anthropic: HashMap::new(),
                        ollama: HashMap::new(),
                    },
                },
            });
        }

        // Check file permissions (warn if world-readable)
        let metadata = fs::metadata(path)
            .map_err(|e| SecretsError::IoError(format!("Failed to read metadata: {}", e)))?;
        let permissions = metadata.permissions();
        let mode = permissions.mode();

        // Check if file is readable by others (permission bits 004, 005, 006, 007)
        if (mode & 0o007) != 0 {
            eprintln!(
                "⚠️  Warning: Secrets file is readable by others. Consider running: chmod 600 {}",
                path.display()
            );
        }

        let content = fs::read_to_string(path)
            .map_err(|e| SecretsError::IoError(format!("Failed to read secrets file: {}", e)))?;

        // Try to parse as EncryptedFile structure
        if let Ok(encrypted_file) = toml::from_str::<EncryptedFile>(&content) {
            if encrypted_file.encrypted {
                return Self::decrypt_and_load(path, encrypted_file);
            }
        }

        // Fallback: Assume plain TOML
        let secrets: toml::Value = toml::from_str(&content)
            .map_err(|e| SecretsError::InvalidFormat(format!("Failed to parse TOML: {}", e)))?;

        Ok(Self { secrets })
    }

    /// Get OpenAI secret by profile name
    pub fn get_openai(&self, profile: &str) -> Result<&OpenAISecret, SecretsError> {
        self.secrets.secrets.openai.get(profile).ok_or_else(|| {
            SecretsError::SecretNotFound(format!("OpenAI profile '{}' not found", profile))
        })
    }

    /// Get Anthropic secret by profile name
    pub fn get_anthropic(&self, profile: &str) -> Result<&AnthropicSecret, SecretsError> {
        self.secrets.secrets.anthropic.get(profile).ok_or_else(|| {
            SecretsError::SecretNotFound(format!("Anthropic profile '{}' not found", profile))
        })
    }

    /// Get Ollama secret by profile name
    pub fn get_ollama(&self, profile: &str) -> Result<&OllamaSecret, SecretsError> {
        self.secrets.secrets.ollama.get(profile).ok_or_else(|| {
            SecretsError::SecretNotFound(format!("Ollama profile '{}' not found", profile))
        })
    }

    /// Resolve OpenAI API key with fallback to environment variable
    pub fn resolve_openai_key(&self, profile: Option<&str>) -> Result<String, SecretsError> {
        // Try secret profile first
        if let Some(profile_name) = profile
            && let Ok(secret) = self.get_openai(profile_name)
        {
            return Ok(secret.api_key.clone());
        }
    }

    /// Save secrets to disk (encrypting if a key is present or requested)
    pub fn save(&mut self) -> Result<(), SecretsError> {
        // If we don't have an encryption key, ask if user wants to encrypt
        if self.encryption_key.is_none() {
            // For non-interactive save (e.g. automated), we might skip this.
            // But for CLI tool, we can ask.
            // For now, let's default to plain text if no key is set, unless we force it.
            // Or we can prompt to set a password.
        }

        let toml_string = toml::to_string(&self.secrets)
            .map_err(|e| SecretsError::IoError(format!("Failed to serialize secrets: {}", e)))?;

        let output_content = if let Some(key) = self.encryption_key {
            // Encrypt
            let salt_bytes = self.salt.ok_or_else(|| {
                SecretsError::EncryptionError("Missing salt for encryption".to_string())
            })?;

            let mut nonce_bytes = [0u8; NONCE_LEN];
            OsRng.fill_bytes(&mut nonce_bytes);

            let cipher = Aes256Gcm::new(&key.into());
            let nonce = Nonce::from_slice(&nonce_bytes);
            let ciphertext = cipher
                .encrypt(nonce, toml_string.as_bytes())
                .map_err(|e| SecretsError::EncryptionError(format!("Encryption failed: {}", e)))?;

            let encrypted_file = EncryptedFile {
                encrypted: true,
                salt: Some(general_purpose::STANDARD.encode(salt_bytes)),
                nonce: Some(general_purpose::STANDARD.encode(nonce_bytes)),
                data: general_purpose::STANDARD.encode(ciphertext),
            };

            toml::to_string(&encrypted_file).map_err(|e| {
                SecretsError::IoError(format!("Failed to serialize encrypted file: {}", e))
            })?
        } else {
            // Plain text
            toml_string
        };

        // Ensure directory exists
        if let Some(parent) = self.config_path.parent() {
            fs::create_dir_all(parent).map_err(|e| {
                SecretsError::IoError(format!("Failed to create config directory: {}", e))
            })?;
        }

        // Write to file
        let mut file = fs::File::create(&self.config_path)
            .map_err(|e| SecretsError::IoError(format!("Failed to create secrets file: {}", e)))?;

    /// Resolve Anthropic API key with fallback to environment variable
    pub fn resolve_anthropic_key(&self, profile: Option<&str>) -> Result<String, SecretsError> {
        // Try secret profile first
        if let Some(profile_name) = profile
            && let Ok(secret) = self.get_anthropic(profile_name)
        {
            return Ok(secret.api_key.clone());
        }

        file.write_all(output_content.as_bytes())
            .map_err(|e| SecretsError::IoError(format!("Failed to write secrets: {}", e)))?;

        Ok(())
    }

    /// Enable encryption by setting a password
    pub fn set_password(&mut self, password: &str) {
        let mut salt = [0u8; SALT_LEN];
        OsRng.fill_bytes(&mut salt);

        let mut key = [0u8; KEY_LEN];
        let _ = pbkdf2::<Hmac<Sha256>>(password.as_bytes(), &salt, PBKDF2_ROUNDS, &mut key);
        self.encryption_key = Some(key);
        self.salt = Some(salt);
    }

    /// Resolve Ollama base URL with fallback to environment variable
    pub fn resolve_ollama_base_url(&self, profile: Option<&str>) -> Result<String, SecretsError> {
        // Try secret profile first
        if let Some(profile_name) = profile
            && let Ok(secret) = self.get_ollama(profile_name)
        {
            return Ok(secret.base_url.clone());
        }

        let mut result: Vec<String> = profiles.into_iter().collect();
        result.sort();
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_list_profiles() {
        let toml_str = r#"
            [secrets]
            [secrets.openai]
            default = { api_key = "sk-..." }
            dev = { api_key = "sk-..." }

            [secrets.anthropic]
            prod = { api_key = "sk-ant-..." }
        "#;

        let secrets: toml::Value = toml::from_str(toml_str).unwrap();
        let manager = SecretsManager {
            mode: SecretMode::Secret,
            secrets,
            config_path: PathBuf::from("dummy"),
            encryption_key: None,
            salt: None,
        };

        let profiles = manager.list_profiles();
        assert!(profiles.contains(&"openai.default".to_string()));
        assert!(profiles.contains(&"openai.dev".to_string()));
        assert!(profiles.contains(&"anthropic.prod".to_string()));
        assert_eq!(profiles.len(), 3);
    }

    #[test]
    fn test_list_profiles_empty() {
        let toml_str = "";
        let secrets: toml::Value = toml::from_str(toml_str).unwrap();
        let manager = SecretsManager {
            mode: SecretMode::Secret,
            secrets,
            config_path: PathBuf::from("dummy"),
            encryption_key: None,
            salt: None,
        };

        let profiles = manager.list_profiles();
        assert!(profiles.is_empty());
    }
}
