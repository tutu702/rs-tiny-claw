use crate::error::Result;
use crate::schema::{ToolCall, ToolDefinition, ToolResult};
use async_trait::async_trait;
use std::collections::HashMap;
use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::RwLock;

pub mod bash;
pub mod edit_file;
pub mod read_file;
pub mod subagent;
pub mod write_file;

pub const MAX_CONTENT_LENGTH: usize = 8000;

pub type MiddlewareFunc =
    Box<dyn Fn(ToolCall) -> Pin<Box<dyn Future<Output = (bool, String)> + Send>> + Send + Sync>;

#[async_trait]
pub trait BaseTool: Send + Sync {
    fn name(&self) -> &str;
    fn definition(&self) -> ToolDefinition;
    async fn execute(&self, args: serde_json::Value) -> Result<String>;
}

#[async_trait]
pub trait Registry: Send + Sync {
    async fn register(&self, tool: Arc<dyn BaseTool>);
    async fn get_available_tools(&self) -> Vec<ToolDefinition>;
    async fn use_mw(&self, mw: MiddlewareFunc);
    async fn execute(&self, call: &ToolCall) -> Result<ToolResult>;
}

pub struct ToolRegistry {
    tools: RwLock<HashMap<String, Arc<dyn BaseTool>>>,
    middlewares: RwLock<Vec<MiddlewareFunc>>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self {
            tools: RwLock::new(HashMap::new()),
            middlewares: RwLock::new(Vec::new()),
        }
    }
}

#[async_trait]
impl Registry for ToolRegistry {
    async fn register(&self, tool: Arc<dyn BaseTool>) {
        let name = tool.name().to_string();
        {
            let tools = self.tools.read().await;
            if tools.contains_key(&name) {
                eprintln!("[Warning] 工具 '{}' 已经被注册，将被覆盖。\n", name);
            }
        }

        self.tools.write().await.insert(name.clone(), tool);
        println!("[Registry] 成功挂载工具: {}\n", name)
    }

    async fn get_available_tools(&self) -> Vec<ToolDefinition> {
        self.tools
            .read()
            .await
            .values()
            .map(|t| t.definition())
            .collect()
    }

    async fn use_mw(&self, mw: MiddlewareFunc) {
        self.middlewares.write().await.push(mw);
    }

    async fn execute(&self, call: &ToolCall) -> Result<ToolResult> {
        let tool = {
            let tools = self.tools.read().await;
            tools.get(&call.name).cloned()
        };
        let Some(tool) = tool else {
            return Ok(ToolResult {
                tool_call_id: call.id.clone(),
                output: format!("Error: 系统中不存在名为 '{}' 的工具。", call.name),
                is_error: true,
            });
        };

        for mw in self.middlewares.read().await.iter() {
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
