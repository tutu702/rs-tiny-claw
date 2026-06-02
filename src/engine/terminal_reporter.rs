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
        println!("\n[🤔 思考中] 模型正在推理...\n")
    }
    async fn on_tool_call(&self, tool_name: &str, args: &str) {
        println!("[🛠️ 调用工具] {}\n", tool_name);
        let mut display_args = args.replace("\n", "\\n");
        display_args = display_args.replace("\r", "\\r");
        if display_args.len() > 150 {
            display_args = format!("{} ... (已截断)", &display_args[..150])
        }
        println!("    参数: {}\n", display_args)
    }
    async fn on_tool_result(&self, tool_name: &str, result: &str, is_error: bool) {
        if is_error {
            println!("[❌ 执行失败] {}\n", tool_name);
            if result != "" {
                println!(" 错误: {}\n", result)
            }
        } else {
            println!("[✅ 执行成功] {}\n", tool_name)
        }
    }
    async fn on_message(&self, content: &str) {
        if content == "" {
            return;
        }

        println!("\n🤖 Agent 回复:\n{}\n\n", content)
    }
}
