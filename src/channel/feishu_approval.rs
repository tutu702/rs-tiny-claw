use regex::Regex;
use std::{
    collections::HashMap,
    sync::{LazyLock, RwLock},
};
use tokio::sync::oneshot;

use crate::{
    channel::feishu_bot::FeishuReporter,
    engine::reporter::Reporter,
    error::{AppError, Result},
};

#[derive(Debug, Clone)]
pub struct ApprovalResult {
    allowed: bool,
    reason: String,
}

/// 全局单例
pub static GLOBAL_APPROVAL_MGR: LazyLock<ApprovalManager> =
    LazyLock::new(|| ApprovalManager::new());

pub struct ApprovalManager {
    pending_tasks: RwLock<HashMap<String, oneshot::Sender<ApprovalResult>>>,
}

impl ApprovalManager {
    pub fn new() -> Self {
        Self {
            pending_tasks: RwLock::new(HashMap::new()),
        }
    }

    pub async fn wait_for_approval(
        &self,
        task_id: &str,
        tool_name: &str,
        args: &str,
        reporter: Option<FeishuReporter>,
    ) -> Result<(bool, String)> {
        let (tx, rx) = oneshot::channel::<ApprovalResult>();

        {
            let mut task = self
                .pending_tasks
                .write()
                .map_err(|e| AppError::Generic(e.to_string()))?;
            task.insert(task_id.to_string(), tx);
        }

        let notice_msg = format!(
            "⚠️ **高危操作审批请求**
Agent 试图执行以下动作:
  - 工具: {}
  - 参数: {}
任务 ID: **{}**

👉 请在此消息下方回复 \"approve {}\" 或 \"reject {}\" 来决定是否放行。",
            tool_name, args, task_id, task_id, task_id
        );

        if let Some(reporter) = reporter {
            reporter.send_msg(&notice_msg).await?;
        } else {
            println!(
                "\n\033[31m[需要审批 TaskID: {}]\033[0m {}",
                task_id, notice_msg
            )
        }

        println!(
            "[Approval] 已发送审批请求 (TaskID: {})，协程挂起等待...",
            task_id
        );

        let result = match rx.await {
            Ok(res) => res,
            Err(_) => ApprovalResult {
                allowed: false,
                reason: "审批通道已关闭".to_string(),
            },
        };

        {
            let mut task = self
                .pending_tasks
                .write()
                .map_err(|e| AppError::Generic(e.to_string()))?;
            task.remove(task_id);
        }

        Ok((result.allowed, result.reason))
    }

    pub fn resolve_approval(&self, task_id: &str, allowed: bool, reason: &str) -> Result<()> {
        let mut tasks = self
            .pending_tasks
            .write()
            .map_err(|e| AppError::Generic(e.to_string()))?;

        if let Some(tx) = tasks.remove(task_id) {
            println!(
                "[Approval] 收到来自飞书的审批结果 (TaskID: {}, Allowed: {})",
                task_id, allowed
            );

            let _ = tx.send(ApprovalResult {
                allowed,
                reason: reason.to_string(),
            });
        } else {
            println!(
                "[Approval] 找不到对应的 TaskID: {}，可能已超时或处理完毕",
                task_id
            );
        }

        Ok(())
    }
}

pub fn is_dangerous_command(tool_name: &str, args: &str) -> bool {
    if tool_name != "bash" && tool_name != "write_file" && tool_name != "edit_file" {
        return false;
    }

    if tool_name == "bash" {
        let dangerous_patterns = vec![
            r"rm\s+-r", // 级联删除
            r"sudo\s+", // 提权
            r"drop\s+", // 数据库删除
            r">.*\.go", // 恶意覆盖源代码
        ];
        for pattern in dangerous_patterns {
            if let Ok(re) = Regex::new(pattern) {
                if re.is_match(args) {
                    return true;
                }
            }
        }
    }

    false
}
