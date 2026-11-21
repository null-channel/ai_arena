use aes_gcm::{
    Aes256Gcm, Nonce,
    aead::{Aead, KeyInit},
};
use base64::{Engine as _, engine::general_purpose};
use pbkdf2::{hmac::Hmac, pbkdf2};
use rand::{RngCore, rngs::OsRng};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use std::env;
use std::io::{self, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::{fmt::write, fs};

const SALT_LEN: usize = 16;
const NONCE_LEN: usize = 12;
const KEY_LEN: usize = 32;
const PBKDF2_ROUNDS: u32 = 100_000;

#[derive(Debug, Clone)]
pub struct SecretsManager {
    pub mode: SecretMode,
    pub secrets: toml::Value,
    pub config_path: PathBuf,
    pub encryption_key: Option<[u8; KEY_LEN]>,
    pub salt: Option<[u8; SALT_LEN]>,
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
    FileNotFound(String),
    InvalidFormat(String),
    PermissionError(String),
    SecretNotFound(String),
    IoError(String),
    EncryptionError(String),
    DecryptionError(String),
    SetSecretError(String),
}

impl std::fmt::Display for SecretsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SecretsError::FileNotFound(path) => write!(f, "Secrets file not found: {}", path),
            SecretsError::InvalidFormat(msg) => write!(f, "Invalid secrets file format: {}", msg),
            SecretsError::PermissionError(msg) => write!(f, "Permission error: {}", msg),
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
                mode: SecretMode::Env,
                secrets: toml::Value::Table(toml::map::Map::new()),
                config_path: path.to_path_buf(),
                encryption_key: None,
                salt: None,
            });
        }

        // Check permissions
        if let Ok(metadata) = fs::metadata(path) {
            let mode = metadata.permissions().mode();
            if (mode & 0o007) != 0 {
                eprintln!(
                    "⚠️  Warning: Secrets file is readable by others. Consider running: chmod 600 {}",
                    path.display()
                );
            }
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

        Ok(Self {
            mode: SecretMode::Secret,
            secrets,
            config_path: path.to_path_buf(),
            encryption_key: None,
            salt: None,
        })
    }

    fn decrypt_and_load(path: &Path, file: EncryptedFile) -> Result<Self, SecretsError> {
        let salt = general_purpose::STANDARD
            .decode(file.salt.ok_or_else(|| {
                SecretsError::InvalidFormat("Missing salt for encrypted file".to_string())
            })?)
            .map_err(|e| SecretsError::InvalidFormat(format!("Invalid salt base64: {}", e)))?;

        let nonce_bytes = general_purpose::STANDARD
            .decode(file.nonce.ok_or_else(|| {
                SecretsError::InvalidFormat("Missing nonce for encrypted file".to_string())
            })?)
            .map_err(|e| SecretsError::InvalidFormat(format!("Invalid nonce base64: {}", e)))?;

        let ciphertext = general_purpose::STANDARD.decode(&file.data).map_err(|e| {
            SecretsError::InvalidFormat(format!("Invalid ciphertext base64: {}", e))
        })?;

        // Prompt for password
        print!("Enter password for secrets file: ");
        io::stdout().flush().unwrap();
        let password = rpassword::read_password()
            .map_err(|e| SecretsError::IoError(format!("Failed to read password: {}", e)))?;

        // Derive key
        let mut key = [0u8; KEY_LEN];
        let _ = pbkdf2::<Hmac<Sha256>>(password.as_bytes(), &salt, PBKDF2_ROUNDS, &mut key); // Decrypt
        let cipher = Aes256Gcm::new(&key.into());
        let nonce = Nonce::from_slice(&nonce_bytes);
        let plaintext = cipher.decrypt(nonce, ciphertext.as_ref()).map_err(|_| {
            SecretsError::DecryptionError("Invalid password or corrupted data".to_string())
        })?;

        let secrets_str = String::from_utf8(plaintext)
            .map_err(|e| SecretsError::DecryptionError(format!("Invalid UTF-8: {}", e)))?;

        let secrets: toml::Value = toml::from_str(&secrets_str).map_err(|e| {
            SecretsError::InvalidFormat(format!("Failed to parse decrypted TOML: {}", e))
        })?;

        let mut salt_bytes = [0u8; SALT_LEN];
        if salt.len() != SALT_LEN {
            return Err(SecretsError::InvalidFormat(
                "Invalid salt length".to_string(),
            ));
        }
        salt_bytes.copy_from_slice(&salt);

        Ok(Self {
            mode: SecretMode::Secret,
            secrets,
            config_path: path.to_path_buf(),
            encryption_key: Some(key),
            salt: Some(salt_bytes),
        })
    }

    /// Get a secret value by key (supports dot notation, e.g., "openai.default.api_key")
    pub fn get(&self, key: &str) -> Option<String> {
        match self.mode {
            SecretMode::Env => {
                println!("get from env");
                let key_vec: Vec<&str> = key.split('.').collect();
                let key_con = key_vec.join("_");

                match env::var(key_con) {
                    Ok(val) => return Some(val),
                    //TODO: Dont eat errors
                    Err(_) => None,
                }
            }
            SecretMode::Secret => {
                let parts: Vec<&str> = key.split('.').collect();
                let mut current = &self.secrets;

                for part in parts {
                    match current.get(part) {
                        Some(value) => current = value,
                        None => return None,
                    }
                }

                match current {
                    toml::Value::String(s) => Some(s.clone()),
                    _ => None,
                }
            }
        }
    }

    /// Set a secret value (supports dot notation)
    pub fn set(&mut self, key: &str, value: &str) -> Result<(), SecretsError> {
        match self.mode {
            SecretMode::Env => {
                return Err(SecretsError::SetSecretError(
                    "Can not set environment vars, run in secret mode to set".to_string(),
                ));
            }
            SecretMode::Secret => {
                let parts: Vec<&str> = key.split('.').collect();
                let mut current = &mut self.secrets;

                for (i, part) in parts.iter().enumerate() {
                    if i == parts.len() - 1 {
                        // Last part, set the value
                        if let toml::Value::Table(map) = current {
                            map.insert(part.to_string(), toml::Value::String(value.to_string()));
                        } else {
                            // If current is not a table (e.g. was empty), make it one
                            // This logic is a bit simplified; handling deep nesting creation requires more work
                            // For now, assume structure exists or is top-level
                            if let toml::Value::Table(map) = current {
                                map.insert(
                                    part.to_string(),
                                    toml::Value::String(value.to_string()),
                                );
                            }
                        }
                    } else {
                        // Navigate or create table
                        if !current.is_table() {
                            *current = toml::Value::Table(toml::map::Map::new());
                        }

                        if let toml::Value::Table(map) = current {
                            if !map.contains_key(*part) {
                                map.insert(
                                    part.to_string(),
                                    toml::Value::Table(toml::map::Map::new()),
                                );
                            }
                            current = map.get_mut(*part).unwrap();
                        }
                    }
                }
                return Ok(());
            }
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

        // Set permissions to 600
        let mut perms = file.metadata().unwrap().permissions();
        perms.set_mode(0o600);
        file.set_permissions(perms).unwrap();

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

    /// List available secret profiles found in the secrets file
    pub fn list_profiles(&self) -> Vec<String> {
        let mut profiles = std::collections::HashSet::new();

        // Check if there is a "secrets" table, otherwise assume root is the container
        let root = self.secrets.get("secrets").unwrap_or(&self.secrets);

        if let Some(table) = root.as_table() {
            println!(
                "SecretsManager: Found {} top-level keys in secrets",
                table.len()
            );
            for (provider, provider_value) in table {
                if let Some(provider_table) = provider_value.as_table() {
                    println!(
                        "SecretsManager: Found provider '{}' with {} profiles",
                        provider,
                        provider_table.len()
                    );
                    for (profile, _) in provider_table {
                        profiles.insert(format!("{}.{}", provider, profile));
                    }
                } else {
                    println!(
                        "SecretsManager: Key '{}' is not a table, treating as a direct profile",
                        provider
                    );
                    profiles.insert(provider.clone());
                }
            }
        } else {
            println!("SecretsManager: Root is not a table!");
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

    #[test]
    fn test_list_profiles_flat() {
        let toml_str = r#"
            [openai]
            default = { api_key = "sk-..." }

            [anthropic]
            dev = { api_key = "sk-ant-..." }
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
        assert!(profiles.contains(&"anthropic.dev".to_string()));
        assert_eq!(profiles.len(), 2);
    }
}
