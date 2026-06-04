use crate::{
    error::Result,
    schema::{Message, RoleType},
    tools::{safe_tail, safe_truncate},
};

pub struct Compactor {
    max_chars: usize,
    retain_last_msgs: usize,
}

impl Compactor {
    pub fn new(max_chars: usize, retain_last_msgs: usize) -> Self {
        Self {
            max_chars,
            retain_last_msgs,
        }
    }

    pub fn compact(&self, msgs: &[Message]) -> Result<Vec<Message>> {
        let current_length = estimate_length(msgs);
        if current_length < self.max_chars {
            return Ok(msgs.to_vec());
        }

        println!(
            "[Compactor] ⚠️ 内存告警：当前上下文长度 ({} 字符) 超过阈值 ({})，触发压缩清理...\n",
            current_length, self.max_chars
        );

        let mut compacted: Vec<_> = Vec::new();
        let msg_count = msgs.len();

        let protect_start_index = msg_count.saturating_sub(self.retain_last_msgs);

        for (i, msg) in msgs.iter().enumerate() {
            if msg.role == RoleType::System {
                compacted.push(msg.clone());
                continue;
            }

            let mut new_msg = msg.clone();
            let is_working_memory = i > protect_start_index;
            if msg.role == RoleType::User && msg.tool_call_id.is_some() {
                if !is_working_memory {
                    if msg.content.len() > 200 {
                        new_msg.content = format!(
                            "...[为了节省内存，早期的工具输出已被系统强制清理。原始长度: {} 字节]...",
                            msg.content.len()
                        );
                    }
                } else {
                    const MAX_KEEP: usize = 1000;
                    if msg.content.len() > MAX_KEEP {
                        let head = safe_truncate(&msg.content, 500);
                        let tail = safe_tail(&msg.content, 500);
                        new_msg.content = format!(
                            "{}\n\n...[内容过长，中间 {} 字节已被系统截断]...\n\n{}",
                            head,
                            msg.content.len() - MAX_KEEP,
                            tail
                        );
                    }
                }
            } else if msg.role == RoleType::Assistant && msg.content != "" {
                if !is_working_memory && msg.content.len() > 200 {
                    new_msg.content = "...[早期的推理思考过程已折叠]...".to_string();
                }
            }
            compacted.push(new_msg);
        }

        let new_length = estimate_length(&compacted);
        println!(
            "[compactor] ✅ 压缩完成。上下文长度从 {} 降至 {} 字符。\n",
            current_length, new_length
        );

        Ok(compacted)
    }
}

fn estimate_length(msgs: &[Message]) -> usize {
    msgs.iter()
        .map(|msg| {
            let tc_len = msg
                .tool_calls
                .iter()
                .flatten()
                .map(|tc| tc.name.len() + tc.arguments.to_string().len())
                .sum();
            msg.content.len().saturating_add(tc_len)
        })
        .sum()
}
