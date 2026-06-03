use std::{
    collections::HashMap,
    fmt,
    sync::{Arc, LazyLock, RwLock},
    time::SystemTime,
};

use crate::{
    error::{AppError, Result},
    schema::{Message, RoleType},
};

pub struct Session {
    id: String,
    work_dir: String,
    history: RwLock<Vec<Message>>,
    created_at: SystemTime,
    updated_at: RwLock<SystemTime>,
}

impl Session {
    pub fn new(id: &str, work_dir: &str) -> Self {
        let now = SystemTime::now();
        Self {
            id: id.to_string(),
            work_dir: work_dir.to_string(),
            history: RwLock::new(Vec::new()),
            created_at: now,
            updated_at: RwLock::new(now),
        }
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn work_dir(&self) -> &str {
        &self.work_dir
    }

    pub fn append(&self, msgs: &[Message]) -> Result<()> {
        let mut history = self
            .history
            .write()
            .map_err(|e| AppError::Generic(e.to_string()))?;
        history.extend_from_slice(msgs);

        let mut updated_at = self
            .updated_at
            .write()
            .map_err(|e| AppError::Generic(e.to_string()))?;
        *updated_at = SystemTime::now();
        Ok(())
    }

    pub fn get_working_memory(&self, limit: usize) -> Result<Vec<Message>> {
        let history = self
            .history
            .read()
            .map_err(|e| AppError::Generic(e.to_string()))?;

        let total = history.len();
        let start = if limit == 0 || total <= limit {
            0
        } else {
            total - limit
        };

        Ok(history[start..]
            .iter()
            .skip_while(|m| m.role == RoleType::User && m.tool_call_id.is_some())
            .cloned()
            .collect())
    }
}

impl fmt::Debug for Session {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Session")
            .field("id", &self.id)
            .field("work_dir", &self.work_dir)
            .field("created_at", &self.created_at)
            .finish_non_exhaustive()
    }
}

/// 全局单例 SessionManager
pub static GLOBAL_SESSION_MGR: LazyLock<SessionManager> = LazyLock::new(|| SessionManager::new());

pub struct SessionManager {
    sessions: RwLock<HashMap<String, Arc<Session>>>,
}

impl SessionManager {
    pub fn new() -> Self {
        Self {
            sessions: RwLock::new(HashMap::new()),
        }
    }

    pub fn get_or_create(&self, id: &str, work_dir: &str) -> Result<Arc<Session>> {
        let mut sessions = self
            .sessions
            .write()
            .map_err(|e| AppError::Generic(e.to_string()))?;
        Ok(sessions
            .entry(id.to_string())
            .or_insert_with(|| Arc::new(Session::new(id, work_dir)))
            .clone())
    }
}

impl Default for SessionManager {
    fn default() -> Self {
        Self::new()
    }
}
