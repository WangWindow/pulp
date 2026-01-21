//! 工具层：提供 core 未覆盖但 UI 必需的能力。
//!
//! 设计原则：
//! - “薄封装”：尽量保持无状态、可测试；
//! - “只做 UI 必需”：复杂业务交给 core；
//! - “统一出口”：外部只使用本模块 re-export 的 API。

pub mod archive;
pub mod format;
pub mod fs;
pub mod icons;
pub mod mounts;
pub mod path_conflict;
pub mod spinner;

pub use format::{archive_stem, format_size, format_time};
pub use icons::icon_handle;
pub use path_conflict::suggest_archive_path;
pub use spinner::SPINNER;
