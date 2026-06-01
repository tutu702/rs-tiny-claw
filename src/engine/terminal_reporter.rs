use crate::engine::reporter::Reporter;
use async_trait::async_trait;

pub struct TerminalReporter {}

impl TerminalReporter {
    pub fn new() -> Self {
        Self {}
    }
}

#[async_trait]
impl Reporter for TerminalReporter {
    async fn on_thinking(&self) {
        todo!()
    }
    async fn on_tool_call(&self, tool_name: &str, args: &str) {
        todo!()
    }
    async fn on_tool_result(&self, tool_name: &str, result: &str, is_error: bool) {
        todo!()
    }
    async fn on_message(&self, content: &str) {
        todo!()
    }
}
