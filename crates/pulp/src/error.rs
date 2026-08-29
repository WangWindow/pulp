use std::io;

use thiserror::Error;

use crate::entry::EntryNameError;
use crate::limits::ResourceLimitKind;
use crate::operation::OperationKind;

/// The result type shared by all Pulp core operations.
pub type ArchiveResult<T> = Result<T, ArchiveError>;

/// A structured failure from an archive operation.
#[derive(Debug, Error)]
pub enum ArchiveError {
    /// The caller supplied an invalid operation argument.
    #[error("invalid input: {0}")]
    InvalidInput(String),
    /// No loaded handler can process the requested format.
    #[error("unsupported format: {0}")]
    UnsupportedFormat(String),
    /// The selected handler does not expose this operation.
    #[error("unsupported operation {operation} (format: {format:?})")]
    UnsupportedOperation {
        /// Operation that was requested.
        operation: OperationKind,
        /// Optional format context.
        format: Option<String>,
    },
    /// An archive requires a password and no provider returned one.
    #[error("password required")]
    PasswordRequired,
    /// The supplied password was rejected by the archive.
    #[error("wrong password")]
    WrongPassword,
    /// A split archive volume could not be opened.
    #[error("archive volume not found: {0}")]
    VolumeNotFound(String),
    /// Archive bytes or structure are invalid.
    #[error("archive data error: {0}")]
    DataError(String),
    /// An operation exceeded an explicit safety limit.
    #[error("resource limit {kind}: {message}")]
    ResourceLimit {
        /// Limit category.
        kind: ResourceLimitKind,
        /// Human-readable detail.
        message: String,
    },
    /// An archive name or filesystem policy was violated.
    #[error("path policy violation: {0}")]
    PathViolation(String),
    /// A caller-selected safety policy rejected an otherwise valid operation.
    #[error("policy violation: {0}")]
    PolicyViolation(String),
    /// The selected adapter cannot provide a requested operation.
    #[error("unsupported: {0}")]
    Unsupported(String),
    /// A supporting adapter failed outside the archive algorithm itself.
    #[error("backend error ({component}): {message}")]
    Backend {
        /// Adapter or subsystem name.
        component: String,
        /// Bounded diagnostic.
        message: String,
    },
    /// A native provider returned an error status.
    #[error("native error {status}: {message}")]
    Native {
        /// Provider status or HRESULT converted to a signed integer.
        status: i32,
        /// Bounded provider diagnostic.
        message: String,
    },
    /// The operation was cancelled by the caller.
    #[error("operation cancelled")]
    Cancelled,
    /// An invariant inside the adapter was violated.
    #[error("internal error: {0}")]
    Internal(String),
    /// A local I/O operation failed.
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),
}

impl From<EntryNameError> for ArchiveError {
    fn from(error: EntryNameError) -> Self {
        Self::PathViolation(error.to_string())
    }
}

impl ArchiveError {
    /// Creates an invalid-input error without requiring a concrete error type.
    pub fn invalid_input(message: impl Into<String>) -> Self {
        Self::InvalidInput(message.into())
    }

    /// Creates a native error with a bounded diagnostic.
    pub fn native(status: i32, message: impl Into<String>) -> Self {
        let mut message = message.into();
        message.truncate(4096);
        Self::Native { status, message }
    }

    /// Creates a generic unsupported-operation error.
    pub fn unsupported(message: impl Into<String>) -> Self {
        Self::Unsupported(message.into())
    }

    /// Creates an adapter error with a bounded diagnostic.
    pub fn backend(component: impl Into<String>, message: impl Into<String>) -> Self {
        let mut message = message.into();
        message.truncate(4096);
        Self::Backend {
            component: component.into(),
            message,
        }
    }
}
