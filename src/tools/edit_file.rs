use std::path::PathBuf;

use async_trait::async_trait;
use serde::Deserialize;

use crate::{
    error::{AppError, Result},
    schema::ToolDefinition,
    tools::BaseTool,
};

pub struct EditFileTool {
    work_dir: String,
}

impl EditFileTool {
    pub fn new(work_dir: &str) -> Self {
        Self {
            work_dir: work_dir.to_string(),
        }
    }
}

#[derive(Deserialize)]
struct EditFileArgs {
    path: String,
    old_text: String,
    new_text: String,
}

#[async_trait]
impl BaseTool for EditFileTool {
    fn name(&self) -> &str {
        "edit_file"
    }

    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: self.name().to_string(),
            description: "对现有文件进行局部的字符串替换。这比重写整个文件更安全、更快速。请提供足够的old_text 上下文以确保匹配的唯一性。".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "要修改的文件路径",
                    },
                    "old_text": {
                        "type": "string",
                        "description": "文件中原有的文本。必须包含足够的上下文（建议上下各多包含几行）"
                    },
                    "new_text": {
                        "type": "string",
                        "description": "要替换成的新文本"
                    }
                },
                "required": ["path", "old_text", "new_text"]
            }),
        }
    }

    async fn execute(&self, args: serde_json::Value) -> Result<String> {
        let input = serde_json::from_value::<EditFileArgs>(args)
            .map_err(|e| AppError::Generic(format!("参数解析失败: {}", e)))?;

        let fullpath = PathBuf::from(&self.work_dir).join(&input.path);

        let content = tokio::fs::read_to_string(&fullpath)
            .await
            .map_err(|e| AppError::Generic(format!("读取文件失败，请确认路径是否正确: {}", e)))?;

        let new_content = fuzzy_replace(&content, &input.old_text, &input.new_text)?;

        tokio::fs::write(fullpath, new_content)
            .await
            .map_err(|e| AppError::Generic(format!("写回文件失败: {}", e)))?;

        Ok(format!("✅ 成功修改文件: {}", input.path))
    }
}

fn fuzzy_replace(original_content: &str, old_text: &str, new_text: &str) -> Result<String> {
    let count = original_content.matches(old_text).count();
    if count == 1 {
        return Ok(original_content.replacen(old_text, new_text, 1));
    }

    if count > 1 {
        return Err(AppError::Generic(format!(
            "old_text 匹配到了 {} 处，请提供更多的上下文代码以确保唯一性",
            count
        )));
    }

    let normalized_content = original_content.replace("\r\n", "\n");
    let normalized_old = old_text.replace("\r\n", "\n");
    let count = normalized_content.as_str().matches(&normalized_old).count();
    if count == 1 {
        return Ok(normalized_content.replacen(&normalized_old, new_text, 1));
    }

    let trimmed_old = normalized_old.trim();
    if trimmed_old != "" {
        let count = normalized_content.as_str().matches(trimmed_old).count();
        if count == 1 {
            return Ok(normalized_content.replacen(trimmed_old, new_text, 1));
        }
    }

    return line_by_line_replace(&normalized_content, &normalized_old, new_text);
}

fn line_by_line_replace(content: &str, old_text: &str, new_text: &str) -> Result<String> {
    let content_lines: Vec<&str> = content.split('\n').collect();
    let old_lines: Vec<&str> = old_text.trim().lines().map(str::trim).collect();
    let old_lines_len = old_lines.len();

    if old_lines_len == 0 || content_lines.len() < old_lines_len {
        return Err(AppError::Generic("找不到该代码片段".to_string()));
    }

    let mut match_count = 0;
    let mut match_start_index = 0;
    let mut match_end_index = 0;

    for (i, window) in content_lines.windows(old_lines_len).enumerate() {
        // 使用 iter().zip() 将当前窗口的每一行与 old_lines 的每一行一一对应
        let is_match = window
            .iter()
            .zip(old_lines.iter())
            .all(|(c_line, o_line)| c_line.trim() == *o_line);
        if is_match {
            match_count += 1;
            match_start_index = i;
            match_end_index = i + old_lines_len;
        }
    }

    if match_count == 0 {
        return Err(AppError::Generic(
            "在文件中未找到 old_text，请大模型先调用 read_file 仔细确认文件内容和缩进".to_string(),
        ));
    }

    if match_count > 1 {
        return Err(AppError::Generic(format!(
            "模糊匹配到了 {} 处相似代码，请提供更多上下行代码以精确定位",
            match_count
        )));
    }

    let mut result = content_lines[..match_start_index].to_vec();
    result.push(new_text);
    result.extend_from_slice(&content_lines[match_end_index..]);

    Ok(result.join("\n"))
}
