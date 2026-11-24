use rig::completion::Prompt;
use rig::prelude::*;
use rig::{agent::Agent, providers::gemini::completion::CompletionModel};
use serde_json::Value;

use crate::agent::{AgentError, AgentResult, MoveRequest, MoveResponse};
use crate::agents::prompt;

pub struct GeminiAgent {
    name: String,
    model: String,
    agent: Agent<CompletionModel>,
    custom_prompt: Option<String>,
}

impl GeminiAgent {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn new(
        name: impl Into<String>,
        model: impl Into<String>,
        api_key: impl Into<String>,
        temperature: Option<f32>,
        custom_prompt: Option<String>,
    ) -> Result<Self, AgentError> {
        let name = name.into();
        let model = model.into();
        let api_key = api_key.into();

        let client = rig::providers::gemini::Client::new(&api_key)
            .map_err(|e| AgentError::Internal(format!("failed to create Gemini client: {}", e)))?;
        let mut builder = client.agent(&model).preamble("Be precise and concise.");
        if let Some(temperature) = temperature {
            builder = builder.additional_params(serde_json::json!({
                "generationConfig": { "temperature": temperature }
            }));
        }
        let agent = builder.build();

        Ok(Self {
            name,
            model,
            agent,
            custom_prompt,
        })
    }

    pub async fn execute_turn(&self, request: &MoveRequest) -> AgentResult<MoveResponse> {
        let system_prompt = prompt::system_prompt(request, self.custom_prompt.as_deref());
        let user_prompt = prompt::user_prompt(request);

        let full_prompt = format!("{}\n\n{}", system_prompt, user_prompt);

        let response = self
            .agent
            .prompt(&full_prompt)
            .await
            .map_err(|e| AgentError::Internal(format!("gemini api error: {}", e)))?;

        let chosen_move: Value = serde_json::from_str(&response).map_err(|e| {
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
    fn test_gemini_agent_creation() {
        let result = GeminiAgent::new("test_agent", "gemini-1.5-pro", "test_key", Some(0.7), None);
        if let Ok(agent) = result {
            assert_eq!(agent.name(), "test_agent");
        }
    }
}
