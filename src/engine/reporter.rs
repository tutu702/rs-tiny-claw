use async_trait::async_trait;

#[async_trait]
pub trait Reporter: Send + Sync {
    async fn on_thinking(&self);
    async fn on_tool_call(&self, tool_name: &str, args: &str);
    async fn on_tool_result(&self, tool_name: &str, result: &str, is_error: bool);
    async fn on_message(&self, content: &str);
}
