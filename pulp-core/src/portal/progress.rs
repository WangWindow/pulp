//! 进度模型（pulp-core portal）

pub use crate::domain::{TaskId, TaskKind, TaskPhase};
use std::path::PathBuf;
use std::sync::Arc;

/// 单个条目的进度信息。
#[derive(Debug, Clone)]
pub struct EntryProgress {
    pub path: PathBuf,
    pub name: String,
    pub total_bytes: Option<u64>,
    pub processed_bytes: u64,
    pub is_dir: bool,
}

impl EntryProgress {
    pub fn ratio(&self) -> Option<f32> {
        self.total_bytes.map(|t| {
            if t == 0 {
                0.0
            } else {
                (self.processed_bytes as f32 / t as f32).min(1.0)
            }
        })
    }
}

/// 统一进度事件（多任务）。
#[derive(Debug, Clone)]
pub enum ProgressEvent {
    TaskStarted {
        task_id: TaskId,
        kind: TaskKind,
        title: String,
    },
    BackendSelected {
        task_id: TaskId,
        backend: String,
    },
    PhaseChanged {
        task_id: TaskId,
        phase: TaskPhase,
    },
    EntryStarted {
        task_id: TaskId,
        entry: EntryProgress,
    },
    EntryProgress {
        task_id: TaskId,
        entry: EntryProgress,
    },
    EntryFinished {
        task_id: TaskId,
        entry: EntryProgress,
    },
    Warning {
        task_id: TaskId,
        message: String,
    },
    Note {
        task_id: TaskId,
        message: String,
    },
    TaskFinished {
        task_id: TaskId,
    },
    TaskCancelled {
        task_id: TaskId,
    },
    TaskFailed {
        task_id: TaskId,
        message: String,
    },
}

/// 进度上报接口（由 UI/CLI 实现）。
pub trait ProgressReporter: Send + Sync {
    fn report(&self, event: ProgressEvent);
}

/// 空实现：用于测试或无需进度的场景。
#[derive(Debug, Default, Clone)]
pub struct NoopProgressReporter;

impl ProgressReporter for NoopProgressReporter {
    fn report(&self, _event: ProgressEvent) {}
}

/// 允许直接用闭包作为 reporter。
impl<F> ProgressReporter for F
where
    F: Fn(ProgressEvent) + Send + Sync,
{
    fn report(&self, event: ProgressEvent) {
        (self)(event)
    }
}

/// 共享 reporter 的便捷类型。
pub type SharedReporter = Arc<dyn ProgressReporter>;
