use md5::{Digest, Md5};

use crate::schema::{Message, ToolCall, ToolResult};
use std::collections::HashMap;

const NUDGE_THRESHOLD: u8 = 3;

#[derive(Default)]
pub struct ReminderInjector {
    consecutive_failures: HashMap<String, u8>,
}

impl ReminderInjector {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn check_and_inject(
        &mut self,
        last_tool_call: &ToolCall,
        last_result: &ToolResult,
    ) -> Option<Message> {
        if !last_result.is_error {
            self.consecutive_failures = HashMap::new();
            return None;
        }

        let finger_print =
            generate_finger_print(&last_tool_call.name, &last_tool_call.arguments.to_string());

        let fail_count = {
            let counter = self
                .consecutive_failures
                .entry(finger_print.clone())
                .or_insert(0);
            *counter = counter.saturating_add(1);
            *counter
        };

        println!(
            "[Reminder] 监控到工具 {} 执行失败，该参数特征连续失败次数: {}, hash: {}\n",
            last_tool_call.name, fail_count, finger_print
        );

        (fail_count > NUDGE_THRESHOLD).then(|| build_nudge(last_tool_call, fail_count))
    }
}

fn generate_finger_print(tool_name: &str, args: &str) -> String {
    let mut hasher = Md5::new();
    hasher.update(tool_name.as_bytes());
    hasher.update(args.as_bytes());

    let result = hasher.finalize();
    hex::encode(result)
}

fn build_nudge(tool_call: &ToolCall, fail_count: u8) -> Message {
    let nudge_msg = format!(
        "[SYSTEM REMINDER 警告]\n\
         你似乎陷入了死循环。你刚刚连续 {fail_count} 次使用相同的参数调用了 '{name}' 工具，并且都失败了。\n\
         请立即停止这种无效的重试！你的注意力被当前的报错过度吸引了。\n\
         你需要：\n\
         1. 停止猜测参数。跳出当前的局部思维。\n\
         2. 彻底改变你的策略。\n\
         3. 如果你确实无法通过系统工具解决当前问题，请直接结束任务并向用户说明你需要什么人工帮助，\
         而不是继续盲目消耗 API 资源尝试。",
        name = tool_call.name,
        fail_count = fail_count,
    );

    println!("[Reminder] ⚠️ 触发死循环干预！注入强力修正指令。");
    Message::user(&nudge_msg, None)
}
