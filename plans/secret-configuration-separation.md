# Plan: Separate Secret Configuration from Non-Sensitive Configuration

## Overview
Refactor the AI agent configuration system to separate API keys and other sensitive credentials from non-sensitive configuration (model, temperature, seed). This will improve security, maintainability, and allow sharing configuration files without exposing secrets.

**Current State:**
- API keys are read from environment variables (`OPENAI_API_KEY`, `ANTHROPIC_API_KEY`, `OLLAMA_BASE_URL`)
- Configuration is mixed: non-sensitive config (model, temp, seed) is in CSV/CLI, while secrets are in environment variables
- No way to reference different API keys for different agents or test cases
- Secrets are global via environment variables, making it hard to use multiple keys per provider

**Goal:**
- Create a separate secrets configuration system
- Allow referencing secrets by name/ID in non-sensitive configuration
- Support secure storage and management of API keys
- Maintain backward compatibility with environment variables

## Architecture Design

### 1. Secrets Configuration File

**Location**: `~/.config/ai_arena/secrets.toml` (or `$XDG_CONFIG_HOME/ai_arena/secrets.toml`)

**Format**: TOML file with named secret profiles

```toml
[secrets.openai.default]
api_key = "sk-..."

[secrets.openai.work]
api_key = "sk-..."

[secrets.anthropic.default]
api_key = "sk-ant-..."

[secrets.anthropic.personal]
api_key = "sk-ant-..."

[secrets.ollama.local]
base_url = "http://localhost:11434"

[secrets.ollama.remote]
base_url = "http://192.168.1.100:11434"
```

**Alternative Format**: JSON (if TOML parsing is not desired)
```json
{
  "secrets": {
    "openai": {
      "default": { "api_key": "sk-..." },
      "work": { "api_key": "sk-..." }
    },
    "anthropic": {
      "default": { "api_key": "sk-ant-..." }
    },
    "ollama": {
      "local": { "base_url": "http://localhost:11434" }
    }
  }
}
```

### 2. Updated Configuration Structures

**Non-Sensitive Config** (`AIAgentConfig`):
```rust
#[derive(Clone, Debug, serde::Deserialize, clap::Args)]
pub struct AIAgentConfig {
    pub model: String,
    pub temp: f32,
    pub seed: Option<u64>,
    #[arg(value_enum)]
    pub agent: AgentKind,
    // NEW: Reference to secret profile
    pub secret_profile: Option<String>, // e.g., "default", "work", "personal"
}
```

**CSV Format Update**:
Add optional `agent_one_secret_profile` and `agent_two_secret_profile` columns:
```csv
game_name,agent_one_kind,agent_one_model,agent_one_temp,agent_one_seed,agent_one_secret_profile,agent_two_kind,agent_two_model,agent_two_temp,agent_two_seed,agent_two_secret_profile,repetitions,description
TicTacToe,OpenAI,gpt-4o-mini,0.7,42,default,OpenAI,gpt-4o-mini,0.7,43,work,1,OpenAI vs OpenAI
```

### 3. Secrets Manager Module

**New File**: `src/secrets.rs`

```rust
pub struct SecretsManager {
    secrets: SecretsConfig,
}

#[derive(Debug, Clone)]
pub struct SecretsConfig {
    pub openai: HashMap<String, OpenAISecret>,
    pub anthropic: HashMap<String, AnthropicSecret>,
    pub ollama: HashMap<String, OllamaSecret>,
}

#[derive(Debug, Clone)]
pub struct OpenAISecret {
    pub api_key: String,
}

#[derive(Debug, Clone)]
pub struct AnthropicSecret {
    pub api_key: String,
}

#[derive(Debug, Clone)]
pub struct OllamaSecret {
    pub base_url: String,
}

impl SecretsManager {
    pub fn load() -> Result<Self, SecretsError>;
    pub fn get_openai(&self, profile: &str) -> Result<&OpenAISecret, SecretsError>;
    pub fn get_anthropic(&self, profile: &str) -> Result<&AnthropicSecret, SecretsError>;
    pub fn get_ollama(&self, profile: &str) -> Result<&OllamaSecret, SecretsError>;
}
```

### 4. Secret Resolution Strategy

**Priority Order** (highest to lowest):
1. **Secret profile** specified in config → Look up in secrets file
2. **Environment variable** → Fallback for backward compatibility
3. **Default profile** → Use "default" profile if exists
4. **Error** → Fail with clear message

**Example Resolution Logic**:
```rust
fn resolve_openai_secret(profile: Option<&str>) -> Result<String, Error> {
    if let Some(profile_name) = profile {
        // Try secrets file first
        if let Ok(secret) = secrets_manager.get_openai(profile_name) {
            return Ok(secret.api_key.clone());
        }
    }
    
    // Fallback to environment variable (backward compatibility)
    if let Ok(key) = std::env::var("OPENAI_API_KEY") {
        return Ok(key);
    }
    
    // Try default profile
    if let Ok(secret) = secrets_manager.get_openai("default") {
        return Ok(secret.api_key.clone());
    }
    
    Err(Error::SecretNotFound("OpenAI API key not found"))
}
```

## Security Considerations

### Option A: Plain Text File (Simplest)
- **Pros**: Easy to implement, no dependencies
- **Cons**: Secrets stored in plain text
- **Mitigation**: File permissions (chmod 600), user's responsibility

### Option B: Encrypted File (Recommended)
- **Pros**: Secrets encrypted at rest
- **Cons**: Requires encryption library, key management
- **Implementation**: Use `age` encryption (Rust-native, simple)
  - Encrypt secrets file with user's public key
  - Decrypt on load using private key (from keychain or file)

### Option C: System Keychain Integration
- **Pros**: Uses OS-native secure storage
- **Cons**: Platform-specific code, additional dependencies
- **Implementation**: 
  - Linux: Use `secret-service` or `keyring` crate
  - macOS: Use `keychain-services` crate
  - Windows: Use `winapi` for Credential Manager

### Option D: Hybrid Approach (Recommended)
- Store secrets in encrypted file (`age` encryption)
- Support environment variable fallback
- Optional: System keychain integration for master key

## Implementation Plan

### Phase 1: Basic Secret Configuration
1. ✅ Create plan document
2. ⏳ Create `src/secrets.rs` module
   - Define `SecretsConfig` structures
   - Implement `SecretsManager` with file loading
   - Support TOML or JSON format
3. ⏳ Update `AIAgentConfig` to include `secret_profile` field
4. ⏳ Update `build_agents()` in `agent_config.rs` to use secrets manager
5. ⏳ Update CSV parsing to handle `secret_profile` columns (optional)
6. ⏳ Update CLI arguments to support `--agent-one-secret-profile` etc.
7. ⏳ Implement secret resolution with environment variable fallback
8. ⏳ Add file permissions check (warn if secrets file is world-readable)

### Phase 2: Security Enhancements
9. ⏳ Add encryption support using `age` crate
   - Encrypt secrets file on write
   - Decrypt on read
   - Handle key management
10. ⏳ Add CLI command to manage secrets (`ai_arena secrets add/remove/list`)
11. ⏳ Add validation and error messages for missing secrets

### Phase 3: Documentation & Migration
12. ⏳ Update README with secrets configuration instructions
13. ⏳ Create example secrets file template
14. ⏳ Add migration guide for existing users
15. ⏳ Update CSV examples

## Questions for User

### 1. Secret Storage Format
- **Question**: Which format do you prefer?
  - **A**: Plain text file (simplest, user manages permissions)
  - **B**: Encrypted file using `age` (recommended, secure)
  - **C**: System keychain integration (most secure, platform-specific)
  - **D**: Hybrid (encrypted file + optional keychain)

**Recommendation**: Option B (encrypted with `age`) - good balance of security and simplicity

### 2. Secrets File Location
- **Question**: Where should the secrets file be stored?
  - **A**: `~/.config/ai_arena/secrets.toml` (XDG standard)
  - **B**: `~/.ai_arena/secrets.toml` (simple, home directory)
  - **C**: Project-local `.ai_arena/secrets.toml` (gitignored)
  - **D**: Configurable via environment variable

**Recommendation**: Option A (XDG standard) with fallback to Option B

### 3. File Format
- **Question**: TOML or JSON for secrets file?
  - **A**: TOML (more readable, better for config)
  - **B**: JSON (simpler parsing, more universal)

**Recommendation**: Option A (TOML) - better for configuration files

### 4. Profile Naming
- **Question**: How should profiles be named?
  - **A**: Simple strings: `"default"`, `"work"`, `"personal"`
  - **B**: Hierarchical: `"openai.default"`, `"openai.work"`
  - **C**: Provider-specific: `"openai_default"`, `"anthropic_personal"`

**Recommendation**: Option A (simple strings) - cleaner, easier to reference

### 5. Backward Compatibility
- **Question**: Should we maintain full backward compatibility?
  - **A**: Yes, environment variables always work as fallback
  - **B**: Yes, but warn if no secrets file exists
  - **C**: No, require secrets file (breaking change)

**Recommendation**: Option A (full backward compatibility) - easier migration

### 6. Multiple Keys Per Provider
- **Question**: Do you need multiple API keys per provider?
  - **A**: Yes, multiple profiles (e.g., work vs personal OpenAI keys)
  - **B**: No, one key per provider is enough

**Recommendation**: Option A (support multiple profiles) - more flexible

### 7. Secret Management CLI
- **Question**: Should we add CLI commands for managing secrets?
  - **A**: Yes, `ai_arena secrets add/remove/list` commands
  - **B**: No, users edit file manually

**Recommendation**: Option A (CLI commands) - better UX, can handle encryption

### 8. Ollama Configuration
- **Question**: Should Ollama base_url be in secrets or regular config?
  - **A**: Secrets (if it contains sensitive network info)
  - **B**: Regular config (it's not really a secret)

**Recommendation**: Option B (regular config) - base_url is not sensitive, but allow override via secrets if needed

## Technical Details

### Dependencies
- **TOML parsing**: `toml` crate (already common in Rust projects)
- **Encryption**: `age` crate (if Option B chosen)
- **Keychain**: `keyring` crate (if Option C chosen)

### Error Handling
- Clear error messages when secrets are missing
- Suggestions for how to fix (e.g., "Run `ai_arena secrets add openai default`")
- Validation of secret format (e.g., OpenAI keys start with `sk-`)

### File Permissions
- Check file permissions on load
- Warn if secrets file is world-readable
- Recommend `chmod 600` for secrets file

## Migration Path

1. **Phase 1**: Add secrets system alongside environment variables (non-breaking)
2. **Phase 2**: Users can optionally migrate to secrets file
3. **Phase 3**: Eventually deprecate environment variables (optional, far future)

## Example Usage

### Setting Up Secrets
```bash
# Initialize secrets file
ai_arena secrets init

# Add OpenAI key
ai_arena secrets add openai default --api-key "sk-..."

# Add Anthropic key with custom profile
ai_arena secrets add anthropic work --api-key "sk-ant-..."

# List secrets (shows profile names only, not keys)
ai_arena secrets list
```

### Using in CSV
```csv
game_name,agent_one_kind,agent_one_model,agent_one_secret_profile,agent_two_kind,agent_two_model,agent_two_secret_profile
TicTacToe,OpenAI,gpt-4o-mini,default,OpenAI,gpt-4o-mini,work
```

### Using in CLI
```bash
ai_arena \
  --game-name TicTacToe \
  --agent-one-kind OpenAI \
  --agent-one-model gpt-4o-mini \
  --agent-one-secret-profile default \
  --agent-two-kind OpenAI \
  --agent-two-model gpt-4o-mini \
  --agent-two-secret-profile work
```

## Implementation Steps

1. ✅ Create plan document
2. ⏳ Get user confirmation on questions above
3. ⏳ Create `src/secrets.rs` module with basic structures
4. ⏳ Implement secrets file loading (TOML parsing)
5. ⏳ Update `AIAgentConfig` to include `secret_profile`
6. ⏳ Update `build_agents()` to use secrets manager
7. ⏳ Update CSV parsing for optional secret profile columns
8. ⏳ Update CLI arguments for secret profiles
9. ⏳ Implement secret resolution with environment variable fallback
10. ⏳ Add file permissions validation
11. ⏳ Add encryption support (if chosen)
12. ⏳ Add CLI commands for secret management (if chosen)
13. ⏳ Update documentation
14. ⏳ Test migration from environment variables

## Notes

- **Security First**: Even with plain text option, we should validate file permissions
- **User Experience**: Clear error messages and helpful suggestions
- **Flexibility**: Support multiple profiles per provider for different use cases
- **Backward Compatibility**: Don't break existing workflows using environment variables

