use rig::{
    agent::Agent,
    providers::anthropic::completion::CompletionModel,
};

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
        let _user_payload = serde_json::json!({
            "turn_index": request.turn_index,
            "game_id": request.game_id,
            "state": request.state,
            "expected_move_schema": request.expected_move_schema,
        })
        .to_string();
        Err(AgentError::Internal(format!(
            "build system msg: {}",
            "DOES NOT WORK"
        )))
    }
}
