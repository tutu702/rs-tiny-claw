use crate::{
    error::{AppError, Result},
    provider::{
        ChatMessage, ChatRequest, ChatResponse, FunctionCall, LlmToolCall, ToolFunctionSpec,
        ToolSpec,
    },
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
            .filter_map(|msg| match msg.role {
                RoleType::System => Some(ChatMessage {
                    role: "system".into(),
                    content: Some(msg.content.clone()),
                    tool_call_id: None,
                    tool_calls: None,
                }),
                RoleType::User => Some(ChatMessage {
                    role: "user".into(),
                    content: Some(msg.content.clone()),
                    // user 角色永远不应该带 tool_call_id；强制清空防止上游误填再次触发 400
                    tool_call_id: None,
                    tool_calls: None,
                }),
                RoleType::Assistant => {
                    let content = if msg.content.is_empty() {
                        None
                    } else {
                        Some(msg.content.clone())
                    };

                    let tool_calls = msg.tool_calls.as_ref().map(|tools| {
                        tools
                            .iter()
                            .map(|t| LlmToolCall {
                                // 必须把 id 一并回传给 LLM，否则 tool 响应中的 tool_call_id 找不到对应项
                                id: t.id.clone(),
                                kind: "function".into(),
                                function: FunctionCall {
                                    name: t.name.clone(),
                                    // 关键：assistant 工具调用的 arguments 必须是 JSON 字符串
                                    arguments: serde_json::to_string(&t.arguments)
                                        .unwrap_or_else(|_| "null".into()),
                                },
                            })
                            .collect()
                    });

                    // 跳过空的 assistant 消息（tool_calls 被 take 后残留的）
                    if content.is_none() && tool_calls.is_none() {
                        return None;
                    }

                    Some(ChatMessage {
                        role: "assistant".into(),
                        content,
                        tool_call_id: msg.tool_call_id.clone(),
                        tool_calls,
                    })
                }
                RoleType::Tool => Some(ChatMessage {
                    role: "tool".into(),
                    content: Some(msg.content.clone()),
                    tool_call_id: msg.tool_call_id.clone(),
                    tool_calls: None,
                }),
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

        // 调试日志：打印请求 JSON
        // let request_json = serde_json::to_string_pretty(&request)
        //     .unwrap_or_else(|_| "Failed to serialize request".into());
        // println!("\n[DEBUG] === LLM Request ===");
        // println!("{}\n", request_json);

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
            let body = response.text().await.unwrap_or_default();
            // println!("[DEBUG] === LLM Response Error ===");
            // println!("Status: {}\nBody: {}\n", status, body);
            // 把响应体一并回传：4xx/5xx 时 OpenAI 通常会带具体的 message/error.code
            return Err(AppError::Generic(format!(
                "OpenAI API request failed: {} | body: {}",
                status, body
            )));
        }

        let body_text = response
            .text()
            .await
            .map_err(|e| AppError::Generic(format!("Failed to read response body: {}", e)))?;

        let chat_resp: ChatResponse = serde_json::from_str(&body_text).map_err(|e| {
            AppError::Generic(format!(
                "Failed to parse response: {}\n--- raw body ---\n{}",
                e, body_text
            ))
        })?;

        // 调试日志：打印响应 JSON
        // let response_json = serde_json::to_string_pretty(&chat_resp)
        //     .unwrap_or_else(|_| "Failed to serialize response".into());
        // println!("[DEBUG] === LLM Response ===");
        // println!("{}\n", response_json);

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

        Ok(Message::assistant(
            choice.message.content.unwrap_or_default(),
            tool_calls,
            chat_resp.usage,
        ))
    }
}
