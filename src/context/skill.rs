use std::{
    fmt::Write as _,
    fs,
    path::{Path, PathBuf},
};

use walkdir::WalkDir;

const SKILL_FILE_NAME: &str = "SKILL.md";
const SECTION_HEADER: &str = "\n### 可用专业技能 (Agent Skills)\n\
以下是你拥有的标准化外挂技能，请在符合 description 描述的场景下严格遵循其正文指令：\n\n";

pub struct Skill {
    pub name: String,
    pub description: String,
    pub body: String,
}

pub struct SkillLoader {
    pub work_dir: String,
}

impl SkillLoader {
    pub fn new(work_dir: &str) -> Self {
        Self {
            work_dir: work_dir.to_string(),
        }
    }

    /// Loads every `SKILL.md` under `<work_dir>/.claw/skills` and renders
    /// them as a Markdown section. Returns an empty string when the skills
    /// directory is missing or contains no skill files.
    pub fn load_all(&self) -> String {
        let skills_dir = PathBuf::from(&self.work_dir).join(".claw").join("skills");
        if !skills_dir.is_dir() {
            return String::new();
        }

        let mut output = String::from(SECTION_HEADER);
        let mut found = false;

        for entry in WalkDir::new(&skills_dir).into_iter().filter_map(Result::ok) {
            let Some(skill) = read_skill(entry.path()) else {
                continue;
            };
            write!(
                output,
                "#### 技能名称: {}\n**触发条件**: {}\n\n**执行指南**:\n{}\n\n---\n",
                skill.name, skill.description, skill.body,
            )
            .unwrap();
            found = true;
        }

        found.then_some(output).unwrap_or_default()
    }
}

fn read_skill(path: &Path) -> Option<Skill> {
    if path.file_name()?.to_str()? != SKILL_FILE_NAME || !path.is_file() {
        return None;
    }
    fs::read_to_string(path)
        .ok()
        .map(|content| parse_skill_md(&content))
}

fn parse_skill_md(content: &str) -> Skill {
    let mut skill = Skill {
        name: "Unknown Skill".to_string(),
        description: "No description provided.".to_string(),
        body: content.to_string(),
    };

    if !(content.starts_with("---\n") || content.starts_with("---\r\n")) {
        return skill;
    }

    let mut parts = content.splitn(3, "---");
    let (Some(_), Some(frontmatter), Some(body)) = (parts.next(), parts.next(), parts.next())
    else {
        return skill;
    };

    skill.body = body.trim().to_string();
    for line in frontmatter.lines() {
        let line = line.trim();
        if let Some(name) = line.strip_prefix("name:") {
            skill.name = name.trim().to_string();
        } else if let Some(desc) = line.strip_prefix("description:") {
            skill.description = desc.trim().to_string();
        }
    }

    skill
}
