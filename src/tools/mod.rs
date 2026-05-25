use crate::error::Result;
use crate::schema::{ToolCall, ToolDefinition, ToolResult};
pub trait Registry {
    fn get_available_tools(&self) -> Vec<ToolDefinition>;
    fn execute(&self, call: &ToolCall) -> Result<ToolResult>;
}
