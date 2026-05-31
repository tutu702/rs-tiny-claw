use std::sync::{Arc};
use tokio::sync::Mutex;
use crate::{
    error::{AppError, Result},
    provider::LlmProvider,
    schema::Message,
    tools::Registry,
};

pub struct AgentEngine {
    provider: Arc<Mutex<dyn LlmProvider>>,
    registry: Arc<dyn Registry>,
    work_dir: String,
    enable_thinking: bool,
}

impl AgentEngine {
    pub fn new(
        provider: Arc<Mutex<dyn LlmProvider>>,
        registry: Arc<dyn Registry>,
        work_dir: String,
        enable_thinking: bool,
    ) -> Self {
        Self {
            provider,
            registry,
            work_dir,
            enable_thinking,
        }
    }

    pub fn get_work_dir(&self) -> &str {
        &self.work_dir
    }

    pub async fn run(&self, user_prompt: &str) -> Result<()> {
        println!("[Engine] 引擎启动, 锁定工作区: {}", self.work_dir);
        println!(
            "[Engine] 慢思考模式 (Thinking Phase): {}",
            self.enable_thinking
        );

        let mut context_history = vec![];
        let system_prompt = "You are tiny-claw, an expert coding assistant. You have full access to tools in the workspace.";
        context_history.push(Message::system(system_prompt));
        context_history.push(Message::user(user_prompt, None));

        let mut turn_count = 0;
        const MAX_TURNS: usize = 50;

        loop {
            turn_count += 1;
            println!("========== [Turn {}] 开始 ==========\n", turn_count);

            if turn_count > MAX_TURNS {
                return Err(AppError::Generic("达到最大轮次限制".into()));
            }

            if self.enable_thinking {
                println!("[Engine][Phase 1] 剥夺工具访问权，强制进入慢思考与规划阶段...");

                let thinking_response = {
                    let mut provider = self.provider.lock().await;
                    provider.generate(&context_history, None).await
                };

                match thinking_response {
                    Ok(v) => {
                        if v.content != "" {
                            println!("🧠 [内部思考 Trace]: {}\n", v.content);
                            context_history.push(v);
                        }
                    }
                    Err(e) => {
                        return Err(AppError::Generic(format!(
                            "Thinking 阶段生成失败: {}",
                            e.to_string()
                        )));
                    }
                }
            }

            println!("[Engine][Phase 2] 恢复工具挂载，等待模型采取行动...");

            let available_tools = self.registry.get_available_tools();
            let response = {
                let mut provider = self.provider.lock().await;
                provider
                    .generate(&context_history, Some(available_tools))
                    .await
            };

            let mut message = match response {
                Ok(v) => {
                    if v.content != "" {
                        println!("🤖 [对外回复]: {}\n", v.content);
                    }
                    v
                }
                Err(e) => {
                    return Err(AppError::Generic(format!(
                        "Action 阶段生成失败: {}",
                        e.to_string()
                    )));
                }
            };

            let tool_calls = message.tool_calls.take();
            context_history.push(message);

            let Some(tool_calls) = tool_calls.filter(|tc| !tc.is_empty()) else {
                println!("[Engine] 模型未请求调用工具，任务宣告完成。");
                break;
            };

            println!("[Engine] 模型请求调用 {} 个工具...\n", tool_calls.len());

            let mut observation_msgs = vec![Message::user("", None); tool_calls.len()];
            let mut tasks: Vec<_> = tool_calls
                .into_iter()
                .enumerate()
                .map(|(i, tool_call)| {
                    let registry = self.registry.clone();
                    tokio::spawn(async move {
                        println!(
                            " -> 🛠️ 执行工具: {}, 参数: {}\n",
                            tool_call.name, tool_call.arguments
                        );
                        let result = registry.execute(&tool_call).await?;
                        if result.is_error {
                            println!(" -> ❌ 工具执行报错: {}\n", result.output);
                        } else {
                            println!(
                                " -> ✅ 工具执行成功 (返回 {} 字节)\n",
                                result.output.len()
                            )
                        }

                        Ok::<_, AppError>((i, Message::user(&result.output, Some(tool_call.id.clone()))))
                    })
                })
                .collect();

            while let Some(res) = tasks.pop() {
                match res.await {
                    Ok(Ok((idx, msg))) => observation_msgs[idx] = msg,
                    Ok(Err(e)) => return Err(e),
                    Err(e) => {
                        return Err(AppError::Generic(format!(
                            "任务执行失败: {}",
                            e.to_string()
                        )))
                    }
                }
            }

            for msg in observation_msgs {
                context_history.push(msg)
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{provider::openai::OpenaiProvider, schema::*};

    struct MockRegistry {}

    impl MockRegistry {
        fn new() -> Self {
            Self {}
        }
    }

    // MockRegistry 必须实现 Send + Sync 才能用于多线程 spawn
    unsafe impl Send for MockRegistry {}
    unsafe impl Sync for MockRegistry {}

    #[async_trait::async_trait]
    impl Registry for MockRegistry {
        fn register(&mut self, _tool: Arc<dyn crate::tools::BaseTool>) {}

        fn get_available_tools(&self) -> Vec<ToolDefinition> {
            vec![ToolDefinition {
                name: "get_weather".into(),
                description: "获取指定城市的当前天气情况。".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "city": {
                            "type": "string",
                        }
                    },
                    "required": ["city"]
                }),
            }]
        }

        async fn execute(&self, call: &ToolCall) -> Result<ToolResult> {
            println!("-> [Mock 工具执行] 获取 {} 的天气中...\n", call.name);
            Ok(ToolResult {
                tool_call_id: call.id.clone(),
                output: "API 返回：今天是晴天，气温 25 度。".into(),
                is_error: false,
            })
        }
    }

    #[tokio::test]
    async fn test_agent_engine_run_with_tool_calls() {
        let base_url = "https://api.minimaxi.com/v1";
        let model = "MiniMax-M2.7";
        let api_key = std::env::var("LLM_API_KEY").expect("请设置环境变量 LLM_API_KEY");
        let llm_provider = OpenaiProvider::new(base_url, model, &api_key);
        let provider = Arc::new(Mutex::new(llm_provider));
        let registry = Arc::new(MockRegistry::new());

        let engine = AgentEngine::new(provider, registry, "/tmp".to_string(), true);

        let prompt = "我想去北京跑步，帮我查查天气适合吗？";
        let result = engine.run(prompt).await;

        assert!(result.is_ok());
    }
}
