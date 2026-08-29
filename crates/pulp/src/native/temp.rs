//! Private temporary streams used when a 7-Zip handler cannot expose a child stream.

use std::fs::{File, OpenOptions, remove_file};
use std::io::{Seek, SeekFrom};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::{ArchiveError, ArchiveResult};

static NEXT_TEMP_ID: AtomicU64 = AtomicU64::new(0);

/// A private, automatically removed seekable archive stream.
pub struct TemporaryArchive {
    path: PathBuf,
    file: File,
}

impl TemporaryArchive {
    /// Creates a new process-owned temporary archive file.
    pub fn create() -> ArchiveResult<Self> {
        for _ in 0..32 {
            let id = NEXT_TEMP_ID.fetch_add(1, Ordering::Relaxed);
            let path =
                std::env::temp_dir().join(format!("pulp-archive-{}-{id}.tmp", std::process::id()));
            let mut options = OpenOptions::new();
            options.read(true).write(true).create_new(true);
            #[cfg(unix)]
            std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
            match options.open(&path) {
                Ok(file) => return Ok(Self { path, file }),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(ArchiveError::Io(error)),
            }
        }
        Err(ArchiveError::Internal(
            "could not allocate a unique temporary archive path".to_owned(),
        ))
    }

    /// Returns the seekable file used by the archive engine.
    pub fn file_mut(&mut self) -> &mut File {
        &mut self.file
    }

    /// Rewinds the stream before it is opened as the next archive layer.
    pub fn rewind(&mut self) -> ArchiveResult<()> {
        self.file.seek(SeekFrom::Start(0))?;
        Ok(())
    }
}

impl Drop for TemporaryArchive {
    fn drop(&mut self) {
        let _ = remove_file(&self.path);
    }
}
