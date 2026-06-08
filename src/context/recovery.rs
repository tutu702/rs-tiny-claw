use crate::error::Result;

pub struct RecoveryManager {}

impl RecoveryManager {
    pub fn new() -> Self {
        Self {}
    }

    pub fn analyze_and_inject(&self, tool_name: &str, raw_error: &str) -> Result<String> {
        let mut hint = String::new();
        let lower_error = raw_error.to_lowercase();

        match tool_name {
            "edit_file" => {
                if raw_error.contains("在文件中未找到 old_text")
                    || raw_error.contains("找不到该代码片段")
                {
                    hint.push_str("你提供的 old_text 与文件当前内容不一致，或者缺少必要的缩进。请先使用 `read_file` 工具重新读取该文件，获取最新、准确的内容后，再重新发起编辑。");
                } else if raw_error.contains("匹配到了多处") || raw_error.contains("提供更多上下文")
                {
                    hint.push_str("你的 old_text 不够具体，命中了多个相同代码块。请在 old_text 中增加上下相邻的几行代码，以确保替换的唯一性。");
                }
            }
            "read_file" | "write_file" => {
                if lower_error.contains("no such file or directory") {
                    hint.push_str("路径似乎不正确。请不要凭空猜测，先使用 `bash` 执行 `ls -la` 或 `find . -name` 命令查找正确的目录结构和文件名。");
                } else if lower_error.contains("permission denied") {
                    hint.push_str(
                        "你没有权限操作该文件。请检查工作区限制，或者思考是否需要修改其他文件。",
                    );
                }
            }
            "bash" => {
                if lower_error.contains("command not found") {
                    hint.push_str("系统中未安装该命令。请先思考：是否有替代命令？或者你需要先编写脚本进行安装？");
                } else if raw_error.contains("超时") || raw_error.contains("DeadlineExceeded") {
                    // 匹配我们手写的 30s context.WithTimeout 报错
                    hint.push_str("该命令执行被超时强杀。如果它是一个常驻服务（如 server 或 watch），请将其转入后台执行（例如使用 `nohup ... &`），不要阻塞主线程。");
                } else if lower_error.contains("syntax error") {
                    hint.push_str(
                        "Bash 语法错误。请检查引号转义或特殊字符，确保命令在终端中可直接运行。",
                    );
                }
            }
            _ => {}
        }

        if hint.is_empty() {
            return Ok(raw_error.to_string());
        }

        Ok(hint)
    }
}
