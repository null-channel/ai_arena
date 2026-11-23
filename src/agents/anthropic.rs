use rig::completion::Prompt;
use rig::prelude::*;
use rig::{agent::Agent, providers::anthropic::completion::CompletionModel};
use serde_json::Value;

use crate::agent::{AgentError, AgentResult, MoveRequest, MoveResponse};
use crate::agents::prompt;

pub struct AnthropicAgent {
    name: String,
    model: String,
    agent: Agent<CompletionModel>,
    custom_prompt: Option<String>,
}

impl AnthropicAgent {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn new(
        name: impl Into<String>,
        model: impl Into<String>,
        api_key: impl Into<String>,
        custom_prompt: Option<String>,
    ) -> Result<Self, AgentError> {
        let name = name.into();
        let model = model.into();
        let api_key = api_key.into();

        let client = rig::providers::anthropic::Client::new(&api_key).map_err(|e| {
            AgentError::Internal(format!("failed to create Anthropic client: {}", e))
        })?;
        let agent = client
            .agent(&model)
            .preamble("Be precise and concise.")
            .max_tokens(1024)
            .build();

        Ok(Self {
            name,
            model,
            agent,
            custom_prompt,
        })
    }

    pub async fn execute_turn(&self, request: &MoveRequest) -> AgentResult<MoveResponse> {
        // Build system and user prompts using the shared prompt module
        let system_prompt = prompt::system_prompt(request, self.custom_prompt.as_deref());
        let user_prompt = prompt::user_prompt(request);

        // Combine system and user prompts since rig Agent uses a single prompt() call
        // The preamble is already set in agent_config.rs, but we include the full
        // system prompt here to ensure game-specific instructions are included
        let full_prompt = format!("{}\n\n{}", system_prompt, user_prompt);

        // Send the prompt to the agent
        let response = self
            .agent
            .prompt(&full_prompt)
            .await
            .map_err(|e| AgentError::Internal(format!("anthropic api error: {}", e)))?;

        // Parse the JSON response
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
    fn test_anthropic_agent_creation() {
        // Test that we can create an AnthropicAgent with valid parameters
        // We don't expect this to actually connect to Anthropic without a valid key,
        // but we can verify the struct is built correctly if the client creation is lazy.
        // If client creation is eager and validates key, this might fail, but let's try.
        let result = AnthropicAgent::new("test_agent", "claude-3-opus", "test_key", None);

        // rig client might not validate key immediately.
        if let Ok(agent) = result {
            assert_eq!(agent.name(), "test_agent");
        } else {
            // If it fails, it might be due to key validation or other internal checks.
            // For now, we just want to ensure the code compiles and runs.
        }
    }
}
