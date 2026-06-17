use std::{
    env, fs,
    process::Command,
    sync::Arc,
    time::{self, Instant},
};

use crate::{
    engine::{
        r#loop::AgentEngine, session::GLOBAL_SESSION_MGR, terminal_reporter::TerminalReporter,
    },
    error::{AppError, Result},
    observability::tracker::CostTracker,
    provider::openai::OpenaiProvider,
    schema::Message,
    tools::{
        Registry, ToolRegistry, bash::BashTool, edit_file::EditFileTool, read_file::ReadFileTool,
        write_file::WriteFileTool,
    },
};
use chrono::Utc;
use tokio::sync::Mutex;

pub struct TestCase {
    pub id: String,
    pub name: String,
    pub setup_script: String,
    pub task_prompt: String,
    pub validate_script: String,
    pub max_turns: u8,
}

impl TestCase {
    pub fn new(
        id: &str,
        name: &str,
        setup_script: &str,
        task_script: &str,
        validate_script: &str,
        max_turns: u8,
    ) -> Self {
        Self {
            id: id.to_string(),
            name: name.to_string(),
            setup_script: setup_script.to_string(),
            task_prompt: task_script.to_string(),
            validate_script: validate_script.to_string(),
            max_turns,
        }
    }
}

pub struct TestResult {
    pub test_case_id: String,
    pub passed: bool,
    pub total_cost_cny: f64,
    pub duration_ms: i64,
    pub error_msg: String,
}

pub struct BenchmarkRunner {
    base_url: String,
    api_key: String,
    model_name: String,
}

impl BenchmarkRunner {
    pub fn new(base_url: &str, model_name: &str, api_key: &str) -> Self {
        Self {
            base_url: base_url.to_string(),
            api_key: api_key.to_string(),
            model_name: model_name.to_string(),
        }
    }

    pub async fn run_suite(&self, test_cases: Vec<TestCase>) -> Result<()> {
        println!("==================================================");
        println!(
            "🚀 启动自动化 Harness Benchmark 评估... | 模型: {}\n",
            self.model_name
        );
        println!("==================================================");

        let total_case = test_cases.len();
        let mut results = Vec::new();
        let mut passed_count = 0;
        let mut total_cost = 0.0;

        for tc in test_cases {
            println!("\n>>> ⏳ 正在执行用例 [{}]: {}\n", tc.id, tc.name);
            let res = self.run_single_test(&tc).await?;

            if res.passed {
                passed_count += 1;
                println!(
                    ">>> ✅ 用例 [{}] 测试通过! | 耗时: {}ms | 花费: ${:.6}\n",
                    tc.id, res.duration_ms, res.total_cost_cny
                )
            } else {
                println!(
                    ">>> ❌ 用例 [{}] 测试失败! | 错误: {}\n",
                    tc.id, res.error_msg
                );
            }
            total_cost += res.total_cost_cny;
            results.push(res);
        }

        // 打印终极报表
        println!("\n================ 🏆 跑分终极报告 ================");
        println!(
            "总用例数: {} | 成功数: {} | 成功率: {:.2}%%\n",
            total_case,
            passed_count,
            (passed_count as f64 / total_case as f64) * 100.0,
        );
        println!("总消耗成本: ${:6}f\n", total_cost);
        println!("==================================================");

        Ok(())
    }

    pub async fn run_single_test(&self, tc: &TestCase) -> Result<TestResult> {
        let start_time = Instant::now();

        let work_dir = env::current_dir().map_err(|err| AppError::Generic(format!("{}", err)))?;
        let work_dir = work_dir.join(format!("workspace/{}_{}", tc.id, Utc::now().timestamp()));
        println!("work_dir: {}", work_dir.display());
        fs::create_dir_all(&work_dir).map_err(|err| {
            AppError::Generic(format!("创建目录 `{}` 失败，{}", work_dir.display(), err))
        })?;

        if !tc.setup_script.is_empty() {
            // const TIMEOUT_SECS: u64 = 30;
            // let timeout_dur = time::Duration::from_secs(TIMEOUT_SECS);
            let output = Command::new("bash")
                .arg("-c")
                .arg(tc.setup_script.clone())
                .current_dir(&work_dir)
                .output()
                .map_err(|err| AppError::Generic(format!("{}", err)))?;

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                return Ok(TestResult {
                    test_case_id: tc.id.clone(),
                    passed: false,
                    error_msg: format!("靶机 Setup 失败: {}", stderr),
                    duration_ms: 0,
                    total_cost_cny: f64::default(),
                });
            }
        }

        let work_dir = work_dir.to_string_lossy().to_string();

        let llm_provider = OpenaiProvider::new(&self.base_url, &self.model_name, &self.api_key);

        let session = GLOBAL_SESSION_MGR.get_or_create(&tc.id, &work_dir)?;
        let tracked_provider = CostTracker::new(
            Box::new(llm_provider),
            &self.model_name,
            Arc::clone(&session),
        );
        // 【防御沙箱】为子智能体准备受限的只读注册表
        let registry = ToolRegistry::new();
        registry
            .register(Arc::new(ReadFileTool::new(&work_dir)))
            .await;
        registry.register(Arc::new(BashTool::new(&work_dir))).await;

        let registry = Arc::new(ToolRegistry::new());
        registry
            .register(Arc::new(ReadFileTool::new(&work_dir)))
            .await;
        registry
            .register(Arc::new(WriteFileTool::new(&work_dir)))
            .await;
        registry.register(Arc::new(BashTool::new(&work_dir))).await;
        registry
            .register(Arc::new(EditFileTool::new(&work_dir)))
            .await;

        let engine = Arc::new(AgentEngine::new(
            Arc::new(Mutex::new(tracked_provider)),
            Arc::clone(&registry) as Arc<dyn Registry>,
            &work_dir,
            false,
            false,
        ));

        let reporter = TerminalReporter::new();
        session.append(&[Message::user(&tc.task_prompt, None)])?;

        if let Err(err) = engine.run(session.clone(), &reporter).await {
            return Ok(TestResult {
                test_case_id: tc.id.clone(),
                passed: false,
                error_msg: format!("Agent 崩溃: {}", err),
                duration_ms: 0,
                total_cost_cny: f64::default(),
            });
        }

        //【核心断言】Agent 跑完了，我们来验收成果！
        let output = Command::new("bash")
            .arg("-c")
            .arg(tc.validate_script.clone())
            .current_dir(work_dir)
            .output()
            .map_err(|err| AppError::Generic(format!("{}", err)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Ok(TestResult {
                test_case_id: tc.id.clone(),
                passed: false,
                error_msg: format!("验证脚本执行失败: {}", stderr),
                duration_ms: 0,
                total_cost_cny: f64::default(),
            });
        }

        let duration = start_time.elapsed().as_millis() as i64;

        Ok(TestResult {
            test_case_id: tc.id.clone(),
            passed: true,
            total_cost_cny: session.get_total_cost(),
            duration_ms: duration,
            error_msg: String::new(),
        })
    }
}
