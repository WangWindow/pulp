use std::sync::Arc;

use crate::entry::{EntryId, EntryName};
use crate::operation::{OperationKind, OperationReport};

/// A coarse phase shown by clients while an operation runs.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OperationPhase {
    /// Opening and reading archive metadata.
    Opening,
    /// Processing entries.
    Processing,
    /// Writing final metadata and closing.
    Finalizing,
}

/// Progress emitted by engines and filesystem adapters.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProgressEvent {
    /// An operation has started.
    Started {
        /// Operation kind.
        operation: OperationKind,
        /// Total bytes when known.
        total_bytes: Option<u64>,
    },
    /// A new entry is being processed.
    EntryStarted {
        /// Entry identifier.
        id: EntryId,
        /// Validated entry name.
        name: EntryName,
        /// Unpacked size when known.
        size: Option<u64>,
    },
    /// More bytes have been processed.
    Bytes {
        /// Delta since the previous event.
        delta: u64,
        /// Total bytes processed so far.
        processed: u64,
        /// Total bytes when known.
        total: Option<u64>,
    },
    /// A non-fatal diagnostic.
    Warning(String),
    /// The operation has completed.
    Finished(OperationReport),
}

/// Receives progress without imposing an async runtime on the core library.
pub trait ProgressReporter: Send + Sync {
    /// Handles one progress event.
    fn report(&self, event: ProgressEvent);
}

impl<F> ProgressReporter for F
where
    F: Fn(ProgressEvent) + Send + Sync,
{
    fn report(&self, event: ProgressEvent) {
        self(event);
    }
}

impl<T> ProgressReporter for Arc<T>
where
    T: ProgressReporter + ?Sized,
{
    fn report(&self, event: ProgressEvent) {
        (**self).report(event);
    }
}

/// A reporter that discards all events.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoopProgressReporter;

impl ProgressReporter for NoopProgressReporter {
    fn report(&self, _event: ProgressEvent) {}
}
