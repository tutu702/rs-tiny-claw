use crate::{
    context::{compactor::Compactor, composer::PromptComposer, recovery::RecoveryManager},
    engine::{
        reminder::ReminderInjector,
        reporter::{self, Reporter},
        session::Session,
    },
    error::{AppError, Result},
    provider::LlmProvider,
    schema::{Message, ToolCall, ToolResult},
    tools::{Registry, safe_truncate, subagent::AgentRunner},
};
use async_trait::async_trait;
use std::sync::{Arc, Mutex as StdMutex};
use tokio::sync::Mutex;

pub struct AgentEngine {
    provider: Arc<Mutex<dyn LlmProvider>>,
    registry: Arc<dyn Registry>,
    work_dir: String,
    enable_thinking: bool,
    // composer: PromptComposer,
    compactor: Compactor,
    plan_mode: bool,
    recovery: RecoveryManager,
    injector: StdMutex<ReminderInjector>,
}

impl AgentEngine {
    pub fn new(
        provider: Arc<Mutex<dyn LlmProvider>>,
        registry: Arc<dyn Registry>,
        work_dir: &str,
        enable_thinking: bool,
        plan_mode: bool,
    ) -> Self {
        Self {
            provider,
            registry,
            work_dir: work_dir.to_string(),
            enable_thinking,
            // composer: PromptComposer::new(&work_dir, plan_mode),
            compactor: Compactor::new(20000, 6),
            plan_mode,
            recovery: RecoveryManager::new(),
            injector: StdMutex::new(ReminderInjector::new()),
        }
    }

    pub fn get_work_dir(&self) -> &str {
        &self.work_dir
    }

    pub fn set_registry(&mut self, r: Arc<dyn Registry>) {
        self.registry = r;
    }

    pub async fn run(&self, session: Arc<Session>, reporter: &dyn Reporter) -> Result<()> {
        println!(
            "[Engine] 唤醒会话 [{}]，锁定工作区: {} (PlanMode: {})\n",
            session.id(),
            session.work_dir(),
            self.plan_mode,
        );

        let composer = PromptComposer::new(&self.work_dir, self.plan_mode);
        let system_msg = composer.build()?;

        // let mut turn_count = 0;

        loop {
            // turn_count += 1;
            // println!("========== [Turn {}] 开始 ==========\n", turn_count);
            let mut context_history = vec![];

            let working_memory = session.get_working_memory(20)?;
            context_history.push(system_msg.clone());
            context_history.extend(working_memory);

            let mut compacted_context = self.compactor.compact(&context_history)?;

            let mut current_turn_thinking_content = String::new();
            if self.enable_thinking {
                // println!("[Engine][Phase 1] 剥夺工具访问权，强制进入慢思考与规划阶段...");
                reporter.on_thinking().await;

                let thinking_response = {
                    let mut provider = self.provider.lock().await;
                    provider.generate(&compacted_context, None).await
                };

                match thinking_response {
                    Ok(v) => {
                        if v.content != "" {
                            // println!("🧠 [内部思考 Trace]: {}\n", v.content);
                            // session.append(&[v.clone()])?;
                            current_turn_thinking_content.push_str(&v.content);
                            compacted_context.push(v);
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

            // println!("[Engine][Phase 2] 恢复工具挂载，等待模型采取行动...");
            let available_tools = self.registry.get_available_tools().await;
            let response = {
                let mut provider = self.provider.lock().await;
                provider
                    .generate(&compacted_context, Some(available_tools))
                    .await
            };

            let mut message = match response {
                Ok(v) => {
                    if v.content != "" {
                        // println!("🤖 [对外回复]: {}\n", v.content);
                        reporter.on_message(&v.content).await;
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
            let final_assistant_msg = Message::assistant(
                format!("{} \n {}", current_turn_thinking_content, message.content),
                tool_calls.clone(),
            );
            session.append(&[final_assistant_msg])?;
            compacted_context.push(message);

            let Some(tool_calls) = tool_calls.filter(|tc| !tc.is_empty()) else {
                println!("[Engine] 模型未请求调用工具，任务宣告完成。");
                break;
            };

            // println!("[Engine] 模型请求调用 {} 个工具...\n", tool_calls.len());

            let mut observation_msgs = vec![Message::user("", None); tool_calls.len()];
            let mut tasks = Vec::with_capacity(tool_calls.len());

            for (i, tool_call) in tool_calls.into_iter().enumerate() {
                reporter
                    .on_tool_call(&tool_call.name, &tool_call.arguments.to_string())
                    .await;

                let registry = self.registry.clone();
                tasks.push(tokio::spawn(async move {
                    let result = registry.execute(&tool_call).await?;
                    Ok::<_, AppError>((i, result, tool_call))
                }));
            }

            let mut last_tool_call: Option<ToolCall> = None;
            let mut last_result: Option<ToolResult> = None;
            while let Some(res) = tasks.pop() {
                match res.await {
                    Ok(Ok((idx, result, tool_call))) => {
                        let mut final_output = result.output.clone();
                        if result.is_error {
                            final_output = self
                                .recovery
                                .analyze_and_inject(&tool_call.name, &result.output);
                            println!("-> [Go-{}] ❌ 注入救援指南: {}\n", idx, final_output);
                        } else {
                            println!(
                                " -> [Go-{}] ✅ 工具执行成功 (返回 {} 字节)\n",
                                idx,
                                result.output.len(),
                            );
                        }

                        let mut display_output = final_output.clone();
                        if display_output.len() > 200 {
                            display_output =
                                format!("{}... (已截断)", safe_truncate(&display_output, 200));
                        }
                        reporter
                            .on_tool_result(&result.tool_call_id, &display_output, result.is_error)
                            .await;
                        // 喂给 LLM 的是注入救援指南后的内容，且必须是 role=tool 消息
                        observation_msgs[idx] = Message::tool(&final_output, &tool_call.id);
                        if idx == 0 {
                            last_tool_call = Some(tool_call);
                            last_result = Some(result);
                        }
                    }
                    Ok(Err(e)) => return Err(e),
                    Err(e) => {
                        return Err(AppError::Generic(format!(
                            "任务执行失败: {}",
                            e.to_string()
                        )));
                    }
                }
            }

            session.append(&observation_msgs)?;

            if let (Some(tool_call), Some(res)) = (last_tool_call.as_ref(), last_result.as_ref()) {
                let mut injector = self.injector.lock().expect("injector mutex poisoned");
                if let Some(reminder_msg) = injector.check_and_inject(tool_call, &res) {
                    session.append(&[reminder_msg])?;
                }
            }

            // for msg in observation_msgs {
            //     context_history.push(msg)
            // }
        }
        Ok(())
    }
}

#[async_trait]
impl AgentRunner for AgentEngine {
    async fn run_sub(
        &self,
        task_prompt: &str,
        read_only_registry: &dyn Registry,
        reporter: &dyn Reporter,
    ) -> Result<String> {
        // 【核心优化】：子智能体极其容易偷懒。我们必须在 System Prompt 中严厉警告它必须使用工具！
        let mut context_history = vec![Message::system("你是一个专门负责深度探索的探路者 (Explorer Subagent)。
你的任务是根据主架构师的指令，在当前工作区内仔细阅读代码、查阅日志，搜集足够的信息。
【核心纪律】
1. 你必须、且只能依靠内置工具（如 bash 的 find/grep，或 read_file）去寻找答案。绝对不允许凭空捏造或猜测！
2. 如果你没有找到确切的答案，你必须继续使用工具深入搜索。
3. 当且仅当你找到了确切的线索后，停止调用工具，直接输出一段纯文本作为你的终极汇报。主架构师会根据你的汇报来做下一步决策。"),
        Message::user(task_prompt, None),
];
        // 限制子智能体最多只能跑 10 个 Turn，防止它自己卡死
        const MAX_SUB_TRUNS: u8 = 10;
        let mut turn_count: u8 = 0;

        loop {
            turn_count += 1;
            if turn_count > MAX_SUB_TRUNS {
                return Err(AppError::Generic(format!(
                    "子智能体探索过于深入，超过 {} 轮被强制召回，请主 Agent 给它更明确的指令",
                    MAX_SUB_TRUNS
                )));
            }

            let available_tools = read_only_registry.get_available_tools().await;
            let compacted_context = self.compactor.compact(&context_history)?;

            let response = {
                let mut provider = self.provider.lock().await;
                provider
                    .generate(&compacted_context, Some(available_tools))
                    .await
            };

            let mut message = match response {
                Ok(v) => v,
                Err(err) => return Err(AppError::Generic(format!("子智能体推理失败: {}", err))),
            };

            let tool_calls = message.tool_calls.take();
            let Some(tool_calls) = tool_calls.filter(|tc| !tc.is_empty()) else {
                // 直接将它的这段汇报内容剥离出来返回给上层
                return Ok(message.content);
            };

            context_history.push(message);

            // 执行只读工具的并发循环
            let mut observation_msgs = vec![Message::user("", None); tool_calls.len()];
            let mut task = Vec::with_capacity(tool_calls.len());

            for (i, tool_call) in tool_calls.into_iter().enumerate() {
                reporter
                    .on_tool_call(
                        &format!("[Subagent] {}", tool_call.name),
                        &tool_call.arguments.to_string(),
                    )
                    .await;
                let registry = self.registry.clone();
                task.push(tokio::spawn(async move {
                    let result = registry.execute(&tool_call).await?;
                    Ok::<_, AppError>((i, result, tool_call))
                }));
            }

            while let Some(res) = task.pop() {
                match res.await {
                    Ok(Ok((idx, result, tool_call))) => {
                        let mut final_output = result.output.clone();
                        if result.is_error {
                            final_output = self
                                .recovery
                                .analyze_and_inject(&tool_call.name, &result.output)
                        }

                        let mut display_output = final_output.clone();
                        if display_output.len() > 200 {
                            display_output =
                                format!("{}... (已截断)", safe_truncate(&display_output, 200));
                        }

                        reporter
                            .on_tool_result(
                                &format!("[Subagent] {}", tool_call.name),
                                &display_output,
                                result.is_error,
                            )
                            .await;

                        observation_msgs[idx] = Message::user(&final_output, Some(tool_call.id));
                    }
                    Ok(Err(err)) => return Err(err),
                    Err(err) => {
                        return Err(AppError::Generic(format!(
                            "任务执行失败: {}",
                            err.to_string()
                        )));
                    }
                }
            }

            context_history.extend_from_slice(&observation_msgs);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        engine::{session::GLOBAL_SESSION_MGR, terminal_reporter::TerminalReporter},
        provider::openai::OpenaiProvider,
        schema::*,
        tools::MiddlewareFunc,
    };

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
        async fn register(&self, _tool: Arc<dyn crate::tools::BaseTool>) {}

        async fn use_mw(&self, _mw: MiddlewareFunc) {}

        async fn get_available_tools(&self) -> Vec<ToolDefinition> {
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

        let work_dir = "/tmp";
        let engine = AgentEngine::new(provider, registry, work_dir, true, false);

        let t_reporter = TerminalReporter::new();
        let session = GLOBAL_SESSION_MGR
            .get_or_create("test_001", work_dir)
            .unwrap();
        let prompt = "我想去北京跑步，帮我查查天气适合吗？";
        session.append(&[Message::user(prompt, None)]).unwrap();
        let result = engine.run(session, &t_reporter).await;

        assert!(result.is_ok());
    }
}
