use std::path::PathBuf;

use async_trait::async_trait;
use serde::Deserialize;

use crate::{
    error::{AppError, Result},
    schema::ToolDefinition,
    tools::BaseTool,
};

pub struct WriteFileTool {
    work_dir: String,
}

impl WriteFileTool {
    pub fn new(work_dir: &str) -> Self {
        Self {
            work_dir: work_dir.to_string(),
        }
    }
}

#[derive(Deserialize)]
struct WriteFileArgs {
    path: String,
    content: String,
}

#[async_trait]
impl BaseTool for WriteFileTool {
    fn name(&self) -> &str {
        "write_file"
    }

    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: self.name().to_string(),
            description:
                "创建或覆盖写入一个文件。如果目录不存在会自动创建。请提供相对于工作区的相对路径。"
                    .to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "要写入的文件路径，如 src/main.rs",
                    },
                    "content": {
                        "type": "string",
                        "description": "要写入的完整文件内容",
                    }
                },
                "required": ["path", "content"],
            }),
        }
    }

    async fn execute(&self, args: serde_json::Value) -> Result<String> {
        let input = serde_json::from_value::<WriteFileArgs>(args)
            .map_err(|e| AppError::Generic(format!("参数解析失败: {}", e)))?;

        let fullpath = PathBuf::from(&self.work_dir).join(&input.path);

        if let Some(parent) = fullpath.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| AppError::Generic(format!("创建父目录失败: {}", e)))?;
        }

        tokio::fs::write(&fullpath, input.content)
            .await
            .map_err(|e| AppError::Generic(format!("写入文件失败: {}", e)))?;

        Ok(format!("成功将内容写入到文件: {}", input.path))
    }
}
