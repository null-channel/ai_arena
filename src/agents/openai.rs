use async_openai::{
    Client,
    config::OpenAIConfig,
    types::{
        ChatCompletionRequestMessage, ChatCompletionRequestSystemMessageArgs,
        ChatCompletionRequestUserMessageArgs, CreateChatCompletionRequestArgs, ResponseFormat,
    },
};
use serde_json::{Value, json};

use crate::agent::{AgentError, AgentResult, MoveRequest, MoveResponse, TokenUsage};

pub struct OpenAIAgent {
    name: String,
    model: String,
    temperature: f32,
    seed: Option<i64>,
    client: Client<OpenAIConfig>,
}

impl OpenAIAgent {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn new(
        name: impl Into<String>,
        model: impl Into<String>,
        api_key: impl Into<String>,
        temperature: f32,
        seed: Option<u64>,
    ) -> Result<Self, AgentError> {
        let seed = seed
            .map(i64::try_from)
            .transpose()
            .map_err(|_| AgentError::InvalidRequest("OpenAI seed exceeds i64::MAX".into()))?;
        let api_key = api_key.into();
        // Create config with the API key directly - no environment variable manipulation needed
        let config = OpenAIConfig::new().with_api_key(&api_key);
        let client = Client::with_config(config);

        Ok(Self {
            name: name.into(),
            model: model.into(),
            temperature,
            seed,
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

        let mut request_builder = CreateChatCompletionRequestArgs::default();
        request_builder
            .model(&self.model)
            .messages(messages)
            .temperature(self.temperature)
            .response_format(ResponseFormat::JsonObject);
        if let Some(seed) = self.seed {
            request_builder.seed(seed);
        }
        let req = request_builder
            .build()
            .map_err(|e| AgentError::Internal(format!("build chat req: {}", e)))?;

        // Use the client that was created with the API key during initialization
        // No environment variable manipulation needed - eliminates race conditions
        let resp = self
            .client
            .chat()
            .create(req)
            .await
            .map_err(|e| AgentError::Internal(format!("openai: {}", e)))?;

        let token_usage = resp.usage.as_ref().map(|usage| TokenUsage {
            input_tokens: usage.prompt_tokens as u64,
            output_tokens: usage.completion_tokens as u64,
            total_tokens: usage.total_tokens as u64,
        });
        let content = resp
            .choices
            .first()
            .and_then(|c| c.message.content.as_deref())
            .ok_or_else(|| AgentError::InvalidResponse("missing content".into()))?;

        let chosen_move: Value = serde_json::from_str(content)
            .map_err(|e| AgentError::InvalidResponse(format!("non-json: {}", e)))?;

        Ok(MoveResponse {
            chosen_move,
            diagnostics: None,
            token_usage,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preserves_sampling_configuration() {
        let agent = OpenAIAgent::new("test", "model", "key", 0.4, Some(42)).unwrap();
        assert_eq!(agent.temperature, 0.4);
        assert_eq!(agent.seed, Some(42));
    }

    #[test]
    fn rejects_seed_larger_than_openai_supports() {
        let result = OpenAIAgent::new("test", "model", "key", 0.4, Some(u64::MAX));
        assert!(result.is_err());
    }
}
