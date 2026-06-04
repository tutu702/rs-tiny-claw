use std::{env, sync::Arc, time::Duration};

use anyhow::Result;
use rs_tiny_claw::{
    channel::feishu_bot::FeishuBot,
    engine::{
        r#loop::AgentEngine, reporter::Reporter, session::GLOBAL_SESSION_MGR,
        terminal_reporter::TerminalReporter,
    },
    provider::openai::OpenaiProvider,
    schema::Message,
    tools::{
        Registry, ToolRegistry, bash::BashTool, edit_file::EditFileTool, read_file::ReadFileTool,
        write_file::WritefileTool,
    },
};
use tokio::sync::Mutex;

#[tokio::main]
async fn main() -> Result<()> {
    let work_dir = env::current_dir()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| ".".into());
    // let work_dir = "/tmp/project_front";

    let base_url = std::env::var("OPENAI_BASE_URL")?;
    let model = std::env::var("LLM_MODEL")?;
    let api_key = std::env::var("LLM_API_KEY")?;

    println!("work_dir: {work_dir}");

    let llm_provider = OpenaiProvider::new(&base_url, &model, &api_key);
    let provider = Arc::new(Mutex::new(llm_provider));

    let mut registry = ToolRegistry::new();
    registry.register(Arc::new(ReadFileTool::new(&work_dir)));
    // registry.register(Arc::new(WritefileTool::new(&work_dir)));
    registry.register(Arc::new(BashTool::new(&work_dir)));
    // registry.register(Arc::new(EditFileTool::new(&work_dir)));

    let engine = Arc::new(AgentEngine::new(
        provider,
        Arc::new(registry),
        &work_dir,
        false,
    ));

    // cli_start_with_session(engine).await?;
    cli_start(&work_dir, engine).await?;
    // feishu_bot_start(engine).await?;

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
        session.append(&[Message::assistant("好的，收到闲聊。".into())])?;

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

async fn cli_start(work_dir: &str, engine: Arc<AgentEngine>) -> Result<()> {
    let reporter = TerminalReporter::new();
    let prompt = r#"请帮我执行以下三个步骤：
    1. 使用 bash 执行 echo "开始排查日志"
    2. 使用 read_file 工具读取当前目录下的巨大文件 mock_log.txt
    3. 使用 bash 执行 date 命令获取当前时间，并告诉我任务全部完成。"#;

    let session = GLOBAL_SESSION_MGR.get_or_create("test_oom_protection_001", work_dir)?;
    session.append(&[Message::user(prompt, None)])?;
    engine
        .run(session, &reporter)
        .await
        .map_err(anyhow::Error::from)
}

async fn feishu_bot_start(engine: Arc<AgentEngine>) -> Result<()> {
    let app_id = std::env::var("FEISHU_APP_ID")?;
    let app_secret = std::env::var("FEISHU_APP_SECRET")?;
    let base_url = std::env::var("FEISHU_BASE_URL")?;
    let bot = Arc::new(FeishuBot::new(&app_id, &app_secret, &base_url, engine));
    bot.start_websocket().await.map_err(anyhow::Error::from)
}
