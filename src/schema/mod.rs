use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub enum RoleType {
    System(String),
    User(String),
    Assistant(String),
    Tool(String),
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Message {
    pub role: RoleType,
    pub content: String,
    pub tool_calls: Option<Vec<ToolCall>>,
    pub tool_call_id: Option<String>,
}

impl Message {
    pub fn system(content: &str) -> Self {
        Self {
            role: RoleType::System("system".into()),
            content: content.into(),
            tool_calls: None,
            tool_call_id: None,
        }
    }

    pub fn user(content: &str, tool_call_id: Option<String>) -> Self {
        Self {
            role: RoleType::User("user".into()),
            content: content.into(),
            tool_calls: None,
            tool_call_id: tool_call_id,
        }
    }

    pub fn assistant(content: String) -> Self {
        Self {
            role: RoleType::Assistant("assistant".into()),
            content: content.into(),
            tool_calls: None,
            tool_call_id: None,
        }
    }

    pub fn tool(content: &str, tool_call_id: &str) -> Self {
        Self {
            role: RoleType::Tool("tool".into()),
            content: content.into(),
            tool_calls: None,
            tool_call_id: Some(tool_call_id.into()),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: serde_json::Value,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ToolResult {
    pub tool_call_id: String,
    pub output: String,
    pub is_error: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}
