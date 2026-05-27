use crate::{
    error::{AppError, Result},
    provider::{ChatMessage, ChatRequest, ChatResponse, ToolFunctionSpec, ToolSpec},
    schema::{Message, RoleType, ToolCall, ToolDefinition},
};
use async_trait::async_trait;
use reqwest::Client;

pub struct OpenaiProvider {
    base_url: String,
    model: String,
    api_key: String,
    http_client: Client,
}

impl OpenaiProvider {
    pub fn new(base_url: &str, model: &str, api_key: &str) -> Self {
        Self {
            base_url: base_url.into(),
            model: model.into(),
            api_key: api_key.into(),
            http_client: Client::new(),
        }
    }
}

#[async_trait]
impl crate::provider::LlmProvider for OpenaiProvider {
    async fn generate(
        &mut self,
        messages: &[Message],
        available_tools: Option<Vec<ToolDefinition>>,
    ) -> Result<Message> {
        let chat_messages: Vec<ChatMessage> = messages
            .iter()
            .map(|msg| match msg.role {
                RoleType::System => ChatMessage {
                    role: "system".into(),
                    content: Some(msg.content.clone()),
                    tool_call_id: None,
                    tool_calls: None,
                },
                RoleType::User => ChatMessage {
                    role: "user".into(),
                    content: Some(msg.content.clone()),
                    tool_call_id: msg.tool_call_id.clone(),
                    tool_calls: None,
                },
                RoleType::Assistant => {
                    let content = if msg.content.is_empty() {
                        None
                    } else {
                        Some(msg.content.clone())
                    };

                    let tool_calls = msg.tool_calls.as_ref().map(|tools| {
                        tools
                            .iter()
                            .map(|t| ToolSpec {
                                kind: "function".into(),
                                function: ToolFunctionSpec {
                                    name: t.name.clone(),
                                    description: String::new(),
                                    parameters: t.arguments.clone(),
                                },
                            })
                            .collect()
                    });

                    ChatMessage {
                        role: "assistant".into(),
                        content,
                        tool_call_id: msg.tool_call_id.clone(),
                        tool_calls,
                    }
                }
                RoleType::Tool => ChatMessage {
                    role: "tool".into(),
                    content: Some(msg.content.clone()),
                    tool_call_id: msg.tool_call_id.clone(),
                    tool_calls: None,
                },
            })
            .collect();

        let tools: Option<Vec<ToolSpec>> = available_tools.map(|defs| {
            defs.into_iter()
                .map(|t| ToolSpec {
                    kind: "function".into(),
                    function: ToolFunctionSpec {
                        name: t.name,
                        description: t.description,
                        parameters: t.input_schema,
                    },
                })
                .collect()
        });

        let request = ChatRequest {
            model: self.model.clone(),
            messages: chat_messages,
            tools,
        };

        let response = self
            .http_client
            .post(format!("{}/chat/completions", self.base_url))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&request)
            .send()
            .await
            .map_err(|e| AppError::Generic(format!("OpenAI Api request failed: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status();
            return Err(AppError::Generic(format!(
                "OpenAI API request failed: {}",
                status
            )));
        }

        let chat_resp: ChatResponse = response
            .json()
            .await
            .map_err(|e| AppError::Generic(format!("Failed to parse response: {}", e)))?;

        let choice = chat_resp
            .choices
            .into_iter()
            .next()
            .ok_or_else(|| AppError::Generic("No response from API".into()))?;

        let tool_calls: Option<Vec<ToolCall>> = choice.message.tool_calls.map(|calls| {
            calls
                .into_iter()
                .map(|t| ToolCall {
                    id: t.id,
                    name: t.function.name,
                    arguments: serde_json::from_str(&t.function.arguments)
                        .unwrap_or(serde_json::Value::Null),
                })
                .collect()
        });

        Ok(Message {
            role: RoleType::Assistant,
            content: choice.message.content.unwrap_or_default(),
            tool_calls,
            tool_call_id: None,
        })
    }
}
