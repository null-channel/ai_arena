use async_openai::{
    config::OpenAIConfig,
    Client,
    types::{
        ChatCompletionRequestMessage, ChatCompletionRequestSystemMessageArgs,
        ChatCompletionRequestUserMessageArgs, CreateChatCompletionRequestArgs, ResponseFormat,
    },
};
use serde_json::{Value, json};

use crate::agent::{AgentError, AgentResult, MoveRequest, MoveResponse};

pub struct OpenAIAgent {
    name: String,
    model: String,
    client: Client<OpenAIConfig>,
}

impl OpenAIAgent {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn new(name: impl Into<String>, model: impl Into<String>, api_key: impl Into<String>) -> Result<Self, AgentError> {
        let api_key = api_key.into();
        // Create config with the API key directly - no environment variable manipulation needed
        let config = OpenAIConfig::new().with_api_key(&api_key);
        let client = Client::with_config(config);
        
        Ok(Self {
            name: name.into(),
            model: model.into(),
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

        let messages: Vec<ChatCompletionRequestMessage> = vec![
            ChatCompletionRequestSystemMessageArgs::default()
                .content(system)
                .build()
                .map_err(|e| AgentError::Internal(format!("build system msg: {}", e)))?
                .into(),
            ChatCompletionRequestUserMessageArgs::default()
                .content(user)
                .build()
                .map_err(|e| AgentError::Internal(format!("build user msg: {}", e)))?
                .into(),
        ];

        let req = CreateChatCompletionRequestArgs::default()
            .model(&self.model)
            .messages(messages)
            .response_format(ResponseFormat::JsonObject)
            .build()
            .map_err(|e| AgentError::Internal(format!("build chat req: {}", e)))?;

        // Use the client that was created with the API key during initialization
        // No environment variable manipulation needed - eliminates race conditions
        let resp = self.client
            .chat()
            .create(req)
            .await
            .map_err(|e| AgentError::Internal(format!("openai: {}", e)))?;

        let content = resp
            .choices
            .get(0)
            .and_then(|c| c.message.content.as_deref())
            .ok_or_else(|| AgentError::InvalidResponse("missing content".into()))?;

        let chosen_move: Value = serde_json::from_str(content)
            .map_err(|e| AgentError::InvalidResponse(format!("non-json: {}", e)))?;

        Ok(MoveResponse {
            chosen_move,
            diagnostics: None,
        })
    }
}
