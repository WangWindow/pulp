//! Errors reported by the statically linked Format7zF provider.

use crate::ArchiveError;
use thiserror::Error;

/// A loading, metadata, bridge, or archive-operation failure.
#[derive(Debug, Error)]
pub enum Format7zError {
    /// The native bridge could not be initialized or completed.
    #[error("Format7zF bridge error {status}: {message}")]
    Bridge {
        /// Native status code.
        status: i32,
        /// Native diagnostic.
        message: String,
    },
    /// Native metadata did not satisfy the Rust model.
    #[error("invalid Format7zF metadata: {0}")]
    Metadata(String),
    /// An archive operation failed after the provider was loaded.
    #[error(transparent)]
    Operation(#[from] ArchiveError),
}
