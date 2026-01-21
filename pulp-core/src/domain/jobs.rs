//! 任务请求/结果模型（对外一致）。

use crate::domain::{
    ArchiveEntry, ArchiveFormat, ArchiveSource, CompressOptions, ExtractOptions, ListOptions,
    TaskId,
};

/// Job 请求（统一入口）。
#[derive(Debug, Clone)]
pub enum JobRequest {
    List {
        task_id: TaskId,
        source: ArchiveSource,
        options: ListOptions,
        title: String,
    },
    Extract {
        task_id: TaskId,
        source: ArchiveSource,
        dest_dir: std::path::PathBuf,
        options: ExtractOptions,
        title: String,
    },
    Compress {
        task_id: TaskId,
        inputs: Vec<std::path::PathBuf>,
        dest_archive: std::path::PathBuf,
        format: Option<ArchiveFormat>,
        options: CompressOptions,
        title: String,
    },
}

impl JobRequest {
    pub fn task_id(&self) -> TaskId {
        match self {
            JobRequest::List { task_id, .. } => *task_id,
            JobRequest::Extract { task_id, .. } => *task_id,
            JobRequest::Compress { task_id, .. } => *task_id,
        }
    }
}

/// Job 结果（统一输出）。
#[derive(Debug, Clone)]
pub enum JobResult {
    List { entries: Vec<ArchiveEntry> },
    Extract { dest_dir: std::path::PathBuf },
    Compress { dest_archive: std::path::PathBuf },
}
