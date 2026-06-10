use async_trait::async_trait;
use open_lark::Config;
use open_lark::communication::MessageRecipient;
use open_lark::ws_client::EventDispatcherHandler;
use open_lark::ws_client::LarkWsClient;
use serde::Deserialize;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tokio::sync::mpsc;

use crate::channel::feishu_approval::GLOBAL_APPROVAL_MGR;
use crate::engine::session::Session;
use crate::error::AppError;
use crate::schema;
use crate::{
    engine::{r#loop::AgentEngine, reporter::Reporter},
    error::Result,
};

pub struct FeishuBot {
    pub client: Arc<open_lark::Client>,
    app_id: String,
    app_secret: String,
    base_url: String,
    engine: Arc<AgentEngine>,
    sess: Arc<Session>,
    reporter: Arc<Mutex<Option<FeishuReporter>>>,
}

#[derive(Debug, Deserialize)]
struct EventEnvelope {
    header: EventHeader,
    event: Option<EventBody>,
}

#[derive(Debug, Deserialize)]
struct EventHeader {
    event_id: String,
    event_type: String,
}

#[derive(Debug, Deserialize)]
struct EventBody {
    #[serde(default)]
    chat_id: Option<String>,
    sender: Option<Sender>,
    message: Option<Message>,
}

#[derive(Debug, Deserialize)]
struct Sender {
    sender_id: SenderId,
}

#[derive(Debug, Deserialize)]
struct SenderId {
    open_id: String,
}

#[derive(Debug, Deserialize)]
struct Message {
    message_type: String,
    content: String,
    chat_type: String,
    #[serde(default)]
    chat_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Chat {
    chat_id: String,
}

#[derive(Debug, Deserialize)]
struct TextContent {
    text: String,
}

impl FeishuBot {
    pub fn new(
        app_id: &str,
        app_secret: &str,
        base_url: &str,
        eng: Arc<AgentEngine>,
        sess: Arc<Session>,
        reporter: Arc<Mutex<Option<FeishuReporter>>>,
    ) -> Self {
        let client = Arc::new(
            open_lark::Client::builder()
                .app_id(app_id)
                .app_secret(app_secret)
                .build()
                .expect("客户端初始化失败"),
        );
        Self {
            client: Arc::clone(&client),
            app_id: app_id.to_string(),
            app_secret: app_secret.to_string(),
            base_url: base_url.to_string(),
            engine: eng,
            sess,
            reporter,
        }
    }

    pub async fn start_websocket(self: Arc<Self>) -> Result<()> {
        let ws_config = Config::builder()
            .app_id(self.app_id.to_string())
            .app_secret(self.app_secret.to_string())
            .base_url(self.base_url.to_string())
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|e| AppError::Generic(format!("构建 WebSocket 配置失败: {}", e)))?;

        let (payload_tx, payload_rx) = mpsc::unbounded_channel::<Vec<u8>>();

        let me = Arc::clone(&self);
        tokio::spawn(async move { me.process_payload_loop(payload_rx).await });

        let event_handler = EventDispatcherHandler::builder()
            .payload_sender(payload_tx)
            .build();

        println!("✅ WebSocket 客户端已创建，正在连接飞书服务器...");
        LarkWsClient::open(Arc::new(ws_config), event_handler)
            .await
            .map_err(|e| AppError::Generic(format!("WebSocket 连接失败: {}", e)))?;
        Ok(())
    }

    pub async fn process_payload_loop(&self, mut payload_rx: mpsc::UnboundedReceiver<Vec<u8>>) {
        while let Some(payload) = payload_rx.recv().await {
            if let Err(err) = self.handle_payload(&payload).await {
                eprintln!("❌ 处理事件失败: {err}");
            }
        }
    }

    async fn handle_payload(
        &self,
        payload: &[u8],
    ) -> std::result::Result<(), Box<dyn std::error::Error>> {
        let envelope: EventEnvelope = match serde_json::from_slice(payload) {
            Ok(v) => v,
            Err(err) => {
                eprintln!("⚠️ 忽略无法解析的事件载荷: {err}");
                return Ok(());
            }
        };

        match envelope.header.event_type.as_str() {
            "im.message.receive_v1" => {
                self.on_message_received(envelope.event).await;
            }
            "im.chat.access_event.bot_p2p_chat_entered_v1" => {
                self.on_p2p_chat_entered(envelope.event).await;
            }
            "im.message.message_read_v1" => {
                // read 事件无需处理
            }
            _ => {}
        }
        Ok(())
    }

    async fn on_message_received(&self, event: Option<EventBody>) {
        let Some(event) = event else {
            eprintln!("⚠️ 事件为 None");
            return;
        };
        let Some(msg) = event.message else {
            eprintln!("⚠️ 事件消息为 None");
            return;
        };
        if msg.message_type != "text" {
            println!("ℹ️ 跳过非文本消息: {}", msg.message_type);
            return;
        }

        let text = match extract_text(&msg.content) {
            Ok(t) => t,
            Err(err) => {
                eprintln!("⚠️ 解析文本消息 content 失败: {err}");
                return;
            }
        };
        if text.trim().is_empty() {
            println!("ℹ️ 跳过空文本消息");
            return;
        }

        let chat_id = msg.chat_id.unwrap_or_default();
        println!("[Feishu] 收到会话 {} 消息: {}", chat_id, text);

        // 【新增】：拦截人工审批的特殊口令
        if text.starts_with("approve ") {
            let task_id = text.trim_start_matches("approve ");
            let _ = GLOBAL_APPROVAL_MGR.resolve_approval(task_id, true, "人类管理员已批准操作");
            return;
        }

        if text.starts_with("reject ") {
            let task_id = text.trim_start_matches("reject ");
            let _ = GLOBAL_APPROVAL_MGR.resolve_approval(
                task_id,
                false,
                "人类管理员认为该操作存在极高风险，已无情拒绝",
            );

            println!("[Feishu] 会话 {}: 🚫 已拒绝任务 {}", chat_id, task_id);
            return;
        }

        let engine = Arc::clone(&self.engine);
        let client = Arc::clone(&self.client);
        let sess = Arc::clone(&self.sess);
        let reporter = FeishuReporter::new(&chat_id, client);
        *self.reporter.lock().await = Some(reporter.clone());
        tokio::spawn(async move {
            let _ = sess.append(&[schema::Message::user(&text, None)]);
            if let Err(err) = engine.run(sess, &reporter).await {
                let _ = reporter
                    .send_msg(&format!("❌ Agent 运行崩溃: {}", err))
                    .await;
            }
        });
    }

    async fn on_p2p_chat_entered(&self, event: Option<EventBody>) {
        let event = match event {
            Some(v) => v,
            None => return,
        };
        if let Some(chat_id) = event.chat_id {
            println!("[Feishu] 用户进入私聊 {}", chat_id);
            let reporter = FeishuReporter::new(&chat_id, Arc::clone(&self.client));
            let _ = reporter
                .send_msg("👋 你好！我是 Go Tiny Claw AI 助手，有什么可以帮你的吗？")
                .await;
        }
    }

    pub async fn reporter(&self) -> Option<FeishuReporter> {
        let r = self.reporter.lock().await;
        r.clone()
    }
}

#[derive(Clone)]
pub struct FeishuReporter {
    pub client: Arc<open_lark::Client>,
    pub chat_id: String,
}

impl FeishuReporter {
    pub fn new(chat_id: &str, client: Arc<open_lark::Client>) -> Self {
        Self {
            client: client,
            chat_id: chat_id.to_string(),
        }
    }

    pub async fn send_msg(&self, text: &str) -> Result<()> {
        self.client
            .communication
            .im
            .send_text(MessageRecipient::chat_id(&self.chat_id), text)
            .await
            .map_err(|e| AppError::Generic(format!("发送消息失败: {}", e)))?;
        Ok(())
    }
}

#[async_trait]
impl Reporter for FeishuReporter {
    async fn on_thinking(&self) {
        let _ = self.send_msg("🤔 模型正在慢思考 (Thinking)...").await;
    }

    async fn on_tool_call(&self, tool_name: &str, args: &str) {
        let _ = self
            .send_msg(&format!(
                "🛠️ **正在执行工具**：`{}`\n参数：`{}`",
                tool_name, args
            ))
            .await;
    }

    async fn on_tool_result(&self, tool_name: &str, result: &str, is_error: bool) {
        if is_error {
            let _ = self
                .send_msg(&format!("⚠️ **执行报错** ({})：\n{}", tool_name, result))
                .await;
        } else {
            let _ = self
                .send_msg(&format!("✅ **执行成功** ({})", tool_name))
                .await;
        }
    }

    async fn on_message(&self, content: &str) {
        let _ = self.send_msg(content).await;
    }
}

fn extract_text(content: &str) -> Result<String> {
    let content_json: TextContent = serde_json::from_str(content)
        .map_err(|e| AppError::Generic(format!("解析文本消息 content 失败: {}", e)))?;
    Ok(content_json.text)
}
