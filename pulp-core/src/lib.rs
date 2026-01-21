//! pulp-core：现代化压缩/解压核心库（纯 Rust 优先）

pub mod backend;
pub mod domain;
pub mod portal;
pub mod prelude;
pub mod utils;

// -------------------------------
// 稳定 re-export（给 UI/CLI 使用）
// -------------------------------

pub use domain::{
    ArchiveEntry, ArchiveFormat, ArchiveSource, CompressOptions, ExtractOptions, FileEntry,
    ListOptions, WalkOptions,
};
pub use domain::{JobRequest, JobResult};
pub use portal::cancel::CancellationToken;
pub use portal::defaults::create_default_service;
pub use portal::error::{PulpError, Result};
pub use portal::progress::{
    EntryProgress, ProgressEvent, ProgressReporter, TaskId, TaskKind, TaskPhase,
};
pub use portal::service::{ArchiveService, DefaultArchiveService};
pub use portal::{list_dir, list_dir_recursive};
pub use utils::security::pathlib::{ExtractPathOptions, PathSafetyError, build_output_path};
pub use utils::task_id::next_task_id;
