use std::io::{Read, Write};

use crate::entry::ArchiveEntry;
use crate::error::ArchiveResult;
use crate::operation::OperationContext;

/// Receives archive entries while an engine lists them.
pub trait EntryVisitor {
    /// Handles one entry's metadata.
    fn visit(&mut self, entry: &ArchiveEntry) -> ArchiveResult<()>;
}

/// Supplies entries and content to an archive creation operation.
pub trait EntrySource {
    /// Returns the next entry, or `None` after the source is exhausted.
    fn next(&mut self, context: &OperationContext) -> ArchiveResult<Option<ArchiveEntry>>;

    /// Opens the content stream for a regular-file entry.
    fn open<'a>(
        &'a mut self,
        entry: &ArchiveEntry,
        context: &OperationContext,
    ) -> ArchiveResult<Box<dyn Read + 'a>>;
}

/// Chooses how an engine should handle the current extracted entry.
pub enum EntrySinkDecision<'a> {
    /// Ignore this entry completely.
    Skip,
    /// Receive metadata but no content bytes.
    MetadataOnly,
    /// Write content to this caller-owned stream.
    Write(Box<dyn Write + 'a>),
}

/// Receives extracted metadata and content destinations.
pub trait EntrySink {
    /// Selects a destination for the current entry.
    fn begin<'a>(
        &'a mut self,
        entry: &ArchiveEntry,
        context: &OperationContext,
    ) -> ArchiveResult<EntrySinkDecision<'a>>;

    /// Finalizes one entry after the engine has finished writing it.
    fn finish(
        &mut self,
        entry: &ArchiveEntry,
        outcome: EntryOutcome,
        context: &OperationContext,
    ) -> ArchiveResult<()>;
}

/// Result of handling one extracted entry.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EntryOutcome {
    /// The entry's content was written successfully.
    Written {
        /// Number of bytes written to the destination.
        bytes: u64,
    },
    /// The entry was intentionally skipped.
    Skipped,
    /// The entry could not be completed.
    Failed,
}

/// A visitor that intentionally ignores every listed entry.
#[derive(Clone, Copy, Debug, Default)]
pub struct IgnoreEntryVisitor;

impl EntryVisitor for IgnoreEntryVisitor {
    fn visit(&mut self, _entry: &ArchiveEntry) -> ArchiveResult<()> {
        Ok(())
    }
}

/// A sink useful for test operations and metadata-only extraction.
#[derive(Clone, Copy, Debug, Default)]
pub struct MetadataOnlySink;

impl EntrySink for MetadataOnlySink {
    fn begin<'a>(
        &'a mut self,
        _entry: &ArchiveEntry,
        _context: &OperationContext,
    ) -> ArchiveResult<EntrySinkDecision<'a>> {
        Ok(EntrySinkDecision::MetadataOnly)
    }

    fn finish(
        &mut self,
        _entry: &ArchiveEntry,
        _outcome: EntryOutcome,
        _context: &OperationContext,
    ) -> ArchiveResult<()> {
        Ok(())
    }
}
