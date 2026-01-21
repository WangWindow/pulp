//! 常用类型预导出（便于 UI/CLI 引入）。

pub use crate::utils::security::pathlib::{ExtractPathOptions, PathSafetyError, build_output_path};
pub use crate::utils::task_id::next_task_id;
pub use crate::{
    ArchiveEntry, ArchiveFormat, ArchiveService, ArchiveSource, CancellationToken, CompressOptions,
    DefaultArchiveService, ExtractOptions, FileEntry, JobRequest, JobResult, ListOptions,
    ProgressEvent, ProgressReporter, Result, TaskId, TaskKind, TaskPhase, WalkOptions,
    create_default_service, list_dir, list_dir_recursive,
};
