//! 后端层（backend）：实现 + 路由。
//!
//! - `zip/tar/sevenz` 为纯 Rust 后端。
//! - `rar` 依赖系统 unrar 库，仅提供 list/extract。

pub mod rar;
pub mod registry;
pub mod sevenz;
pub mod tar;
pub mod traits;
pub mod zip;

pub use registry::{BackendRegistry, create_default_registry};
pub use traits::{ArchiveBackend, TaskContext};
