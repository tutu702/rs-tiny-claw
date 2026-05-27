use std::path::PathBuf;

use crate::{
    error::{AppError, Result},
    schema::ToolDefinition,
    tools::BaseTool,
};
use async_trait::async_trait;
use serde::Deserialize;

const MAX_CONTENT_LENGTH: usize = 8000;

pub struct ReadFileTool {
    work_dir: String,
}

impl ReadFileTool {
    pub fn new(work_dir: &str) -> Self {
        Self {
            work_dir: work_dir.to_string(),
        }
    }
}

#[derive(Deserialize)]
struct ReadFileArgs {
    path: String,
}

#[async_trait]
impl BaseTool for ReadFileTool {
    fn name(&self) -> &str {
        "read_file"
    }

    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: self.name().into(),
            description: "读取指定路径的文件内容。请提供相对工作区的路径。".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "要读取的文件路径，如 cmd/claw/main.go",
                    },
                },
                "required": ["path"],
            }),
        }
    }
    async fn execute(&self, args: serde_json::Value) -> Result<String> {
        let input = serde_json::from_value::<ReadFileArgs>(args)
            .map_err(|e| AppError::Generic(format!("参数解析失败: {}", e)))?;

        let fullpath = PathBuf::from(&self.work_dir).join(&input.path);
        println!("[read_file] path: {}", fullpath.display());

        let content = tokio::fs::read_to_string(&fullpath)
            .await
            .map_err(|e| AppError::Generic(format!("读取文件内容失败: {}", e)))?;

        if content.len() > MAX_CONTENT_LENGTH {
            return Ok(format!(
                "{}\n\n...[内容已截断至前 {} 字节]...",
                &content[..MAX_CONTENT_LENGTH],
                MAX_CONTENT_LENGTH
            ));
        }

        Ok(content)
    }
}
