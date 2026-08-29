#![doc = "Filesystem adapters for Pulp archive callers."]
#![deny(unsafe_code)]

use std::collections::HashSet;
use std::fs::{self, File, Metadata, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::{
    ArchiveEntry, ArchiveError, ArchiveResult, EntryId, EntryKind, EntryName, EntryOutcome,
    EntrySink, EntrySinkDecision, EntrySource, MetadataPolicy, OperationContext, ReadSeek,
    VolumeProvider,
};
use filetime::{FileTime, set_file_mtime};

static NEXT_TEMP_ID: AtomicU64 = AtomicU64::new(0);

/// Resolves a user-selected split volume to the archive's primary file.
///
/// Format7zF opens sibling volumes by name after the primary file has been
/// opened. The caller still needs to open the primary volume first, so this
/// helper handles the common `.7z.002`, `.z01`, `.part2.rar`, `.r00`, and
/// numeric `.002` entry points.
#[must_use]
pub fn resolve_split_archive_path(path: impl AsRef<Path>) -> PathBuf {
    let path = path.as_ref();
    let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
        return path.to_owned();
    };
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let Some(primary_name) = primary_volume_name(file_name) else {
        return path.to_owned();
    };
    let candidate = parent.join(primary_name);
    if candidate.is_file() {
        candidate
    } else {
        path.to_owned()
    }
}

/// Opens sibling files requested by the native split-archive callback.
#[derive(Clone, Debug)]
pub struct FileVolumeProvider {
    archive: PathBuf,
}

impl FileVolumeProvider {
    /// Creates a provider rooted beside the given primary archive file.
    #[must_use]
    pub fn new(archive: impl Into<PathBuf>) -> Self {
        Self {
            archive: archive.into(),
        }
    }
}

impl VolumeProvider for FileVolumeProvider {
    fn open_volume(&self, name: &str) -> ArchiveResult<Box<dyn ReadSeek>> {
        let requested = Path::new(name);
        let mut components = requested.components();
        let Some(Component::Normal(file_name)) = components.next() else {
            return Err(ArchiveError::invalid_input(
                "native volume name is not a file name",
            ));
        };
        if components.next().is_some() {
            return Err(ArchiveError::invalid_input(
                "native volume name contains a path",
            ));
        }
        let parent = self.archive.parent().unwrap_or_else(|| Path::new("."));
        let path = parent.join(file_name);
        match File::open(&path) {
            Ok(file) => Ok(Box::new(file)),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                Err(ArchiveError::VolumeNotFound(path.display().to_string()))
            }
            Err(error) => Err(error.into()),
        }
    }

    fn archive_name(&self) -> Option<String> {
        self.archive
            .file_name()
            .and_then(|name| name.to_str())
            .map(str::to_owned)
    }
}

fn primary_volume_name(file_name: &str) -> Option<String> {
    let (base, extension) = file_name.rsplit_once('.')?;
    let lower_extension = extension.to_ascii_lowercase();

    if base.to_ascii_lowercase().ends_with(".7z")
        && extension.len() >= 3
        && extension.bytes().all(|byte| byte.is_ascii_digit())
    {
        return Some(format!("{base}.{:0width$}", 1, width = extension.len()));
    }

    if lower_extension.starts_with('z')
        && lower_extension.len() >= 3
        && lower_extension[1..]
            .bytes()
            .all(|byte| byte.is_ascii_digit())
    {
        return Some(format!("{base}.zip"));
    }

    if let Some(marker) = file_name.to_ascii_lowercase().rfind(".part") {
        let prefix = &file_name[..marker];
        let suffix = &file_name[marker + ".part".len()..];
        if let Some((number, extension)) = suffix.rsplit_once('.')
            && !number.is_empty()
            && number.bytes().all(|byte| byte.is_ascii_digit())
            && extension.eq_ignore_ascii_case("rar")
        {
            return Some(format!(
                "{prefix}.part{:0width$}.{extension}",
                1,
                width = number.len()
            ));
        }
    }

    if lower_extension.starts_with('r')
        && lower_extension.len() >= 3
        && lower_extension[1..]
            .bytes()
            .all(|byte| byte.is_ascii_digit())
    {
        return Some(format!("{base}.rar"));
    }

    if extension.len() >= 3 && extension.bytes().all(|byte| byte.is_ascii_digit()) {
        return Some(format!("{base}.{:0width$}", 1, width = extension.len()));
    }

    None
}

/// Policy for an existing extraction destination.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum OverwritePolicy {
    /// Refuse to replace an existing path.
    #[default]
    Error,
    /// Replace an existing regular file or link.
    Replace,
    /// Leave an existing path untouched.
    Skip,
}

/// A seekable output file that is published by an atomic rename.
pub struct AtomicFile {
    file: File,
    target: PathBuf,
    temporary: PathBuf,
    committed: bool,
}

impl AtomicFile {
    /// Creates a temporary output file beside the requested target.
    pub fn create(path: impl AsRef<Path>) -> ArchiveResult<Self> {
        let target = path.as_ref().to_owned();
        let parent = target.parent().unwrap_or_else(|| Path::new("."));
        ensure_directory_tree(parent)?;
        if let Ok(metadata) = fs::symlink_metadata(&target)
            && metadata.is_dir()
        {
            return Err(ArchiveError::PolicyViolation(format!(
                "archive output is a directory: {}",
                target.display()
            )));
        }
        let (temporary, file) = create_temporary_file(parent, target.file_name())?;
        Ok(Self {
            file,
            target,
            temporary,
            committed: false,
        })
    }

    /// Flushes and publishes the temporary file at its target path.
    pub fn commit(mut self) -> ArchiveResult<()> {
        self.file.flush()?;
        self.file.sync_all()?;
        fs::rename(&self.temporary, &self.target)?;
        self.committed = true;
        Ok(())
    }
}

impl Write for AtomicFile {
    fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
        self.file.write(buffer)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.file.flush()
    }
}

impl Seek for AtomicFile {
    fn seek(&mut self, position: SeekFrom) -> std::io::Result<u64> {
        self.file.seek(position)
    }
}

impl Drop for AtomicFile {
    fn drop(&mut self) {
        if !self.committed {
            let _ = fs::remove_file(&self.temporary);
        }
    }
}

#[derive(Clone)]
struct SourceItem {
    path: PathBuf,
    entry: ArchiveEntry,
}

/// A deterministic filesystem-backed archive source.
pub struct FileSystemSource {
    items: Vec<SourceItem>,
    cursor: usize,
}

impl FileSystemSource {
    /// Creates a source for one file or a recursively walked directory.
    pub fn new(path: impl AsRef<Path>) -> ArchiveResult<Self> {
        Self::from_paths([path.as_ref().to_owned()])
    }

    /// Creates one deterministic source from several files or directories.
    pub fn from_paths(paths: impl IntoIterator<Item = PathBuf>) -> ArchiveResult<Self> {
        let mut roots = paths.into_iter().collect::<Vec<_>>();
        if roots.is_empty() {
            return Err(ArchiveError::InvalidInput(
                "at least one archive source is required".to_owned(),
            ));
        }
        roots.sort_by(|left, right| {
            source_name(left)
                .unwrap_or_default()
                .cmp(&source_name(right).unwrap_or_default())
                .then_with(|| left.cmp(right))
        });

        let mut items = Vec::new();
        let mut next_id = 0_u64;
        let mut root_names = HashSet::new();
        for path in roots {
            let metadata = fs::symlink_metadata(&path)?;
            if metadata.file_type().is_symlink() {
                return Err(ArchiveError::PathViolation(format!(
                    "source path is a symbolic link: {}",
                    path.display()
                )));
            }

            let root_name = source_name(&path)?;
            if !root_names.insert(root_name.clone()) {
                return Err(ArchiveError::InvalidInput(format!(
                    "source root name is duplicated: {root_name}"
                )));
            }
            let root_entry_name = EntryName::parse(root_name)?;
            if metadata.is_file() {
                items.push(SourceItem {
                    path,
                    entry: entry_from_metadata(
                        next_entry_id(&mut next_id)?,
                        root_entry_name,
                        EntryKind::File,
                        &metadata,
                    ),
                });
            } else if metadata.is_dir() {
                items.push(SourceItem {
                    path: path.clone(),
                    entry: entry_from_metadata(
                        next_entry_id(&mut next_id)?,
                        root_entry_name.clone(),
                        EntryKind::Directory,
                        &metadata,
                    ),
                });
                collect_directory(&path, &root_entry_name, &mut next_id, &mut items)?;
            } else {
                return Err(ArchiveError::Unsupported(format!(
                    "source path is not a regular file or directory: {}",
                    path.display()
                )));
            }
        }
        Ok(Self { items, cursor: 0 })
    }
}

impl EntrySource for FileSystemSource {
    fn next(&mut self, context: &OperationContext) -> ArchiveResult<Option<ArchiveEntry>> {
        context.check_cancelled()?;
        let item = self.items.get(self.cursor).cloned();
        if item.is_some() {
            self.cursor += 1;
        }
        Ok(item.map(|item| item.entry))
    }

    fn open<'a>(
        &'a mut self,
        entry: &ArchiveEntry,
        _context: &OperationContext,
    ) -> ArchiveResult<Box<dyn Read + 'a>> {
        if entry.kind != EntryKind::File {
            return Err(ArchiveError::InvalidInput(format!(
                "only regular files have content: {}",
                entry.name
            )));
        }
        let item = self
            .items
            .iter()
            .find(|item| item.entry.id == entry.id)
            .ok_or_else(|| {
                ArchiveError::InvalidInput("entry does not belong to this source".to_owned())
            })?;
        Ok(Box::new(File::open(&item.path)?))
    }
}

struct PendingFile {
    temporary: PathBuf,
    destination: PathBuf,
}

/// A filesystem-backed extraction sink with path and overwrite policies.
pub struct FileSystemSink {
    destination: PathBuf,
    overwrite: OverwritePolicy,
    pending_file: Option<PendingFile>,
    pending_directory: Option<PathBuf>,
}

impl FileSystemSink {
    /// Creates a sink rooted at `destination`.
    pub fn new(destination: impl Into<PathBuf>) -> Self {
        Self {
            destination: destination.into(),
            overwrite: OverwritePolicy::default(),
            pending_file: None,
            pending_directory: None,
        }
    }

    /// Sets the existing-path policy.
    #[must_use]
    pub const fn with_overwrite(mut self, overwrite: OverwritePolicy) -> Self {
        self.overwrite = overwrite;
        self
    }

    /// Removes an unfinished temporary file, if any.
    pub fn finalize(&mut self) -> ArchiveResult<()> {
        self.cleanup_pending_file();
        self.pending_directory = None;
        Ok(())
    }

    fn path_for(&self, name: &EntryName) -> PathBuf {
        let mut path = self.destination.clone();
        for component in name.components() {
            path.push(component);
        }
        path
    }

    fn cleanup_pending_file(&mut self) {
        if let Some(pending) = self.pending_file.take() {
            let _ = fs::remove_file(pending.temporary);
        }
    }

    fn ensure_idle(&self) -> ArchiveResult<()> {
        if self.pending_file.is_some() || self.pending_directory.is_some() {
            Err(ArchiveError::Internal(
                "filesystem sink received a new entry before finishing the previous one".to_owned(),
            ))
        } else {
            Ok(())
        }
    }
}

impl EntrySink for FileSystemSink {
    fn begin<'a>(
        &'a mut self,
        entry: &ArchiveEntry,
        context: &OperationContext,
    ) -> ArchiveResult<EntrySinkDecision<'a>> {
        self.ensure_idle()?;
        context.validate_entry(entry)?;
        let destination = self.path_for(&entry.name);
        match entry.kind {
            EntryKind::File => {
                let parent = destination.parent().unwrap_or_else(|| Path::new("."));
                ensure_directory_tree(parent)?;
                if path_exists(&destination) {
                    match self.overwrite {
                        OverwritePolicy::Error => {
                            return Err(existing_path_error(&destination));
                        }
                        OverwritePolicy::Skip => return Ok(EntrySinkDecision::Skip),
                        OverwritePolicy::Replace => {
                            if fs::symlink_metadata(&destination)?.is_dir() {
                                return Err(ArchiveError::PolicyViolation(format!(
                                    "cannot replace directory with file: {}",
                                    destination.display()
                                )));
                            }
                        }
                    }
                }
                let (temporary, file) = create_temporary_file(parent, destination.file_name())?;
                self.pending_file = Some(PendingFile {
                    temporary,
                    destination,
                });
                Ok(EntrySinkDecision::Write(Box::new(file)))
            }
            EntryKind::Directory => {
                ensure_directory_entry(&destination, self.overwrite)?;
                self.pending_directory = Some(destination);
                Ok(EntrySinkDecision::MetadataOnly)
            }
            EntryKind::Symlink => {
                if context.policy().links != crate::LinkPolicy::Preserve {
                    return Err(ArchiveError::PolicyViolation(format!(
                        "symbolic link extraction is disabled: {}",
                        entry.name
                    )));
                }
                let target = entry.link_target.as_deref().ok_or_else(|| {
                    ArchiveError::InvalidInput("symbolic link has no target".to_owned())
                })?;
                let target = EntryName::parse(target)?;
                prepare_link_destination(&destination, self.overwrite)?;
                ensure_directory_tree(destination.parent().unwrap_or_else(|| Path::new(".")))?;
                #[cfg(unix)]
                std::os::unix::fs::symlink(target.as_str(), &destination)?;
                #[cfg(not(unix))]
                return Err(ArchiveError::Unsupported(
                    "symbolic link extraction is unavailable on this platform".to_owned(),
                ));
                Ok(EntrySinkDecision::MetadataOnly)
            }
            EntryKind::Hardlink => {
                if context.policy().links != crate::LinkPolicy::Preserve {
                    return Err(ArchiveError::PolicyViolation(format!(
                        "hard link extraction is disabled: {}",
                        entry.name
                    )));
                }
                let target = entry.link_target.as_deref().ok_or_else(|| {
                    ArchiveError::InvalidInput("hard link has no target".to_owned())
                })?;
                let target_name = EntryName::parse(target)?;
                let target_path = self.path_for(&target_name);
                prepare_link_destination(&destination, self.overwrite)?;
                fs::hard_link(target_path, destination)?;
                Ok(EntrySinkDecision::MetadataOnly)
            }
            EntryKind::Special => {
                if context.policy().special_files == crate::SpecialFilePolicy::Reject {
                    Err(ArchiveError::PolicyViolation(format!(
                        "special-file extraction is disabled: {}",
                        entry.name
                    )))
                } else {
                    Err(ArchiveError::Unsupported(
                        "creating special files is not implemented by the filesystem adapter"
                            .to_owned(),
                    ))
                }
            }
        }
    }

    fn finish(
        &mut self,
        entry: &ArchiveEntry,
        outcome: EntryOutcome,
        context: &OperationContext,
    ) -> ArchiveResult<()> {
        match outcome {
            EntryOutcome::Written { .. } => {
                if let Some(pending) = self.pending_file.take() {
                    if self.overwrite == OverwritePolicy::Error && path_exists(&pending.destination)
                    {
                        let _ = fs::remove_file(&pending.temporary);
                        return Err(existing_path_error(&pending.destination));
                    }
                    fs::rename(&pending.temporary, &pending.destination)?;
                    apply_metadata(&pending.destination, entry, context)?;
                } else if let Some(directory) = self.pending_directory.take() {
                    apply_metadata(&directory, entry, context)?;
                }
            }
            EntryOutcome::Skipped => {
                self.cleanup_pending_file();
                self.pending_directory = None;
            }
            EntryOutcome::Failed => {
                self.cleanup_pending_file();
                self.pending_directory = None;
            }
        }
        Ok(())
    }
}

impl Drop for FileSystemSink {
    fn drop(&mut self) {
        self.cleanup_pending_file();
    }
}

/// Rejects output paths that would overwrite the source or be created below a source directory.
pub fn validate_output_path(source: &Path, output: &Path) -> ArchiveResult<()> {
    let source_metadata = fs::symlink_metadata(source)?;
    if source_metadata.file_type().is_symlink() {
        return Err(ArchiveError::PathViolation(format!(
            "source path is a symbolic link: {}",
            source.display()
        )));
    }
    let source = fs::canonicalize(source)?;
    let output = canonicalize_or_append(output)?;
    if source == output || (source_metadata.is_dir() && output.starts_with(&source)) {
        return Err(ArchiveError::PolicyViolation(format!(
            "archive output must not be the source or one of its children: {}",
            output.display()
        )));
    }
    Ok(())
}

fn collect_directory(
    directory: &Path,
    prefix: &EntryName,
    next_id: &mut u64,
    items: &mut Vec<SourceItem>,
) -> ArchiveResult<()> {
    let mut children = fs::read_dir(directory)?.collect::<Result<Vec<_>, _>>()?;
    children.sort_by_key(|entry| entry.file_name());
    for child in children {
        let path = child.path();
        let metadata = fs::symlink_metadata(&path)?;
        if metadata.file_type().is_symlink() {
            return Err(ArchiveError::Unsupported(format!(
                "source contains a symbolic link: {}",
                path.display()
            )));
        }
        let child_name = child.file_name().into_string().map_err(|_| {
            ArchiveError::InvalidInput("source filename is not valid UTF-8".to_owned())
        })?;
        let name = EntryName::parse(format!("{prefix}/{child_name}"))?;
        let kind = if metadata.is_dir() {
            EntryKind::Directory
        } else if metadata.is_file() {
            EntryKind::File
        } else {
            return Err(ArchiveError::Unsupported(format!(
                "source contains a non-regular entry: {}",
                path.display()
            )));
        };
        items.push(SourceItem {
            path: path.clone(),
            entry: entry_from_metadata(next_entry_id(next_id)?, name.clone(), kind, &metadata),
        });
        if kind == EntryKind::Directory {
            collect_directory(&path, &name, next_id, items)?;
        }
    }
    Ok(())
}

fn entry_from_metadata(
    id: EntryId,
    name: EntryName,
    kind: EntryKind,
    metadata: &Metadata,
) -> ArchiveEntry {
    let mut entry = ArchiveEntry::new(id, name, kind);
    if kind == EntryKind::File {
        entry.size = Some(metadata.len());
    }
    entry.modified = metadata.modified().ok();
    entry.accessed = metadata.accessed().ok();
    entry.created = metadata.created().ok();
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        entry.attributes = Some(metadata.mode() as u64);
        entry.posix_attributes = Some(metadata.mode() as u64);
    }
    entry
}

fn next_entry_id(next_id: &mut u64) -> ArchiveResult<EntryId> {
    let id = EntryId::new(*next_id);
    *next_id = next_id
        .checked_add(1)
        .ok_or_else(|| ArchiveError::ResourceLimit {
            kind: crate::ResourceLimitKind::Entries,
            message: "source entry identifier overflowed".to_owned(),
        })?;
    Ok(id)
}

fn source_name(path: &Path) -> ArchiveResult<String> {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !matches!(*name, "." | "..") && !name.is_empty())
        .unwrap_or("content");
    Ok(name.to_owned())
}

fn ensure_directory_entry(path: &Path, overwrite: OverwritePolicy) -> ArchiveResult<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => Ok(()),
        Ok(metadata) => match overwrite {
            OverwritePolicy::Error => Err(existing_path_error(path)),
            OverwritePolicy::Skip => Ok(()),
            OverwritePolicy::Replace => {
                if metadata.is_dir() {
                    return Err(ArchiveError::PolicyViolation(format!(
                        "cannot replace symbolic directory: {}",
                        path.display()
                    )));
                }
                fs::remove_file(path)?;
                ensure_directory_tree(path)
            }
        },
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => ensure_directory_tree(path),
        Err(error) => Err(error.into()),
    }
}

fn prepare_link_destination(path: &Path, overwrite: OverwritePolicy) -> ArchiveResult<()> {
    if !path_exists(path) {
        return Ok(());
    }
    match overwrite {
        OverwritePolicy::Error => Err(existing_path_error(path)),
        OverwritePolicy::Skip => Err(ArchiveError::PolicyViolation(format!(
            "link destination already exists and cannot be selected: {}",
            path.display()
        ))),
        OverwritePolicy::Replace => {
            let metadata = fs::symlink_metadata(path)?;
            if metadata.is_dir() {
                return Err(ArchiveError::PolicyViolation(format!(
                    "cannot replace directory with link: {}",
                    path.display()
                )));
            }
            fs::remove_file(path)?;
            Ok(())
        }
    }
}

fn apply_metadata(
    path: &Path,
    entry: &ArchiveEntry,
    context: &OperationContext,
) -> ArchiveResult<()> {
    if context.policy().metadata != MetadataPolicy::RestoreSafe {
        return Ok(());
    }
    #[cfg(unix)]
    if let Some(mode) = entry.posix_attributes {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode((mode as u32) & 0o7777))?;
    }
    if let Some(modified) = entry.modified {
        set_file_mtime(path, FileTime::from_system_time(modified))?;
    }
    Ok(())
}

fn path_exists(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok()
}

fn existing_path_error(path: &Path) -> ArchiveError {
    ArchiveError::PolicyViolation(format!("destination already exists: {}", path.display()))
}

fn ensure_directory_tree(path: &Path) -> ArchiveResult<()> {
    let mut current = PathBuf::new();
    for component in path.components() {
        current.push(component.as_os_str());
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {}
            Ok(_) => {
                return Err(ArchiveError::PathViolation(format!(
                    "path component is not a directory: {}",
                    current.display()
                )));
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                fs::create_dir(&current)?;
            }
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}

fn create_temporary_file(
    parent: &Path,
    file_name: Option<&std::ffi::OsStr>,
) -> ArchiveResult<(PathBuf, File)> {
    let base = file_name
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .unwrap_or("archive");
    for _ in 0..32 {
        let id = NEXT_TEMP_ID.fetch_add(1, Ordering::Relaxed);
        let temporary = parent.join(format!(".{base}.pulp-{id}.tmp"));
        match OpenOptions::new()
            .create_new(true)
            .read(true)
            .write(true)
            .open(&temporary)
        {
            Ok(file) => return Ok((temporary, file)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error.into()),
        }
    }
    Err(ArchiveError::Io(std::io::Error::new(
        std::io::ErrorKind::AlreadyExists,
        "could not allocate a unique temporary file",
    )))
}

fn canonicalize_or_append(path: &Path) -> ArchiveResult<PathBuf> {
    let absolute = if path.is_absolute() {
        path.to_owned()
    } else {
        std::env::current_dir()?.join(path)
    };
    let mut missing = Vec::new();
    let mut cursor = absolute.clone();
    while fs::symlink_metadata(&cursor).is_err() {
        let Some(name) = cursor.file_name() else {
            return Ok(absolute);
        };
        missing.push(name.to_owned());
        if !cursor.pop() {
            return Ok(absolute);
        }
    }
    let mut result = fs::canonicalize(cursor)?;
    for component in missing.iter().rev() {
        result.push(component);
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::io::Read as _;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::{FileSystemSource, FileVolumeProvider, resolve_split_archive_path};
    use crate::{EntrySource, OperationContext, VolumeProvider};

    fn temp_root(label: &str) -> std::path::PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be after epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("pulp-{label}-{}-{nanos}", std::process::id()))
    }

    #[test]
    fn multi_root_source_is_sorted_and_uses_each_basename() {
        let root = temp_root("multi-root");
        fs::create_dir_all(&root).expect("test root should be created");
        let second = root.join("zeta.txt");
        let first = root.join("alpha.txt");
        fs::write(&second, b"z").expect("second source should be written");
        fs::write(&first, b"a").expect("first source should be written");

        let mut source =
            FileSystemSource::from_paths([second, first]).expect("sources should be accepted");
        let context = OperationContext::new();
        let names = [source.next(&context), source.next(&context)]
            .into_iter()
            .map(|entry| {
                entry
                    .expect("source read should succeed")
                    .expect("entry should exist")
            })
            .map(|entry| entry.name.to_string())
            .collect::<Vec<_>>();
        assert_eq!(names, ["alpha.txt", "zeta.txt"]);

        fs::remove_dir_all(root).expect("test root should be removed");
    }

    #[test]
    fn duplicate_multi_root_names_are_rejected_before_iteration() {
        let root = temp_root("duplicate-root");
        let left = root.join("left");
        let right = root.join("right");
        fs::create_dir_all(&left).expect("left should be created");
        fs::create_dir_all(&right).expect("right should be created");
        let left_file = left.join("same.txt");
        let right_file = right.join("same.txt");
        fs::write(&left_file, b"left").expect("left source should be written");
        fs::write(&right_file, b"right").expect("right source should be written");

        assert!(FileSystemSource::from_paths([left_file, right_file]).is_err());
        fs::remove_dir_all(root).expect("test root should be removed");
    }

    #[test]
    fn split_volume_entry_points_resolve_to_the_primary_file() {
        let root = temp_root("split-path");
        fs::create_dir_all(&root).expect("test root should be created");
        for name in ["data.7z.001", "data.7z.002", "data.zip", "data.z01"] {
            fs::write(root.join(name), []).expect("volume should be written");
        }
        fs::write(root.join("backup.part1.rar"), []).expect("volume should be written");
        fs::write(root.join("backup.part2.rar"), []).expect("volume should be written");

        assert_eq!(
            resolve_split_archive_path(root.join("data.7z.002")),
            root.join("data.7z.001")
        );
        assert_eq!(
            resolve_split_archive_path(root.join("data.z01")),
            root.join("data.zip")
        );
        assert_eq!(
            resolve_split_archive_path(root.join("backup.part2.rar")),
            root.join("backup.part1.rar")
        );

        fs::remove_dir_all(root).expect("test root should be removed");
    }

    #[test]
    fn volume_provider_opens_only_a_sibling_file() {
        let root = temp_root("volume-provider");
        fs::create_dir_all(&root).expect("test root should be created");
        let archive = root.join("data.zip");
        let volume = root.join("data.z01");
        fs::write(&archive, []).expect("archive should be written");
        fs::write(&volume, b"volume").expect("volume should be written");

        let provider = FileVolumeProvider::new(&archive);
        let mut reader = provider
            .open_volume("data.z01")
            .expect("sibling volume should open");
        let mut content = String::new();
        reader
            .read_to_string(&mut content)
            .expect("volume should be readable");
        assert_eq!(content, "volume");
        assert!(provider.open_volume("../data.z01").is_err());

        fs::remove_dir_all(root).expect("test root should be removed");
    }
}
