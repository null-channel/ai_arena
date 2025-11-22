use llm_connector::{
    LlmClient,
    types::{ChatRequest, Message, Role},
};
use serde_json::{Value, json};

use crate::agent::{AgentError, AgentResult, MoveRequest, MoveResponse};

pub struct OllamaAgent {
    name: String,
    model: String,
    base_url: String,
    temperature: f32,
    client: LlmClient,
}

impl OllamaAgent {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn new(
        name: impl Into<String>,
        model: impl Into<String>,
        base_url: impl Into<String>,
        temperature: f32,
    ) -> Result<Self, AgentError> {
        let base_url = base_url.into();
        let client = LlmClient::ollama_with_base_url(&base_url)
            .map_err(|e| AgentError::Internal(format!("failed to create Ollama client: {}", e)))?;

        Ok(Self {
            name: name.into(),
            model: model.into(),
            base_url,
            temperature,
            client,
        })
    }

    pub async fn execute_turn(&self, request: &MoveRequest) -> AgentResult<MoveResponse> {
        let system = "You are a game-playing AI. Respond ONLY with strict JSON matching the expected schema. Do not include any text outside JSON.";
        let user = json!({
            "turn_index": request.turn_index,
            "game_id": request.game_id,
            "state": request.state,
            "expected_move_schema": request.expected_move_schema,
        })
        .to_string();

        let messages = vec![
            Message::text(Role::System, system),
            Message::text(Role::User, &user),
        ];

        let chat_request = ChatRequest {
            model: self.model.clone(),
            messages,
            temperature: Some(self.temperature),
            ..Default::default()
        };

        let response = self
            .client
            .chat(&chat_request)
            .await
            .map_err(|e| AgentError::Internal(format!("ollama chat request failed: {}", e)))?;

        // Extract content from response - llm-connector returns content as a String
        let content = response.content;

        // Parse the JSON response
        let chosen_move: Value = serde_json::from_str(&content).map_err(|e| {
            AgentError::InvalidResponse(format!("failed to parse JSON response: {}", e))
        })?;

        Ok(MoveResponse {
            chosen_move,
            diagnostics: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ollama_agent_creation() {
        // Test that we can create an OllamaAgent with valid parameters
        // Note: This will fail if Ollama server is not running, but tests the API
        let result = OllamaAgent::new("test_agent", "llama3", "http://localhost:11434", 0.7);

        // If Ollama is running, this should succeed
        // If not, we at least verify the API is correct
        match result {
            Ok(agent) => {
                assert_eq!(agent.name(), "test_agent");
            }
            Err(e) => {
                // If connection fails, that's expected if Ollama isn't running
                // But we verify the error is handled properly
                assert!(
                    e.to_string().contains("failed to create Ollama client")
                        || e.to_string().contains("ollama")
                );
            }
        }
    }

    #[test]
    fn test_ollama_agent_name() {
        // Test name method (doesn't require Ollama to be running)
        if let Ok(agent) = OllamaAgent::new("test_name", "llama3", "http://localhost:11434", 0.7) {
            assert_eq!(agent.name(), "test_name");
        }
    }
}
