use std::collections::HashMap;
use std::pin::Pin;
use std::sync::Arc;

use crate::error::Result;
use crate::schema::{ToolCall, ToolDefinition, ToolResult};
use async_trait::async_trait;

pub mod bash;
pub mod edit_file;
pub mod read_file;
pub mod write_file;

pub const MAX_CONTENT_LENGTH: usize = 8000;

pub type MiddlewareFunc = Box<
    dyn Fn(ToolCall) -> Pin<Box<dyn Future<Output = (bool, String)> + Send>> + Send + Sync,
>;

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
    fn use_mw(&mut self, mw: MiddlewareFunc);
    async fn execute(&self, call: &ToolCall) -> Result<ToolResult>;
}

pub struct ToolRegistry {
    tools: HashMap<String, Arc<dyn BaseTool>>,
    middlewares: Vec<MiddlewareFunc>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
            middlewares: Vec::new(),
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

    fn use_mw(&mut self, mw: MiddlewareFunc) {
        self.middlewares.push(mw);
    }

    async fn execute(&self, call: &ToolCall) -> Result<ToolResult> {
        let Some(tool) = self.tools.get(&call.name) else {
            return Ok(ToolResult {
                tool_call_id: call.id.clone(),
                output: format!("Error: 系统中不存在名为 '{}' 的工具。", call.name),
                is_error: true,
            });
        };

        for mw in &self.middlewares {
            let (allowed, reason) = mw(call.clone()).await;
            if !allowed {
                println!(
                    "[Registry] ⚠️ 工具 {} 被 Middleware 拦截: {}\n",
                    call.name, reason
                );
                return Ok(ToolResult {
                    tool_call_id: call.id.clone(),
                    output: format!("执行被系统拦截。原因: {}", reason),
                    is_error: true,
                });
            }
        }

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

pub fn safe_truncate(s: &str, max_bytes: usize) -> &str {
    if s.len() < max_bytes {
        return s;
    }

    let mut end = s.floor_char_boundary(max_bytes);
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1
    }
    &s[..end]
}

pub fn safe_tail(s: &str, max_bytes: usize) -> &str {
    if s.len() < max_bytes {
        return s;
    }

    let start = s.ceil_char_boundary(s.len() - max_bytes);
    &s[start..]
}
