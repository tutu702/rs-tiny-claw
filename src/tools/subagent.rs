use std::sync::Arc;

use crate::{
    engine::reporter::Reporter,
    error::{AppError, Result},
    schema::ToolDefinition,
    tools::{BaseTool, Registry},
};
use async_trait::async_trait;
use serde::Deserialize;

#[async_trait]
pub trait AgentRunner: Send + Sync {
    async fn run_sub(
        &self,
        task_prompt: &str,
        read_only_registry: &dyn Registry,
        reporter: &dyn Reporter,
    ) -> Result<String>;
}

pub struct SubAgentTool {
    runner: Arc<dyn AgentRunner>,
    read_only_registry: Box<dyn Registry>,
    reporter: Box<dyn Reporter>,
}

impl SubAgentTool {
    pub fn new(
        runner: Arc<dyn AgentRunner>,
        read_only_registry: Box<dyn Registry>,
        reporter: Box<dyn Reporter>,
    ) -> Self {
        Self {
            runner,
            read_only_registry,
            reporter,
        }
    }
}

#[derive(Deserialize)]
struct SubAgentArgs {
    task_prompt: String,
}

#[async_trait]
impl BaseTool for SubAgentTool {
    fn name(&self) -> &str {
        "spawn_subagent"
    }

    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: self.name().to_string(),
            description: "派出一个专门用于深度探索（Exploration）的子智能体。当你需要阅读大量代码、跨文件查找逻辑时请调用此工具。它在探索完毕后，会给你返回一份极度精炼的摘要报告。".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "task_prompt": {
                        "type": "string",
                        "description": "给子智能体下达的明确指令。",
                    }
                },
                "required": ["task_prompt"],
            }),
        }
    }
    async fn execute(&self, args: serde_json::Value) -> Result<String> {
        let input = serde_json::from_value::<SubAgentArgs>(args)
            .map_err(|err| AppError::Generic(format!("参数解析失败: {}", err)))?;

        println!(
            "[Subagent] 🚀 主 Agent 发起委派！正在拉起探路者: [{}]...\n",
            input.task_prompt
        );

        let summary = match self
            .runner
            .run_sub(
                &input.task_prompt,
                &*self.read_only_registry,
                &*self.reporter,
            )
            .await
        {
            Ok(v) => v,
            Err(err) => {
                return Err(AppError::Generic(format!("子智能体执行失败: {}", err)));
            }
        };

        println!("[Subagent] ✅ 子智能体任务结束。报告返回给主干...");

        Ok(format!("【子智能体探索报告】:\n {}", summary))
    }
}
