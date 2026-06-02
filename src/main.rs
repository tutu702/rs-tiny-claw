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
        .map(|p| p.join("workspace").to_string_lossy().to_string())
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

    let engine = AgentEngine::new(provider, Arc::new(registry), &work_dir, true);

    cli_start(Arc::new(engine)).await;

    // feishu_bot_start(Arc::new(engine)).await;

    Ok(())
}

async fn cli_start(engine: Arc<AgentEngine>) {
    let reporter = TerminalReporter::new();
    let prompt = r#"我需要在当前目录下新建一个 ping.go，提供一个简单的 http ping 接口。 写完之后，帮我把代码用 git 提交一下。"#;
    engine.run(prompt, &reporter).await.unwrap();
}

async fn feishu_bot_start(engine: Arc<AgentEngine>) {
    let app_id = std::env::var("FEISHU_APP_ID").expect("请设置环境变量 FEISHU_APP_ID");
    let app_secret = std::env::var("FEISHU_APP_SECRET").expect("请设置环境变量 FEISHU_APP_SECRET");
    let base_url = std::env::var("FEISHU_BASE_URL").expect("请设置环境变量 FEISHU_BASE_URL");
    let bot = Arc::new(FeishuBot::new(&app_id, &app_secret, &base_url, engine));
    let _ = bot.start_websocket().await;
}
