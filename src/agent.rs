use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::agents::{anthropic::AnthropicAgent, ollama::OllamaAgent, openai::OpenAIAgent};

#[derive(Debug, Serialize, Deserialize)]
pub struct MoveRequest {
    pub turn_index: u32,
    pub game_id: String,
    pub state: Value,
    pub expected_move_schema: Value,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MoveResponse {
    pub chosen_move: Value,
    pub diagnostics: Option<String>,
}

#[derive(Debug)]
pub enum AgentError {
    InvalidRequest(String),
    InvalidResponse(String),
    Internal(String),
}

impl std::fmt::Display for AgentError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AgentError::InvalidRequest(msg) => write!(f, "invalid request: {}", msg),
            AgentError::InvalidResponse(msg) => write!(f, "invalid response: {}", msg),
            AgentError::Internal(msg) => write!(f, "internal error: {}", msg),
        }
    }
}

impl std::error::Error for AgentError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_error_display() {
        let err1 = AgentError::InvalidRequest("test request".to_string());
        assert_eq!(err1.to_string(), "invalid request: test request");

        let err2 = AgentError::InvalidResponse("test response".to_string());
        assert_eq!(err2.to_string(), "invalid response: test response");

        let err3 = AgentError::Internal("test internal".to_string());
        assert_eq!(err3.to_string(), "internal error: test internal");
    }

    #[test]
    fn test_agent_error_error_trait() {
        let err = AgentError::Internal("test".to_string());
        // Just verify it implements Error trait
        let _: &dyn std::error::Error = &err;
    }
}

pub type AgentResult<T> = Result<T, AgentError>;

pub enum AIAgent {
    OpenAI(OpenAIAgent),
    Anthropic(AnthropicAgent),
    Ollama(OllamaAgent),
}

impl AIAgent {
    pub fn name(&self) -> &str {
        match self {
            AIAgent::OpenAI(agent) => agent.name(),
            AIAgent::Anthropic(agent) => agent.name(),
            AIAgent::Ollama(agent) => agent.name(),
        }
    }

    pub async fn execute_turn(&self, request: &MoveRequest) -> AgentResult<MoveResponse> {
        match self {
            AIAgent::OpenAI(agent) => agent.execute_turn(request).await,
            AIAgent::Anthropic(agent) => agent.execute_turn(request).await,
            AIAgent::Ollama(agent) => agent.execute_turn(request).await,
        }
    }
}
