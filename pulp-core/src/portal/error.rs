//! pulp-core 错误模型（对外 API）
//!
//! 设计目标：
//! - 稳定：UI/CLI 只依赖本错误类型，不透传具体 backend 的错误类型。
//! - 可诊断：保留足够上下文（backend 名称、路径、操作阶段等）。
//! - 可本地化：不要在这里硬编码“面向用户的自然语言长段落”；
//!   UI/CLI 做 i18n 时应使用 error 的结构化信息选择文案。

use std::{io, path::PathBuf};

/// core 层统一错误类型。
#[derive(Debug, thiserror::Error)]
pub enum PulpError {
    /// 标准 IO 错误。
    #[error("IO error: {0}")]
    Io(#[from] io::Error),

    /// 不支持的格式或能力。
    #[error("Unsupported format or operation: {0}")]
    Unsupported(String),

    /// 压缩包损坏或无效。
    #[error("Invalid or corrupted archive: {0}")]
    InvalidArchive(String),

    /// 需要密码或密码不正确。
    #[error("Password required or incorrect")]
    PasswordRequired,

    /// 任务被取消。
    #[error("Task cancelled")]
    Cancelled,

    /// 目标文件已存在且禁止覆盖。
    #[error("Target already exists: {0}")]
    AlreadyExists(PathBuf),

    /// 后端内部错误（保留 backend 名称便于诊断）。
    #[error("Backend [{backend}] error: {message}")]
    BackendError { backend: String, message: String },

    /// 路径安全错误（ZipSlip 防护触发）。
    #[error("Unsafe path: {0}")]
    UnsafePath(String),
}

pub type Result<T> = std::result::Result<T, PulpError>;

impl PulpError {
    /// 便捷构造 backend 错误。
    pub fn backend(backend: impl Into<String>, message: impl Into<String>) -> Self {
        PulpError::BackendError {
            backend: backend.into(),
            message: message.into(),
        }
    }

    /// 是否为“取消”语义。
    pub fn is_cancelled(&self) -> bool {
        matches!(self, PulpError::Cancelled)
    }
}
