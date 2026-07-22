use rig::{agent::Agent, completion::Prompt, providers::anthropic::completion::CompletionModel};
use serde_json::Value;

use crate::agent::{AgentError, AgentResult, MoveRequest, MoveResponse};

pub struct AnthropicAgent {
    name: String,
    agent: Agent<CompletionModel>,
}

impl AnthropicAgent {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn new(name: impl Into<String>, agent: Agent<CompletionModel>) -> Result<Self, AgentError> {
        Ok(Self {
            name: name.into(),
            agent,
        })
    }

    pub async fn execute_turn(&self, request: &MoveRequest) -> AgentResult<MoveResponse> {
        let user_payload = serde_json::json!({
            "turn_index": request.turn_index,
            "game_id": request.game_id,
            "state": request.state,
            "expected_move_schema": request.expected_move_schema,
        })
        .to_string();
        let content = self
            .agent
            .prompt(user_payload)
            .await
            .map_err(|e| AgentError::Internal(format!("anthropic: {e}")))?;
        let chosen_move: Value = serde_json::from_str(&content)
            .map_err(|e| AgentError::InvalidResponse(format!("non-json: {e}")))?;

        Ok(MoveResponse {
            chosen_move,
            diagnostics: None,
        })
    }
}
