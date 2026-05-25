use crate::error::Result;
use crate::schema::{Message, ToolDefinition};

pub trait LlmProvider {
    fn generate(
        &mut self,
        messages: &[Message],
        available_tools: Vec<ToolDefinition>,
    ) -> Result<Message>;
}
