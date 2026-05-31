use std::collections::HashMap;
use std::sync::Arc;

use crate::error::Result;
use crate::schema::{ToolCall, ToolDefinition, ToolResult};
use async_trait::async_trait;

pub mod bash;
pub mod edit_file;
pub mod read_file;
pub mod write_file;

pub const MAX_CONTENT_LENGTH: usize = 8000;

#[async_trait]
pub trait BaseTool: Send + Sync {
    fn name(&self) -> &str;
    fn definition(&self) -> ToolDefinition;
    async fn execute(&self, args: serde_json::Value) -> Result<String>;
}

#[async_trait]
pub trait Registry: Send + Sync {
    fn register(&mut self, tool: Arc<dyn BaseTool>);
    fn get_available_tools(&self) -> Vec<ToolDefinition>;
    async fn execute(&self, call: &ToolCall) -> Result<ToolResult>;
}

pub struct ToolRegistry {
    tools: HashMap<String, Arc<dyn BaseTool>>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
        }
    }
}

#[async_trait]
impl Registry for ToolRegistry {
    fn register(&mut self, tool: Arc<dyn BaseTool>) {
        let name = tool.name().to_string();
        if self.tools.contains_key(&name) {
            eprintln!("[Warning] 工具 '{}' 已经被注册，将被覆盖。\n", name);
        }
        self.tools.insert(name.clone(), tool);
        println!("[Registry] 成功挂载工具: {}\n", name)
    }

    fn get_available_tools(&self) -> Vec<ToolDefinition> {
        self.tools.values().map(|t| t.definition()).collect()
    }

    async fn execute(&self, call: &ToolCall) -> Result<ToolResult> {
        let Some(tool) = self.tools.get(&call.name) else {
            return Ok(ToolResult {
                tool_call_id: call.id.clone(),
                output: format!("Error: 系统中不存在名为 '{}' 的工具。", call.name),
                is_error: true,
            });
        };

        let output = tool.execute(call.arguments.clone()).await;
        match output {
            Ok(content) => Ok(ToolResult {
                tool_call_id: call.id.clone(),
                output: content,
                is_error: false,
            }),
            Err(e) => Ok(ToolResult {
                tool_call_id: call.id.clone(),
                output: format!("Error executing {}: {}", call.name, e),
                is_error: true,
            }),
        }
    }
}
