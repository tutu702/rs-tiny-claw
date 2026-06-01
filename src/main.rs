use anyhow::Result;
use rs_tiny_claw::{
    channel::feishu_bot::FeishuBot,
    engine::{self, r#loop::AgentEngine, reporter::Reporter, terminal_reporter::TerminalReporter},
    provider::openai::OpenaiProvider,
    tools::{
        Registry, ToolRegistry, bash::BashTool, edit_file::EditFileTool, read_file::ReadFileTool,
        write_file::WritefileTool,
    },
};
use std::{env, sync::Arc};
use tokio::sync::Mutex;

#[tokio::main]
async fn main() -> Result<()> {
    let work_dir = env::current_dir()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| ".".to_string());

    let base_url = std::env::var("OPENAI_BASE_URL").expect("请设置环境变量 OPENAI_BASE_URL");
    let model = std::env::var("LLM_MODEL").expect("请设置环境变量 LLM_MODEL");
    let api_key = std::env::var("LLM_API_KEY").expect("请设置环境变量 LLM_API_KEY");

    println!("work_dir: {}", work_dir);

    let llm_provider = OpenaiProvider::new(&base_url, &model, &api_key);
    let provider = Arc::new(Mutex::new(llm_provider));

    let mut registry = ToolRegistry::new();
    registry.register(Arc::new(ReadFileTool::new(&work_dir)));
    registry.register(Arc::new(WritefileTool::new(&work_dir)));
    registry.register(Arc::new(BashTool::new(&work_dir)));
    registry.register(Arc::new(EditFileTool::new(&work_dir)));

    let engine = AgentEngine::new(provider, Arc::new(registry), work_dir, true);

    // let prompt = r#"我当前目录下有 a.txt, b.txt, c.txt 三个文件。
    // 为了节省时间，请你同时一次性读取这三个文件，并将它们的内容综合起来，告诉我它们分别记录了什么领域的信息。"#;

    // let reporter = TerminalReporter::new();
    // engine.run(prompt, &reporter).await?;

    feishu_bot_start(Arc::new(engine)).await;

    Ok(())
}

async fn feishu_bot_start(engine: Arc<AgentEngine>) {
    let app_id = std::env::var("FEISHU_APP_ID").expect("请设置环境变量 FEISHU_APP_ID");
    let app_secret = std::env::var("FEISHU_APP_SECRET").expect("请设置环境变量 FEISHU_APP_SECRET");
    let base_url = std::env::var("FEISHU_BASE_URL").expect("请设置环境变量 FEISHU_BASE_URL");
    let bot = Arc::new(FeishuBot::new(&app_id, &app_secret, &base_url, engine));
    let _ = bot.start_websocket().await;
}
