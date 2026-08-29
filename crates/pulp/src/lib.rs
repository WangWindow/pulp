#![doc = "The Pulp archive library backed by the statically linked Format7zF SDK."]
#![allow(unsafe_code)]
#![deny(unsafe_op_in_unsafe_fn)]

mod backend;
mod detect;
mod engine;
mod entry;
mod error;
mod filesystem;
mod format;
mod io;
mod limits;
mod native;
mod operation;
mod password;
mod policy;
mod progress;
mod smart_extract;

pub use backend::{
    EntryOutcome, EntrySink, EntrySinkDecision, EntrySource, EntryVisitor, IgnoreEntryVisitor,
    MetadataOnlySink,
};
pub use detect::{
    DEFAULT_SNIFF_BYTES, DetectionCandidate, DetectionHint, DetectionMethod, DetectionResult,
    FormatDetector, sniff_prefix,
};
pub use engine::EngineInfo;
pub use entry::{ArchiveEntry, Checksum, EntryId, EntryKind, EntryName, EntryNameError};
pub use error::{ArchiveError, ArchiveResult};
pub use filesystem::{
    AtomicFile, FileSystemSink, FileSystemSource, FileVolumeProvider, OverwritePolicy,
    resolve_split_archive_path, validate_output_path,
};
pub use format::{
    ArchiveFormatId, CompressionMethod, FormatCapability, FormatDescriptor, LicenseNotice,
    Signature,
};
pub use io::{NoVolumeProvider, ReadSeek, VolumeProvider, WriteSeek};
pub use limits::{ResourceLimitKind, ResourceLimits};
pub use native::{ArchiveEngine, Format7zError};
pub use operation::{
    CancellationToken, CreateOptions, ExtractOptions, OperationContext, OperationKind,
    OperationReport, TestOptions, UpdateOptions,
};
pub use password::{
    NoPasswordProvider, Password, PasswordProvider, PasswordReason, PasswordRequest,
};
pub use policy::{ExtractionPolicy, LinkPolicy, MetadataPolicy, SpecialFilePolicy};
pub use progress::{NoopProgressReporter, OperationPhase, ProgressEvent, ProgressReporter};
pub use smart_extract::{
    CollisionPolicy, ExistingNameSet, ExtractDestination, ExtractPlan, PlannedEntry, PolicyWarning,
    plan_explicit_destination, plan_smart_destination, plan_smart_destination_with_policy,
};

/// The package name exposed by the library.
pub const PACKAGE_NAME: &str = env!("CARGO_PKG_NAME");

#[cfg(test)]
mod tests {
    use super::PACKAGE_NAME;

    #[test]
    fn exposes_the_library_package_name() {
        assert_eq!(PACKAGE_NAME, "pulp");
    }
}
