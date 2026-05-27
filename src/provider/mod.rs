use crate::error::Result;
use crate::schema::{Message, ToolDefinition};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

pub mod openai;
#[async_trait]
pub trait LlmProvider {
    async fn generate(
        &mut self,
        messages: &[Message],
        available_tools: Option<Vec<ToolDefinition>>,
    ) -> Result<Message>;
}

#[derive(Debug, Serialize)]
pub struct ChatRequest {
    model: String,
    messages: Vec<ChatMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<ToolSpec>>,
    // #[serde(skip_serializing_if = "Option::is_none")]
    // tool_choice: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ChatMessage {
    role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<ToolSpec>>,
    // #[serde(skip_serializing_if = "Option::is_none")]
    // reasoning_content: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ToolSpec {
    #[serde(rename = "type")]
    kind: String,
    function: ToolFunctionSpec,
}

#[derive(Debug, Serialize)]
pub struct ToolFunctionSpec {
    name: String,
    description: String,
    parameters: serde_json::Value,
}

#[derive(Debug, Deserialize)]
pub struct ChatResponse {
    choices: Vec<Choice>,
}

#[derive(Debug, Deserialize)]
pub struct Choice {
    message: ResponseMessage,
}

#[derive(Debug, Deserialize)]
pub struct ResponseMessage {
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    reasoning_content: Option<String>,
    #[serde(default)]
    tool_calls: Option<Vec<LlmToolCall>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LlmToolCall {
    id: String,
    #[serde(rename = "type")]
    kind: String,
    function: FunctionCall,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FunctionCall {
    name: String,
    arguments: String,
}
