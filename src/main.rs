use std::{
    env,
    sync::{Arc, Mutex},
};

use anyhow::Result;
use rs_tiny_claw::{
    engine::r#loop::AgentEngine,
    provider::openai::OpenaiProvider,
    tools::{Registry, ToolRegistry, read_file::ReadFileTool},
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
    let read_file_tool = ReadFileTool::new(&work_dir);
    registry.register(Arc::new(read_file_tool));

    let engine = AgentEngine::new(provider, Arc::new(registry), work_dir, false);

    let prompt =
        "请调用工具读取一下当前工作区目录下 hello.txt 文件的内容，并用一句话向我总结它说了什么。";

    engine.run(prompt).await?;

    Ok(())
}
