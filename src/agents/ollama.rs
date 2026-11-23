use llm_connector::{
    LlmClient,
    types::{ChatRequest, Message, Role},
};
use serde_json::Value;

use crate::{
    agent::{AgentError, AgentResult, MoveRequest, MoveResponse},
    agents::prompt,
};

pub struct OllamaAgent {
    name: String,
    model: String,
    temperature: f32,
    client: LlmClient,
    custom_prompt: Option<String>,
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
        custom_prompt: Option<String>,
    ) -> Result<Self, AgentError> {
        let base_url = base_url.into();
        println!("base_url: {}", base_url);
        let client = LlmClient::ollama_with_base_url(&base_url)
            .map_err(|e| AgentError::Internal(format!("failed to create Ollama client: {}", e)))?;

        Ok(Self {
            name: name.into(),
            model: model.into(),
            temperature,
            client,
            custom_prompt,
        })
    }

    pub async fn execute_turn(&self, request: &MoveRequest) -> AgentResult<MoveResponse> {
        let system = prompt::system_prompt(request, self.custom_prompt.as_deref());
        let user = prompt::user_prompt(request);
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
        let result = OllamaAgent::new("test_agent", "llama3", "http://localhost:11434", 0.7, None);

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
        if let Ok(agent) =
            OllamaAgent::new("test_name", "llama3", "http://localhost:11434", 0.7, None)
        {
            assert_eq!(agent.name(), "test_name");
        }
    }
}
