//! portal：对外能力入口（面向 UI/CLI）。
//!
//! 设计目标：
//! - **稳定入口**：UI/CLI 只依赖本模块，不触碰 backend 细节。
//! - **多任务友好**：进度事件统一携带 `TaskId`。
//! - **可取消**：取消是第一语义，错误模型可区分 Cancelled 与失败。
//! - **安全**：解压路径统一走 pathlib 的安全映射。

pub mod cancel;
pub mod defaults;
pub mod error;
pub mod fs;
pub mod handles;
pub mod progress;
pub mod service;

pub use crate::domain::{
    ArchiveEntry, ArchiveFormat, ArchiveSource, CompressOptions, ExtractOptions, JobRequest,
    JobResult, ListOptions, TaskId, TaskKind, TaskPhase,
};
pub use cancel::CancellationToken;
pub use defaults::create_default_service;
pub use error::{PulpError, Result};
pub use fs::{exists, list_dir, list_dir_recursive, remove_path, rename, create_dir};
pub use handles::{CancelHandle, JobHandle};
pub use progress::{EntryProgress, ProgressEvent, ProgressReporter};
pub use service::{ArchiveService, DefaultArchiveService};
