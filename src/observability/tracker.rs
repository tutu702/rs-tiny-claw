use async_trait::async_trait;
use std::sync::{Arc, LazyLock};
use std::{collections::HashMap, time::Instant};

use crate::{engine::session::Session, error::Result, provider::LlmProvider, schema::Message};

pub static PRICING_MODEL: LazyLock<HashMap<String, ModelPrice>> = LazyLock::new(|| {
    let mut map = HashMap::new();
    map.insert(
        "deepseek-v4-pro".to_string(),
        ModelPrice {
            input_price: 3.025,
            output_price: 6.0,
        },
    );
    map
});

pub struct ModelPrice {
    input_price: f64,
    output_price: f64,
}

pub struct CostTracker {
    next_provider: Box<dyn LlmProvider>,
    model_name: String,
    session: Arc<Session>,
}

impl CostTracker {
    pub fn new(
        next_provider: Box<dyn LlmProvider>,
        model_name: &str,
        session: Arc<Session>,
    ) -> Self {
        Self {
            next_provider,
            model_name: model_name.to_string(),
            session,
        }
    }
}

#[async_trait]
impl LlmProvider for CostTracker {
    async fn generate(
        &mut self,
        messages: &[Message],
        available_tools: Option<Vec<crate::schema::ToolDefinition>>,
    ) -> Result<Message> {
        let start_time = Instant::now();

        let resp_msg = self.next_provider.generate(messages, available_tools).await;

        let latency = start_time.elapsed();

        let message = match resp_msg {
            Ok(v) => v,
            Err(err) => {
                println!("[Tracker] ❌ API 调用失败，耗时: {:?}\n", latency);
                return Err(err);
            }
        };

        let mut cost = 0.0;
        if let Some(ref usage) = message.usage {
            let prompt_tokens = usage.input();
            let completion_tokens = usage.output();

            if let Some(price) = PRICING_MODEL.get(&self.model_name) {
                // 计算花费 = (输入Tokens * 输入单价 + 输出Tokens * 输出单价) / 1000000
                cost = (prompt_tokens as f64 * price.input_price
                    + completion_tokens as f64 * price.output_price)
                    / 1000000.0
            }

            // 5. 打印精美的仪表盘日志
            println!(
                "[Tracker] 📊 API 调用完成 | 耗时: {:?} | 输入: {} tk | 输出: {} tk | 花费: ¥{:.6}",
                latency, prompt_tokens, completion_tokens, cost
            );

            let session = self.session.as_ref();

            session.record_usage(prompt_tokens, completion_tokens, cost);
            println!(
                "[Tracker] 💰 当前会话 ({}) 累计花费: ¥{:.6}\n",
                session.id(),
                session.get_total_cost(),
            );
        } else {
            println!(
                "[Tracker] ⚠️ API 调用完成，但未返回 Usage 数据 | 耗时: {:?}\n",
                latency
            )
        }

        Ok(message)
    }
}
