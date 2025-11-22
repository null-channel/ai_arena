# Plan: Support Ollama as an AI Agent

## Overview
Add support for Ollama as an AI agent provider in the ai_arena project using the `llm-connector` crate. Ollama allows running large language models locally or over the network, providing an alternative to cloud-based APIs like OpenAI and Anthropic.

**Approach**: Use the `llm-connector` Rust crate which provides a unified interface for various LLM protocols, including Ollama. This simplifies implementation and provides better abstraction than direct HTTP calls.

## Current Architecture
- Agents implement the `AIAgent` trait defined in `src/agent.rs`
- The trait requires:
  - `name()` method returning the agent's name
  - `perform_turn()` async method that takes a `MoveRequest` and returns a `MoveResponse`
- Existing agents: `OpenAIChatAgent` and `AnthropicAgent` in `src/agents/`
- Agent selection is done via `AgentKind` enum in `src/main.rs`
- Agents are configured via `AIAgentConfig` struct with model, temperature, seed, and agent kind

## Implementation Plan

### 1. Add Ollama to AgentKind Enum
**File**: `src/main.rs`
- Add `Ollama` variant to the `AgentKind` enum (line 31-34)
- This enables CLI and config file selection of Ollama agents

### 2. Create OllamaAgent Implementation
**File**: `src/agents/ollama.rs` (new file)
- Create `OllamaAgent` struct with:
  - `name: String`
  - `model: String` (e.g., "llama3", "mistral", etc.)
  - `base_url: String` (configurable, supports both localhost and network URLs)
  - `temperature: f32` (for request configuration)
  - `client: llm_connector::Client` (or appropriate client type from llm-connector)
- Implement `AIAgent` trait:
  - `name()`: return agent name
  - `perform_turn()`: 
    - Use `llm-connector` API to create chat completion request
    - Set system prompt and user message
    - Configure temperature and seed (if provided)
    - Request JSON format response
    - Parse response and extract the move
    - Handle errors appropriately (network errors, connection timeouts, etc.)
- Support network connectivity:
  - `llm-connector` handles URL parsing and validation
  - Supports both HTTP and HTTPS connections
  - Handles network errors and timeouts through the library

### 3. Update Agent Module
**File**: `src/agents/mod.rs`
- Add `pub mod ollama;` to export the new module

### 4. Update Agent Building Logic
**File**: `src/main.rs`
- Update `build_agent()` function (line 82-99) to handle `AgentKind::Ollama`
- Update `build_agents()` function (line 252-274) to handle Ollama agents
- **Base URL Configuration** (priority order):
  1. Per-agent configuration (if added to `AIAgentConfig`)
  2. Environment variable `OLLAMA_BASE_URL` (for global default)
  3. Default fallback: `http://localhost:11434`
- Use the model name from config for the Ollama model
- Support network URLs: `http://192.168.1.100:11434`, `https://ollama.example.com`, etc.

### 5. Dependencies
**File**: `Cargo.toml`
- Add `llm-connector` crate (latest version, ~0.3.11+)
  - Provides unified interface for LLM protocols including Ollama
  - Handles HTTP/HTTPS connections, URL parsing, error handling
  - Supports network connectivity out of the box
- `reqwest` is already included but may not be needed if `llm-connector` handles HTTP internally
- No need for `url` crate as `llm-connector` handles URL parsing

## Technical Details

### Using llm-connector
- **Library**: `llm-connector` crate provides unified interface for LLM protocols
- **Benefits**:
  - Simplified API compared to direct HTTP calls
  - Built-in support for network connectivity
  - Handles URL parsing, validation, and error handling
  - Supports multiple LLM backends (future extensibility)
- **Integration Pattern**:
  - Initialize client with base URL (supports localhost and network URLs)
  - Use client API to create chat completion requests
  - Configure model, temperature, seed, and JSON format
  - Parse responses through the library's response types
- **API Usage** (to be confirmed during implementation):
  ```rust
  // Example pattern (exact API to be verified):
  let client = llm_connector::Client::new(base_url)?;
  let response = client.chat_completion(...)?;
  ```

### Error Handling
- `llm-connector` handles most low-level errors (connection, timeouts, HTTP errors)
- Map `llm-connector` errors to `AgentError` variants:
  - Connection errors → `AgentError::Internal`
  - Invalid model names → `AgentError::InvalidRequest`
  - Malformed JSON responses → `AgentError::InvalidResponse`
  - Network timeouts → `AgentError::Internal` with descriptive message
- Provide helpful error messages including the attempted URL when connection fails

## Questions for User

1. **Ollama Base URL Configuration** (NETWORK SUPPORT REQUIRED):
   - ✅ **Confirmed**: Support network connections (not just localhost)
   - ✅ **Confirmed**: Use `llm-connector` crate for implementation
   - **Priority order**: Per-agent config > Environment variable > Default localhost
   - **Question**: Should we add a `base_url` field to `AIAgentConfig` struct for per-agent configuration?
   - **Question**: Or should we use a global `OLLAMA_BASE_URL` env var that applies to all Ollama agents?
   - **Question**: Should we support different base URLs for different Ollama agents in the same game?

2. **Model Selection**:
   - Should the model name come from the `model` field in `AIAgentConfig`?
   - Do you want a default model if none is specified?

3. **Temperature and Seed**:
   - Should temperature be passed through from `AIAgentConfig.temp`?
   - Should seed be passed through from `AIAgentConfig.seed` (if provided)?

4. **JSON Format**:
   - Should we use `llm-connector`'s JSON format support to ensure JSON responses?
   - Or should we rely on prompt engineering to get JSON responses?

5. **llm-connector Version**:
   - Use latest version from crates.io? (currently ~0.3.11+)
   - Any specific version requirements or features needed?

## Testing Considerations

1. **Unit Tests**: Test OllamaAgent with mocked HTTP responses
2. **Integration Tests**: Test against a running Ollama instance (if available)
3. **Error Cases**: Test behavior when Ollama server is down, invalid model, etc.

## Implementation Steps

1. ✅ Create plan document
2. ✅ User confirmed: Network connectivity support required
3. ✅ User confirmed: Use `llm-connector` crate
4. ⏳ Get user confirmation on remaining questions above
5. ⏳ Research `llm-connector` API documentation to understand exact usage patterns
6. ⏳ Add `llm-connector` crate to `Cargo.toml`
7. ⏳ Add `Ollama` variant to `AgentKind` enum
8. ⏳ Create `src/agents/ollama.rs` with `OllamaAgent` implementation
   - Use `llm-connector` client API
   - Support configurable base_url (network URLs)
   - Implement proper error handling (map llm-connector errors to AgentError)
   - Configure temperature, seed, and JSON format
9. ⏳ Update `src/agents/mod.rs` to include ollama module
10. ⏳ Update `build_agent()` and `build_agents()` functions in `main.rs`
    - Handle base_url configuration (per-agent or env var)
    - Initialize llm-connector client
11. ⏳ Test with local Ollama instance (localhost)
12. ⏳ Test with network Ollama instance (if available)
13. ⏳ Update documentation if needed

## Notes
- **Using llm-connector**: This approach simplifies implementation significantly
  - No need for manual HTTP request building
  - Built-in URL parsing and validation
  - Unified error handling
  - Future-proof for adding other LLM backends
- **Network Support**: Ollama can be accessed over the network, not just localhost
- No API key is required (unlike OpenAI/Anthropic), unless behind an authenticated proxy
- The implementation will use `llm-connector`'s API instead of direct HTTP calls
- `llm-connector` handles HTTP/HTTPS connections, timeouts, and network errors
- URL examples:
  - `http://localhost:11434` (local)
  - `http://192.168.1.100:11434` (local network)
  - `https://ollama.example.com` (remote, HTTPS)
  - `http://ollama.example.com:11434` (remote, custom port)
- **Next Steps**: Review `llm-connector` documentation to understand exact API before implementation

