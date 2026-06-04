use async_trait::async_trait;
use serde::Deserialize;
use tokio::process::Command;

use crate::{
    error::{AppError, Result},
    schema::ToolDefinition,
    tools::{BaseTool, MAX_CONTENT_LENGTH, safe_truncate},
};

pub struct BashTool {
    work_dir: String,
}

impl BashTool {
    pub fn new(work_dir: &str) -> Self {
        Self {
            work_dir: work_dir.to_string(),
        }
    }
}

#[derive(Deserialize)]
struct BashArgs {
    command: String,
}

#[async_trait]
impl BaseTool for BashTool {
    fn name(&self) -> &str {
        "bash"
    }

    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: self.name().to_string(),
            description: "在当前工作区执行任意的 bash 命令。 支持链式命令(如 &&)。返回标准输出(stdout)和标准错误(stderr)。".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "command": {
                        "type": "string",
                        "description": "要执行的 bash 命令，例如：ls -la 或 cargo test"
                    }
                },
                "required": ["command"],
            }),
        }
    }

    async fn execute(&self, args: serde_json::Value) -> Result<String> {
        let input = serde_json::from_value::<BashArgs>(args)
            .map_err(|e| AppError::Generic(format!("参数解析失败: {}", e)))?;

        let child = Command::new("bash")
            .arg("-c")
            .arg(input.command)
            .current_dir(&self.work_dir)
            .stdout(std::process::Stdio::piped())
            .stdin(std::process::Stdio::piped())
            .spawn()
            .map_err(|e| AppError::Generic(format!("创建 Command 失败: {}", e)))?;

        let timeout_duration = std::time::Duration::from_secs(30);

        match tokio::time::timeout(timeout_duration, child.wait_with_output()).await {
            Ok(Ok(output)) => {
                if output.status.success() {
                    let mut res = String::from_utf8_lossy(&output.stdout).into_owned();
                    if res.len() > MAX_CONTENT_LENGTH {
                        res = format!(
                            "{}\n\n...[终端输出过长，已截断至前 {} 字节]...",
                            safe_truncate(&res, MAX_CONTENT_LENGTH),
                            MAX_CONTENT_LENGTH
                        )
                    }
                    Ok(res)
                } else {
                    let res = String::from_utf8_lossy(&output.stderr);
                    let code = output
                        .status
                        .code()
                        .map_or("未知".to_string(), |c| c.to_string());
                    Ok(format!("执行报错: {}\n输出:\n{}", code, res))
                }
            }
            Ok(Err(err)) => {
                // 进程自身执行出错（例如命令没找到等，非超时引起）
                Err(AppError::Generic(format!("执行报错: {}", err)))
            }
            Err(_) => Err(AppError::Generic(format!(
                "[警告: 命令执行超时(30s)，已被系统强制终止。如果是启动常驻服务，请尝试将其转入后台。]",
            ))),
        }
    }
}
