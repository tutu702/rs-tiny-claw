use anyhow::Result;
use clap::Parser;
use rs_tiny_claw::{
    channel::{
        feishu_approval::{self, GLOBAL_APPROVAL_MGR},
        feishu_bot::{FeishuBot, FeishuReporter},
    },
    engine::{
        r#loop::AgentEngine,
        reporter::Reporter,
        session::{GLOBAL_SESSION_MGR, Session},
        terminal_reporter::TerminalReporter,
    },
    observability::tracker::CostTracker,
    provider::openai::OpenaiProvider,
    schema::Message,
    tools::{
        Registry, ToolRegistry,
        bash::BashTool,
        edit_file::EditFileTool,
        read_file::ReadFileTool,
        subagent::{AgentRunner, SubAgentTool},
        write_file::WriteFileTool,
    },
};
use std::{env, sync::Arc, time::Duration};
use tokio::sync::Mutex;

#[derive(Debug, Parser)]
#[command(name = "tiny-claw")]
#[command(version = "1.0")]
#[command(about, long_about = None)]
struct Cli {
    #[arg(short, long)]
    prompt: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    let work_dir = env::current_dir()
        .map(|p| p.join("workspace").to_string_lossy().to_string())
        .unwrap_or_else(|_| ".".into());
    // let work_dir = "/tmp/project_front";

    let base_url = std::env::var("OPENAI_BASE_URL")?;
    let model = std::env::var("LLM_MODEL")?;
    let api_key = std::env::var("LLM_API_KEY")?;

    println!("work_dir: {work_dir}");

    println!("\n>>> 🚀 收到指令: {:?}\n", cli.prompt);

    // feishu_bot_start(&base_url, &model, &api_key, &work_dir).await?;
    cli_run(&base_url, &model, &api_key, &work_dir, &cli.prompt.unwrap()).await?;

    Ok(())
}

async fn cli_run(
    base_url: &str,
    model: &str,
    api_key: &str,
    work_dir: &str,
    prompt: &str,
) -> Result<()> {
    if prompt.is_empty() {
        println!("用法: cargo run --prompt \"你的任务指令\"");
        return Ok(());
    }
    let llm_provider = OpenaiProvider::new(&base_url, &model, &api_key);

    let session = GLOBAL_SESSION_MGR.get_or_create("test_trace_001", work_dir)?;
    let tracked_provider = CostTracker::new(Box::new(llm_provider), model, Arc::clone(&session));
    // 【防御沙箱】为子智能体准备受限的只读注册表
    let read_only_registry = ToolRegistry::new();
    read_only_registry
        .register(Arc::new(ReadFileTool::new(work_dir)))
        .await;
    read_only_registry
        .register(Arc::new(BashTool::new(&work_dir)))
        .await;

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

    registry
        .register(Arc::new(SubAgentTool::new(
            Arc::clone(&engine) as Arc<dyn AgentRunner>,
            Box::new(read_only_registry),
            Box::new(TerminalReporter::new()),
        )))
        .await;

    // cli_start_with_session(engine).await?;
    cli_start(&prompt, engine, session).await?;
    Ok(())
}

async fn cli_start_with_session(engine: Arc<AgentEngine>) -> Result<()> {
    let reporter: Arc<dyn Reporter> = Arc::new(TerminalReporter::new());

    let front = tokio::spawn(run_session_a(engine.clone(), Arc::clone(&reporter)));
    let back = tokio::spawn(run_session_b(engine, reporter));

    let _ = tokio::join!(front, back);
    Ok(())
}

async fn run_session_a(engine: Arc<AgentEngine>, reporter: Arc<dyn Reporter>) -> Result<()> {
    let session = GLOBAL_SESSION_MGR.get_or_create("chat_front_001", "/tmp/project_front")?;

    println!("\n>>> 🙋‍♂️ [Session A / Turn 1]: 帮我看看 README.md 里记录了什么密钥？");
    session.append(&[Message::user("帮我看看 README.md 里记录了什么密钥？", None)])?;
    engine.run(Arc::clone(&session), reporter.as_ref()).await?;

    // 故意制造大量"废话"对话，刷掉记忆（假设 Working Memory Limit=6）
    for _ in 0..6 {
        session.append(&[Message::user("这只是一句闲聊占位符。", None)])?;
        session.append(&[Message::assistant("好的，收到闲聊。".into(), None, None)])?;

        println!("\n>>> 🙋‍♂️ [Session A / Turn 2]: 请直接告诉我，刚才第一轮你查到的那个密钥是什么？");
        session.append(&[Message::user(
            "请直接告诉我，刚才第一轮你查到的那个密钥是什么？不准调用工具！",
            None,
        )])?;
        engine.run(Arc::clone(&session), reporter.as_ref()).await?;
    }
    Ok(())
}

async fn run_session_b(engine: Arc<AgentEngine>, reporter: Arc<dyn Reporter>) -> Result<()> {
    // 稍微错开一点时间发起请求
    tokio::time::sleep(Duration::from_secs(1)).await;

    let session = GLOBAL_SESSION_MGR.get_or_create("chat_back_002", "/tmp/project_back")?;

    println!("\n>>> 🙋‍♂️ [Session B]: 别人查到了一个密钥，你这里能看到吗？");
    session.append(&[Message::user(
        "别人查到了一个密钥，你这里能看到吗？不准调用工具！",
        None,
    )])?;
    engine
        .run(session, reporter.as_ref())
        .await
        .map_err(anyhow::Error::from)
}

async fn cli_start(prompt: &str, engine: Arc<AgentEngine>, session: Arc<Session>) -> Result<()> {
    let reporter = TerminalReporter::new();
    session.append(&[Message::user(prompt, None)])?;
    engine
        .run(Arc::clone(&session), &reporter)
        .await
        .map_err(anyhow::Error::from)?;
    println!("\n================ 财务报表 ================");
    println!("会话 ID: {}", session.id());
    println!("总消耗 Input Tokens: {}", session.get_total_prompt_tokens());
    println!(
        "总消耗 Output Tokens: {}",
        session.get_total_completion_tokens()
    );
    println!("总计费用 (CNY): ¥{:.6}", session.get_total_cost());
    println!("==========================================");
    Ok(())
}

async fn feishu_bot_start(
    base_url: &str,
    model: &str,
    api_key: &str,
    work_dir: &str,
) -> Result<()> {
    let app_id = std::env::var("FEISHU_APP_ID")?;
    let app_secret = std::env::var("FEISHU_APP_SECRET")?;
    let feishu_base_url = std::env::var("FEISHU_BASE_URL")?;

    let session_id = "test_command_intercept_001";
    let sess = GLOBAL_SESSION_MGR.get_or_create(session_id, work_dir)?;
    sess.append(&[Message::user("", None)])?;

    let registry = ToolRegistry::new();
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

    let reporter: Arc<Mutex<Option<FeishuReporter>>> = Arc::new(Mutex::new(None));
    let reporter_mw = Arc::clone(&reporter);
    registry
        .use_mw(Box::new(move |call| {
            let reporter_slot = Arc::clone(&reporter_mw);
            Box::pin(async move {
                let args_str = call.arguments.to_string();

                // 检查是否命中高危特征库
                if feishu_approval::is_dangerous_command(&call.name, &args_str) {
                    let task_id = call.id;
                    let reporter = reporter_slot.lock().await.clone();
                    // 这里还没拿到 bot,审批消息先发到控制台(feishu_approval.rs 的 None 分支)
                    let (allowed, reason) = GLOBAL_APPROVAL_MGR
                        .wait_for_approval(&task_id, &call.name, &args_str, reporter)
                        .await
                        .unwrap_or((false, "审批调用失败".to_string()));
                    if !allowed {
                        // 拒绝,将理由传回给大模型
                        return (false, reason);
                    }
                }

                (true, String::new())
            })
        }))
        .await;

    let llm_provider = OpenaiProvider::new(&base_url, &model, &api_key);
    let provider = Arc::new(Mutex::new(llm_provider));
    let engine = Arc::new(AgentEngine::new(
        provider,
        Arc::new(registry),
        &work_dir,
        false,
        false,
    ));

    let bot = Arc::new(FeishuBot::new(
        &app_id,
        &app_secret,
        &feishu_base_url,
        engine,
        sess,
        reporter,
    ));

    bot.start_websocket().await.map_err(anyhow::Error::from)
}
