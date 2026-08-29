use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::entry::ArchiveEntry;
use crate::error::{ArchiveError, ArchiveResult};
use crate::format::ArchiveFormatId;
use crate::io::{NoVolumeProvider, ReadSeek, VolumeProvider};
use crate::limits::ResourceLimits;
use crate::password::{NoPasswordProvider, Password, PasswordProvider};
use crate::policy::ExtractionPolicy;
use crate::progress::{NoopProgressReporter, ProgressEvent, ProgressReporter};

/// The operation currently being performed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OperationKind {
    /// Detect a format.
    Detect,
    /// List metadata.
    List,
    /// Extract entries.
    Extract,
    /// Test archive data.
    Test,
    /// Create a new archive.
    Create,
    /// Update an existing archive.
    Update,
}

impl fmt::Display for OperationKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Detect => "detect",
            Self::List => "list",
            Self::Extract => "extract",
            Self::Test => "test",
            Self::Create => "create",
            Self::Update => "update",
        })
    }
}

/// A cancellation flag safe to clone into a worker and callbacks.
#[derive(Clone, Debug, Default)]
pub struct CancellationToken(Arc<AtomicBool>);

impl CancellationToken {
    /// Creates a non-cancelled token.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Requests cancellation.
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }

    /// Returns whether cancellation has been requested.
    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}

/// Shared operation dependencies passed to every engine callback.
pub struct OperationContext {
    cancellation: CancellationToken,
    progress: Arc<dyn ProgressReporter>,
    password: Arc<dyn PasswordProvider>,
    volumes: Arc<dyn VolumeProvider>,
    limits: ResourceLimits,
    policy: ExtractionPolicy,
}

impl Default for OperationContext {
    fn default() -> Self {
        Self {
            cancellation: CancellationToken::new(),
            progress: Arc::new(NoopProgressReporter),
            password: Arc::new(NoPasswordProvider),
            volumes: Arc::new(NoVolumeProvider),
            limits: ResourceLimits::default(),
            policy: ExtractionPolicy::default(),
        }
    }
}

impl OperationContext {
    /// Creates a context with default limits and no-op callbacks.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Replaces the cancellation token.
    #[must_use]
    pub fn with_cancellation(mut self, cancellation: CancellationToken) -> Self {
        self.cancellation = cancellation;
        self
    }

    /// Replaces the progress reporter.
    #[must_use]
    pub fn with_progress<P>(mut self, progress: P) -> Self
    where
        P: ProgressReporter + 'static,
    {
        self.progress = Arc::new(progress);
        self
    }

    /// Returns a copy for internal work that should not surface progress
    /// events as a separate user-visible operation.
    #[must_use]
    pub fn without_progress(&self) -> Self {
        Self {
            cancellation: self.cancellation.clone(),
            progress: Arc::new(NoopProgressReporter),
            password: Arc::clone(&self.password),
            volumes: Arc::clone(&self.volumes),
            limits: self.limits.clone(),
            policy: self.policy,
        }
    }

    /// Replaces the password provider.
    #[must_use]
    pub fn with_password_provider(mut self, password: Arc<dyn PasswordProvider>) -> Self {
        self.password = password;
        self
    }

    /// Replaces the resolver used for split archive volumes.
    #[must_use]
    pub fn with_volume_provider(mut self, volumes: Arc<dyn VolumeProvider>) -> Self {
        self.volumes = volumes;
        self
    }

    /// Replaces the resource limits.
    #[must_use]
    pub fn with_limits(mut self, limits: ResourceLimits) -> Self {
        self.limits = limits;
        self
    }

    /// Replaces the extraction safety policy used by filesystem sinks.
    #[must_use]
    pub fn with_policy(mut self, policy: ExtractionPolicy) -> Self {
        self.policy = policy;
        self
    }

    /// Returns the cancellation token.
    #[must_use]
    pub fn cancellation(&self) -> &CancellationToken {
        &self.cancellation
    }

    /// Returns whether the operation should stop.
    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.cancellation.is_cancelled()
    }

    /// Returns an error when cancellation was requested.
    pub fn check_cancelled(&self) -> ArchiveResult<()> {
        if self.is_cancelled() {
            Err(ArchiveError::Cancelled)
        } else {
            Ok(())
        }
    }

    /// Emits one progress event.
    pub fn report(&self, event: ProgressEvent) {
        self.progress.report(event);
    }

    /// Requests a password from the configured provider.
    pub fn request_password(
        &self,
        request: crate::password::PasswordRequest,
    ) -> ArchiveResult<Option<Password>> {
        self.password.request(request)
    }

    /// Opens a sibling volume requested by the native archive handler.
    pub fn open_volume(&self, name: &str) -> ArchiveResult<Box<dyn ReadSeek>> {
        self.check_cancelled()?;
        self.volumes.open_volume(name)
    }

    /// Returns the primary archive name for the current volume resolver.
    #[must_use]
    pub fn archive_name(&self) -> Option<String> {
        self.volumes.archive_name()
    }

    /// Validates per-entry limits before an engine or adapter starts work.
    pub fn validate_entry(&self, entry: &ArchiveEntry) -> ArchiveResult<()> {
        let path_bytes = entry.name.as_str().len();
        if path_bytes > self.limits.max_path_bytes {
            return Err(ArchiveError::ResourceLimit {
                kind: crate::limits::ResourceLimitKind::PathBytes,
                message: format!(
                    "entry path is {path_bytes} bytes, maximum is {}",
                    self.limits.max_path_bytes
                ),
            });
        }
        if let Some(size) = entry.size
            && size > self.limits.max_entry_bytes
        {
            return Err(ArchiveError::ResourceLimit {
                kind: crate::limits::ResourceLimitKind::EntryBytes,
                message: format!(
                    "entry declares {size} bytes, maximum is {}",
                    self.limits.max_entry_bytes
                ),
            });
        }
        Ok(())
    }

    /// Returns the configured safety limits.
    #[must_use]
    pub fn limits(&self) -> &ResourceLimits {
        &self.limits
    }

    /// Returns the extraction safety policy.
    #[must_use]
    pub const fn policy(&self) -> &ExtractionPolicy {
        &self.policy
    }
}

/// Options shared by archive creation and update operations.
#[derive(Clone, Debug, Default)]
pub struct CreateOptions {
    /// Requested method identifier, interpreted by the selected handler.
    pub compression_method: Option<String>,
    /// Requested compression level.
    pub compression_level: Option<u32>,
    /// Optional archive password.
    pub password: Option<Password>,
    /// Whether the handler should use solid mode when supported.
    pub solid: Option<bool>,
    /// Whether archive headers should be encrypted when supported.
    pub header_encryption: bool,
    /// Requested volume size for multi-volume output.
    pub volume_size: Option<u64>,
}

/// Options for extraction.
#[derive(Clone, Debug, Default)]
pub struct ExtractOptions {
    /// Selected entry IDs; `None` means all entries.
    pub selected: Option<Vec<crate::entry::EntryId>>,
    /// Whether metadata should be restored by a filesystem adapter.
    pub restore_metadata: bool,
}

/// Options for verification.
#[derive(Clone, Debug)]
pub struct TestOptions {
    /// Whether compressed data should be fully decoded.
    pub verify_data: bool,
}

impl Default for TestOptions {
    fn default() -> Self {
        Self { verify_data: true }
    }
}

/// Options for updating an archive.
#[derive(Clone, Debug, Default)]
pub struct UpdateOptions {
    /// Creation/update properties.
    pub create: CreateOptions,
    /// Whether entries absent from the source may remain in an existing archive.
    /// Providers without an existing-archive update adapter reject this option.
    pub keep_unmentioned: bool,
}

/// A compact summary of a completed operation.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct OperationReport {
    /// Format processed by the operation.
    pub format: Option<ArchiveFormatId>,
    /// Operation kind.
    pub operation: Option<OperationKind>,
    /// Entries observed.
    pub entries_seen: u64,
    /// Entries completed successfully.
    pub entries_completed: u64,
    /// Bytes read from archive/source entries.
    pub bytes_read: u64,
    /// Bytes written to archive/output entries.
    pub bytes_written: u64,
    /// Non-fatal warnings.
    pub warnings: Vec<String>,
    /// Adapter diagnostics.
    pub diagnostics: Vec<String>,
}

impl OperationReport {
    /// Creates an empty report for an operation.
    #[must_use]
    pub fn new(operation: OperationKind) -> Self {
        Self {
            operation: Some(operation),
            ..Self::default()
        }
    }

    /// Sets the runtime format.
    #[must_use]
    pub fn with_format(mut self, format: ArchiveFormatId) -> Self {
        self.format = Some(format);
        self
    }
}
