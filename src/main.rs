use std::{
    env,
    sync::{Arc, Mutex},
};

use anyhow::Result;
use rs_tiny_claw::{
    engine::r#loop::AgentEngine,
    provider::openai::OpenaiProvider,
    tools::{
        Registry, ToolRegistry, bash::BashTool, read_file::ReadFileTool, write_file::WritefileTool,
    },
};

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

    let engine = AgentEngine::new(provider, Arc::new(registry), work_dir, false);

    let prompt = r#"请帮我执行以下操作：
1. 用 bash 查看一下我当前电脑的 Go 版本。
2. 帮我写一个简单的 helloworld.go 文件，输出 "Hello, go-tiny-claw!"。
3. 用 bash 编译并运行这个 go 文件，确认它能正常工作。"#;

    engine.run(prompt).await?;

    Ok(())
}
