use std::sync::{Arc, Mutex};

use crate::{error::Result, provider::LlmProvider, schema::Message, tools::Registry};

pub struct AgentEngine {
    provider: Arc<Mutex<dyn LlmProvider>>,
    registry: Arc<dyn Registry>,
    work_dir: String,
}

impl AgentEngine {
    pub fn new(
        provider: Arc<Mutex<dyn LlmProvider>>,
        registry: Arc<dyn Registry>,
        work_dir: String,
    ) -> Self {
        Self {
            provider,
            registry,
            work_dir,
        }
    }

    pub fn get_work_dir(&self) -> &str {
        &self.work_dir
    }

    pub fn run(&self, user_prompt: &str) -> Result<()> {
        println!("[Engine] 引擎启动, 锁定工作区: {}", self.work_dir);

        let mut context_history = vec![];
        let system_prompt = "You are tiny-claw, an expert coding assistant. You have full access to tools in the workspace.";
        context_history.push(Message::system(system_prompt));
        context_history.push(Message::user(user_prompt, None));

        let mut turn_count = 0;

        loop {
            turn_count += 1;
            println!("========== [Turn {}] 开始 ==========\n", turn_count);

            let available_tools = self.registry.get_available_tools();

            println!("[Engine] 正在思考 (Reasoning)...");

            let mut provider = self.provider.lock().unwrap();

            let mut response = provider.generate(&context_history, available_tools)?;

            if response.content != "" {
                println!("🤖 模型: {}\n", response.content);
            }

            let tool_calls = response.tool_calls.take();
            context_history.push(response);

            if tool_calls.is_none() || tool_calls.as_ref().is_some_and(|v| v.is_empty()) {
                println!("[Engine] 任务完成，退出循环。");
                break;
            }

            for tool_call in tool_calls.unwrap() {
                println!(
                    " -> 🛠️ 执行工具: {}, 参数: {}\n",
                    tool_call.name, tool_call.arguments
                );

                let result = self.registry.execute(&tool_call)?;

                if result.is_error {
                    println!(" -> ❌ 工具执行报错: {}\n", result.output);
                } else {
                    println!(" -> ✅ 工具执行成功 (返回 {} 字节)\n", result.output.len())
                }

                context_history.push(Message::user(&result.output, Some(tool_call.id.clone())));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::*;

    struct MockProvider {
        turn: usize,
    }

    impl MockProvider {
        fn new() -> Self {
            Self { turn: 0 }
        }
    }

    impl LlmProvider for MockProvider {
        fn generate(
            &mut self,
            _messages: &[Message],
            _available_tools: Vec<ToolDefinition>,
        ) -> Result<Message> {
            self.turn += 1;
            if self.turn == 1 {
                return Ok(Message {
                    role: RoleType::Assistant("assistant".into()),
                    content: "让我来看看当前目录下有什么文件。".into(),
                    tool_call_id: None,
                    tool_calls: Some(vec![ToolCall {
                        id: "call_123".into(),
                        name: "bash".into(),
                        arguments: serde_json::json!({"command": "ls -la"}),
                    }]),
                });
            }

            return Ok(Message {
                role: RoleType::Assistant("assistant".into()),
                content: "我看到了文件列表，里面包含 main.go，任务完成！".into(),
                tool_call_id: None,
                tool_calls: None,
            });
        }
    }

    struct MockRegistry {}

    impl MockRegistry {
        fn new() -> Self {
            Self {}
        }
    }

    impl Registry for MockRegistry {
        fn get_available_tools(&self) -> Vec<ToolDefinition> {
            vec![]
        }

        fn execute(&self, call: &ToolCall) -> Result<ToolResult> {
            Ok(ToolResult {
                tool_call_id: call.id.clone(),
                output: "-rw-r--r-- 1 user group 234 Oct 24 10:00 main.go\n".into(),
                is_error: false,
            })
        }
    }

    #[test]
    fn test_agent_engine_run_with_tool_calls() {
        let provider = Arc::new(Mutex::new(MockProvider::new()));
        let registry = Arc::new(MockRegistry::new());

        let engine = AgentEngine::new(provider, registry, "/tmp".to_string());
        let result = engine.run("帮我检查当前目录的文件");

        assert!(result.is_ok());
    }
}
