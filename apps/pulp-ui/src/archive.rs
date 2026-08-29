//! Archive operations and data projection for the desktop workspace.

use std::collections::{HashSet, hash_map::DefaultHasher};
use std::fs::{self, File};
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::{Arc, mpsc::Sender};

use pulp::{
    ArchiveEngine, ArchiveEntry, ArchiveError, ArchiveFormatId, ArchiveResult, AtomicFile,
    CancellationToken, CollisionPolicy, CreateOptions, EntryOutcome, EntrySink, EntrySinkDecision,
    EntryVisitor, ExtractOptions, FileSystemSink, FileSystemSource, FileVolumeProvider,
    FormatCapability, NoVolumeProvider, OperationContext, OperationReport, PasswordProvider,
    ProgressEvent, TestOptions, plan_smart_destination_with_policy, resolve_split_archive_path,
};

use crate::settings::SettingsFile;

/// Metadata loaded from one archive.
#[derive(Clone, Debug)]
pub struct LoadedArchive {
    /// Host path of the archive.
    pub path: PathBuf,
    /// All archive entries in provider order.
    pub entries: Vec<ArchiveEntry>,
}

/// A writable handler exposed to archive-creation controls.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FormatOption {
    /// Handler identifier.
    pub id: ArchiveFormatId,
    /// Provider display name.
    pub name: String,
    /// Preferred filename extension.
    pub extension: String,
    /// Writable compression methods.
    pub methods: Vec<String>,
}

/// Dependencies shared by one background archive job.
pub struct ArchiveJob {
    settings: SettingsFile,
    progress: Sender<ProgressEvent>,
    cancellation: CancellationToken,
    password: Arc<dyn PasswordProvider>,
}

impl ArchiveJob {
    /// Creates an operation from the current UI preferences and callbacks.
    pub fn new(
        settings: SettingsFile,
        progress: Sender<ProgressEvent>,
        cancellation: CancellationToken,
        password: Arc<dyn PasswordProvider>,
    ) -> Self {
        Self {
            settings,
            progress,
            cancellation,
            password,
        }
    }

    fn context(&self, volumes: Arc<dyn pulp::VolumeProvider>) -> OperationContext {
        operation_context(
            &self.settings,
            self.progress.clone(),
            self.cancellation.clone(),
            Arc::clone(&self.password),
            volumes,
        )
    }
}

/// Opens and lists an archive on a worker thread.
pub fn load_archive(path: PathBuf, operation: ArchiveJob) -> ArchiveResult<LoadedArchive> {
    let path = resolve_split_archive_path(path);
    let engine = load_engine()?;
    let mut archive = File::open(&path)?;
    let volumes = Arc::new(FileVolumeProvider::new(path.clone()));
    let context = operation.context(volumes);
    let mut collector = EntryCollector::default();
    engine.list(&mut archive, &context, &mut collector)?;
    Ok(LoadedArchive {
        path,
        entries: collector.entries,
    })
}

/// Extracts all entries or the selected entry subtrees.
pub fn extract_archive(
    archive: &LoadedArchive,
    destination: PathBuf,
    selected: HashSet<String>,
    operation: ArchiveJob,
) -> ArchiveResult<OperationReport> {
    let engine = load_engine()?;
    let mut input = File::open(&archive.path)?;
    let volumes = Arc::new(FileVolumeProvider::new(archive.path.clone()));
    let context = operation.context(volumes);
    let mut sink = SelectionSink::new(destination, selected, operation.settings.overwrite_policy());
    let report = engine.extract(
        &mut input,
        &context,
        &mut sink,
        &ExtractOptions {
            restore_metadata: operation.settings.extraction.restore_metadata,
            ..ExtractOptions::default()
        },
    )?;
    sink.finalize()?;
    Ok(report)
}

/// Tests all compressed data in an archive.
pub fn test_archive(
    archive: &LoadedArchive,
    operation: ArchiveJob,
) -> ArchiveResult<OperationReport> {
    let engine = load_engine()?;
    let mut input = File::open(&archive.path)?;
    let volumes = Arc::new(FileVolumeProvider::new(archive.path.clone()));
    engine.test(
        &mut input,
        &operation.context(volumes),
        &TestOptions::default(),
    )
}

/// Creates an archive from one or more host filesystem roots.
pub fn create_archive(
    sources: Vec<PathBuf>,
    output: PathBuf,
    format: ArchiveFormatId,
    options: CreateOptions,
    operation: ArchiveJob,
) -> ArchiveResult<OperationReport> {
    for source in &sources {
        pulp::validate_output_path(source, &output)?;
    }
    let engine = load_engine()?;
    let mut source = FileSystemSource::from_paths(sources)?;
    let mut archive = AtomicFile::create(&output)?;
    let context = operation.context(Arc::new(NoVolumeProvider));
    let report = engine.create(&format, &mut source, &mut archive, &options, &context)?;
    archive.commit()?;
    if operation.settings.archive.test_after_create {
        let mut created = File::open(output)?;
        engine.test(&mut created, &context, &TestOptions::default())?;
    }
    Ok(report)
}

/// Returns the destination selected by smart extraction.
pub fn smart_destination(
    archive: &LoadedArchive,
    base: &Path,
    policy: CollisionPolicy,
) -> ArchiveResult<PathBuf> {
    let existing = existing_names(base)?;
    let plan = plan_smart_destination_with_policy(
        &archive_stem(&archive.path),
        &archive.entries,
        &existing,
        policy,
    )?;
    if plan.destination_name.is_empty() {
        Ok(base.to_owned())
    } else {
        Ok(base.join(plan.destination_name))
    }
}

/// Returns writable handlers and their provider-reported methods.
pub fn creation_formats() -> ArchiveResult<Vec<FormatOption>> {
    let engine = load_engine()?;
    let mut formats = engine
        .formats()
        .iter()
        .filter(|descriptor| descriptor.supports(FormatCapability::Create))
        .map(|descriptor| FormatOption {
            id: descriptor.id.clone(),
            name: descriptor.name.clone(),
            extension: descriptor
                .extensions
                .iter()
                .chain(descriptor.add_extensions.iter())
                .map(|extension| extension.trim_start_matches('.'))
                .find(|extension| !extension.is_empty() && *extension != "*")
                .unwrap_or(descriptor.id.as_str())
                .to_owned(),
            methods: descriptor
                .methods
                .iter()
                .filter(|method| method.can_encode)
                .map(|method| method.id.clone())
                .collect(),
        })
        .collect::<Vec<_>>();
    formats.sort_by(|left, right| left.id.cmp(&right.id));
    Ok(formats)
}

fn load_engine() -> ArchiveResult<ArchiveEngine> {
    ArchiveEngine::load().map_err(|error| ArchiveError::backend("Format7zF", error.to_string()))
}

fn operation_context(
    settings: &SettingsFile,
    progress: Sender<ProgressEvent>,
    cancellation: CancellationToken,
    password: Arc<dyn PasswordProvider>,
    volumes: Arc<dyn pulp::VolumeProvider>,
) -> OperationContext {
    OperationContext::new()
        .with_cancellation(cancellation)
        .with_password_provider(password)
        .with_volume_provider(volumes)
        .with_limits(settings.resource_limits())
        .with_policy(settings.extraction_policy())
        .with_progress(move |event| {
            let _ = progress.send(event);
        })
}

struct SelectionSink {
    inner: FileSystemSink,
    selected: HashSet<String>,
}

impl SelectionSink {
    fn new(
        destination: PathBuf,
        selected: HashSet<String>,
        overwrite: pulp::OverwritePolicy,
    ) -> Self {
        let inner = FileSystemSink::new(destination).with_overwrite(overwrite);
        Self { inner, selected }
    }

    fn includes(&self, name: &str) -> bool {
        if self.selected.is_empty() {
            return true;
        }
        self.selected.iter().any(|selected| {
            same_or_descendant(name, selected) || same_or_descendant(selected, name)
        })
    }

    fn finalize(&mut self) -> ArchiveResult<()> {
        self.inner.finalize()
    }
}

impl EntrySink for SelectionSink {
    fn begin<'a>(
        &'a mut self,
        entry: &ArchiveEntry,
        context: &OperationContext,
    ) -> ArchiveResult<EntrySinkDecision<'a>> {
        if self.includes(entry.name.as_str()) {
            self.inner.begin(entry, context)
        } else {
            Ok(EntrySinkDecision::Skip)
        }
    }

    fn finish(
        &mut self,
        entry: &ArchiveEntry,
        outcome: EntryOutcome,
        context: &OperationContext,
    ) -> ArchiveResult<()> {
        self.inner.finish(entry, outcome, context)
    }
}

#[derive(Default)]
struct EntryCollector {
    entries: Vec<ArchiveEntry>,
}

impl EntryVisitor for EntryCollector {
    fn visit(&mut self, entry: &ArchiveEntry) -> ArchiveResult<()> {
        self.entries.push(entry.clone());
        Ok(())
    }
}

fn existing_names(path: &Path) -> ArchiveResult<HashSet<String>> {
    match fs::read_dir(path) {
        Ok(entries) => entries
            .map(|entry| {
                entry
                    .map(|entry| entry.file_name().to_string_lossy().into_owned())
                    .map_err(ArchiveError::from)
            })
            .collect(),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(HashSet::new()),
        Err(error) => Err(error.into()),
    }
}

fn archive_stem(path: &Path) -> String {
    path.file_stem()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .unwrap_or("archive")
        .to_owned()
}

fn same_or_descendant(name: &str, parent: &str) -> bool {
    name == parent
        || name
            .strip_prefix(parent)
            .is_some_and(|remainder| remainder.starts_with('/'))
}

/// Returns a stable identifier for a host path used by list row elements.
#[must_use]
pub fn row_id(entry: &ArchiveEntry) -> String {
    let mut hasher = DefaultHasher::new();
    entry.id.hash(&mut hasher);
    entry.name.hash(&mut hasher);
    format!("entry-{:x}", hasher.finish())
}

#[cfg(test)]
mod tests {
    use super::same_or_descendant;

    #[test]
    fn directory_selection_respects_component_boundaries() {
        assert!(same_or_descendant("docs/readme.md", "docs"));
        assert!(!same_or_descendant("documentation/readme.md", "docs"));
    }
}
