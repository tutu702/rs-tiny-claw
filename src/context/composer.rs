use std::{fmt::Write as _, fs, path::PathBuf};

use crate::{
    context::skill::SkillLoader,
    schema::{Message, RoleType},
};

pub struct PromptComposer {
    work_dir: String,
    skill_loader: SkillLoader,
}

const SYSTEM_PROMPT: &str = r"# 核心身份
你名叫 go-tiny-claw，一个由驾驭工程驱动的骨灰级研发助手。
你具备极简主义哲学，拒绝废话。你能通过系统提供的内置工具，创建、读取、修改和执行工作区中的代码。
# 核心纪律 (CRITICAL)
1. 如需检查文件是否存在，请使用 bash 的 ls 或 test -f，而不是对目录使用 read_file。
2. 创建新文件时，务必使用 write_file，并同时提供 path 和 content 参数。
3. 编辑文件前务必先读取现有文件，以理解上下文。
4. 无论何时你需要写代码或创建文件，都要直接使用 write_file 工具。
5. 遇到工具执行报错时，仔细阅读 stderr，尝试自己修正命令并重试。
6. 始终用中文回复，以便传达你的进展和想法。";

impl PromptComposer {
    pub fn new(work_dir: &str) -> Self {
        Self {
            work_dir: work_dir.to_string(),
            skill_loader: SkillLoader::new(work_dir),
        }
    }

    pub fn build(&self) -> Message {
        let mut prompt = String::from(SYSTEM_PROMPT);
        let agents_md_path = PathBuf::from(&self.work_dir).join("AGENTS.md");

        if let Ok(content) = fs::read_to_string(agents_md_path) {
            write!(
                prompt,
               "\n# 项目专属指南 (来自 AGENTS.md)\n以下是当前工作区特有的架构规范与注意事项，你的行为必须绝对符合以下要求：\n```markdown\n{}\n```\n",
                content,
            )
            .unwrap();
        }

        let skills_content = self.skill_loader.load_all();
        if skills_content != "" {
            prompt.push_str(&skills_content);
        }

        Message {
            role: RoleType::System,
            content: prompt,
            tool_calls: None,
            tool_call_id: None,
        }
    }
}
