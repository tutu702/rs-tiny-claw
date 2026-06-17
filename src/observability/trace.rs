use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Instant,
};

use chrono::{DateTime, Utc};
use serde::Serialize;

use crate::error::{AppError, Result};

#[derive(Clone)]
pub struct Span {
    inner: Arc<Mutex<SpanInner>>,
}

impl Span {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            inner: Arc::new(Mutex::new(SpanInner::new(name))),
        }
    }

    pub fn start_child(&self, name: impl Into<String>) -> Self {
        let child = Self::new(name);
        if let Ok(mut parent) = self.inner.lock() {
            parent.children.push(Arc::clone(&child.inner));
        }
        child
    }

    pub fn end(&self) {
        if let Ok(mut span) = self.inner.lock() {
            let elapsed_ms = span.start_instant.elapsed().as_millis() as i64;
            span.end_time = Some(span.start_time + chrono::Duration::milliseconds(elapsed_ms));
            span.duration_ms = Some(elapsed_ms);
        }
    }

    pub fn add_attribute<V: Into<serde_json::Value>>(&self, key: impl Into<String>, value: V) {
        if let Ok(mut span) = self.inner.lock() {
            span.attributes.insert(key.into(), value.into());
        }
    }

    pub fn inner(&self) -> Arc<Mutex<SpanInner>> {
        Arc::clone(&self.inner)
    }
}

#[derive(Debug)]
pub struct SpanInner {
    pub name: String,
    pub start_time: DateTime<Utc>,
    pub end_time: Option<DateTime<Utc>>,
    pub duration_ms: Option<i64>,
    pub attributes: HashMap<String, serde_json::Value>,
    pub children: Vec<Arc<Mutex<SpanInner>>>,
    start_instant: Instant,
}

impl SpanInner {
    fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            start_time: Utc::now(),
            end_time: None,
            duration_ms: None,
            attributes: HashMap::new(),
            children: Vec::new(),
            start_instant: Instant::now(),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct SpanSnapshot {
    pub name: String,
    pub start_time: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_time: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<i64>,
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    pub attributes: HashMap<String, serde_json::Value>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<SpanSnapshot>,
}

impl SpanSnapshot {
    pub fn from_arc_mutex(span: &Arc<Mutex<SpanInner>>) -> Self {
        let guard = span.lock().expect("SpanInner mutex poisoned");
        Self::from_inner(&guard)
    }

    fn from_inner(inner: &SpanInner) -> Self {
        Self {
            name: inner.name.clone(),
            start_time: inner.start_time,
            end_time: inner.end_time,
            duration_ms: inner.duration_ms,
            attributes: inner.attributes.clone(),
            children: inner.children.iter().map(Self::from_arc_mutex).collect(),
        }
    }
}

pub fn export_trace_to_file(
    root_span: &Arc<Mutex<SpanInner>>,
    work_dir: impl AsRef<Path>,
    session_id: &str,
) -> Result<()> {
    let trace_dir: PathBuf = work_dir.as_ref().join(".claw").join("traces");
    fs::create_dir_all(&trace_dir)
        .map_err(|e| AppError::Generic(format!("create trace dir failed: {e}")))?;

    let file_path = trace_dir.join(format!(
        "trace_{}_{}.json",
        session_id,
        Utc::now().timestamp()
    ));

    let snapshot = SpanSnapshot::from_arc_mutex(root_span);
    let contents = serde_json::to_string_pretty(&snapshot)
        .map_err(|e| AppError::Generic(format!("serialize trace failed: {e}")))?;

    fs::write(&file_path, contents)
        .map_err(|e| AppError::Generic(format!("write trace file failed: {e}")))?;

    Ok(())
}

/// RAII 守卫:在 drop 时自动结束根 span 并导出 trace 报告。
pub struct TraceGuard {
    root_span: Span,
    work_dir: String,
    session_id: String,
}

impl TraceGuard {
    pub fn new(
        root_span: Span,
        work_dir: impl Into<String>,
        session_id: impl Into<String>,
    ) -> Self {
        Self {
            root_span,
            work_dir: work_dir.into(),
            session_id: session_id.into(),
        }
    }
}

impl Drop for TraceGuard {
    fn drop(&mut self) {
        self.root_span.end();

        let span_arc = self.root_span.inner();
        match export_trace_to_file(&span_arc, &self.work_dir, &self.session_id) {
            Ok(()) => {
                println!("📊 [Tracing] 本次任务的执行回放链路已保存至工作区的 .claw/traces 目录下")
            }
            Err(e) => eprintln!("[Tracing] 导出 trace 失败: {e}"),
        }
    }
}
