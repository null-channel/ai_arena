use async_openai::{
    Client,
    config::OpenAIConfig,
    types::{
        ChatCompletionRequestMessage, ChatCompletionRequestSystemMessageArgs,
        ChatCompletionRequestUserMessageArgs, CreateChatCompletionRequestArgs, ResponseFormat,
    },
};
use serde_json::Value;

use crate::agent::{AgentError, AgentResult, MoveRequest, MoveResponse};
use crate::agents::prompt;

pub struct OpenAIAgent {
    name: String,
    model: String,
    client: Client<OpenAIConfig>,
    temperature: Option<f32>,
    custom_prompt: Option<String>,
}

impl OpenAIAgent {
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
        let api_key = api_key.into();
        // Create config with the API key directly - no environment variable manipulation needed
        let config = OpenAIConfig::new().with_api_key(&api_key);
        let client = Client::with_config(config);

        Ok(Self {
            name: name.into(),
            model: model.into(),
            client,
            temperature,
            custom_prompt,
        })
    }

    pub async fn execute_turn(&self, request: &MoveRequest) -> AgentResult<MoveResponse> {
        let system = prompt::system_prompt(request, self.custom_prompt.as_deref());
        let user = prompt::user_prompt(request);

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

        let mut builder = CreateChatCompletionRequestArgs::default();
        if let Some(temperature) = self.temperature {
            builder.temperature(temperature);
        }
        let req = builder
            .model(&self.model)
            .messages(messages)
            .response_format(ResponseFormat::JsonObject)
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
