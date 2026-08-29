use std::io::{Read, Seek, Write};

use crate::{ArchiveError, ArchiveResult};

/// A trait-object-friendly readable and seekable stream.
pub trait ReadSeek: Read + Seek {}

impl<T: Read + Seek + ?Sized> ReadSeek for T {}

/// Provides sibling streams requested by a split archive handler.
pub trait VolumeProvider: Send + Sync {
    /// Opens one provider-requested volume by its archive-relative name.
    fn open_volume(&self, name: &str) -> ArchiveResult<Box<dyn ReadSeek>>;

    /// Returns the primary archive name used by 7-Zip to derive volume names.
    fn archive_name(&self) -> Option<String> {
        None
    }
}

/// Default provider used when an operation has no volume resolver.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoVolumeProvider;

impl VolumeProvider for NoVolumeProvider {
    fn open_volume(&self, name: &str) -> ArchiveResult<Box<dyn ReadSeek>> {
        Err(ArchiveError::VolumeNotFound(name.to_owned()))
    }
}

/// A trait-object-friendly writable and seekable stream.
pub trait WriteSeek: Write + Seek {}

impl<T: Write + Seek + ?Sized> WriteSeek for T {}
