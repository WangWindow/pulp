use crate::{
    domain::{
        ArchiveEntry, ArchiveFormat, ArchiveSource, CompressOptions, ExtractOptions, ListOptions,
    },
    portal::{
        cancel::CancellationToken,
        error::Result,
        progress::{ProgressEvent, ProgressReporter, TaskId, TaskPhase},
    },
};
use async_trait::async_trait;
use std::path::{Path, PathBuf};

/// 任务上下文：贯穿 backend/registry/service。
#[derive(Clone, Copy)]
pub struct TaskContext<'a> {
    pub task_id: TaskId,
    pub progress: &'a dyn ProgressReporter,
    pub cancel: &'a CancellationToken,
}

impl<'a> TaskContext<'a> {
    pub fn new(
        task_id: TaskId,
        progress: &'a dyn ProgressReporter,
        cancel: &'a CancellationToken,
    ) -> Self {
        Self {
            task_id,
            progress,
            cancel,
        }
    }

    /// 便捷：上报阶段变更。
    pub fn phase(&self, phase: TaskPhase) {
        self.progress.report(ProgressEvent::PhaseChanged {
            task_id: self.task_id,
            phase,
        });
    }

    /// 便捷：上报警告。
    pub fn warn(&self, message: impl Into<String>) {
        self.progress.report(ProgressEvent::Warning {
            task_id: self.task_id,
            message: message.into(),
        });
    }

    /// 便捷：上报备注。
    pub fn note(&self, message: impl Into<String>) {
        self.progress.report(ProgressEvent::Note {
            task_id: self.task_id,
            message: message.into(),
        });
    }
}

/// 压缩/解压后端接口。
#[async_trait]
pub trait ArchiveBackend: Send + Sync {
    /// 后端稳定标识。
    fn name(&self) -> &'static str;

    /// 支持的格式集合。
    fn supported_formats(&self) -> &'static [ArchiveFormat];

    /// 是否能处理指定路径（默认用扩展名推断）。
    fn can_handle_path(&self, archive_path: &Path) -> bool {
        ArchiveFormat::from_path(archive_path)
            .is_some_and(|fmt| self.supported_formats().contains(&fmt))
    }

    /// 列出条目（不解压内容）。
    async fn list(
        &self,
        source: &ArchiveSource,
        options: &ListOptions,
        ctx: &TaskContext<'_>,
    ) -> Result<Vec<ArchiveEntry>>;

    /// 解压到目标目录。
    async fn extract(
        &self,
        source: &ArchiveSource,
        dest_dir: &Path,
        options: &ExtractOptions,
        ctx: &TaskContext<'_>,
    ) -> Result<()>;

    /// 压缩：把输入路径打包为目标压缩包。
    async fn compress(
        &self,
        inputs: &[PathBuf],
        dest_archive: &Path,
        format: ArchiveFormat,
        options: &CompressOptions,
        ctx: &TaskContext<'_>,
    ) -> Result<()>;
}
