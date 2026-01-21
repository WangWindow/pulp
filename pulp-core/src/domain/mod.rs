//! 领域模型：格式、来源、条目、选项与任务类型。

pub mod archive;
pub mod fs;
pub mod jobs;
pub mod options;
pub mod task;

pub use archive::{ArchiveEntry, ArchiveFormat, ArchiveSource};
pub use fs::{FileEntry, WalkOptions};
pub use jobs::{JobRequest, JobResult};
pub use options::{CompressOptions, ExtractOptions, ListOptions};
pub use task::{TaskId, TaskKind, TaskPhase};
