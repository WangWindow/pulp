use std::fmt;
use std::time::SystemTime;

const MAX_ENTRY_NAME_BYTES: usize = 16 * 1024;

/// A stable identifier for an entry within one archive operation.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct EntryId(u64);

impl EntryId {
    /// Creates an entry identifier from a numeric value.
    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    /// Returns the numeric representation of this identifier.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// A validated archive-relative entry name.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct EntryName(String);

impl EntryName {
    /// Parses and normalizes an archive-relative name.
    pub fn parse(value: impl AsRef<str>) -> Result<Self, EntryNameError> {
        let value = value.as_ref();
        if value.is_empty() {
            return Err(EntryNameError::Empty);
        }
        if value.as_bytes().contains(&0) {
            return Err(EntryNameError::Nul);
        }
        if value.starts_with('/') || value.starts_with('\\') {
            return Err(EntryNameError::Absolute);
        }

        let normalized = value.replace('\\', "/");
        let normalized = normalized.trim_end_matches('/');
        if normalized.is_empty() {
            return Err(EntryNameError::Empty);
        }
        if normalized.len() > MAX_ENTRY_NAME_BYTES {
            return Err(EntryNameError::TooLong {
                bytes: normalized.len(),
                maximum: MAX_ENTRY_NAME_BYTES,
            });
        }

        let first_component = normalized.split('/').next().unwrap_or_default();
        if first_component.as_bytes().get(1) == Some(&b':') {
            return Err(EntryNameError::DrivePrefix);
        }

        let mut components = Vec::new();
        for component in normalized.split('/') {
            match component {
                "" => return Err(EntryNameError::EmptyComponent),
                "." => return Err(EntryNameError::CurrentDirectory),
                ".." => return Err(EntryNameError::ParentTraversal),
                component => components.push(component),
            }
        }
        if components.is_empty() {
            return Err(EntryNameError::Empty);
        }
        Ok(Self(components.join("/")))
    }

    /// Alias for [`EntryName::parse`].
    pub fn new(value: impl AsRef<str>) -> Result<Self, EntryNameError> {
        Self::parse(value)
    }

    /// Returns the normalized archive-relative name.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Consumes the name and returns its normalized string.
    #[must_use]
    pub fn into_string(self) -> String {
        self.0
    }

    /// Returns the path components without converting them to host paths.
    pub fn components(&self) -> impl Iterator<Item = &str> {
        self.0.split('/')
    }
}

impl AsRef<str> for EntryName {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl fmt::Display for EntryName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl TryFrom<&str> for EntryName {
    type Error = EntryNameError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::parse(value)
    }
}

impl TryFrom<String> for EntryName {
    type Error = EntryNameError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(value)
    }
}

/// Explains why an archive entry name failed validation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EntryNameError {
    /// The name is empty or contains only separators.
    Empty,
    /// The name contains a NUL byte.
    Nul,
    /// The name is absolute or starts with a UNC separator.
    Absolute,
    /// The name contains a parent traversal component.
    ParentTraversal,
    /// The name contains a current-directory component.
    CurrentDirectory,
    /// The first component looks like a Windows drive prefix.
    DrivePrefix,
    /// The name contains an empty component between separators.
    EmptyComponent,
    /// The name exceeds the archive-relative name limit.
    TooLong {
        /// Actual UTF-8 byte length.
        bytes: usize,
        /// Maximum permitted UTF-8 byte length.
        maximum: usize,
    },
}

impl fmt::Display for EntryNameError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => formatter.write_str("entry name is empty"),
            Self::Nul => formatter.write_str("entry name contains a NUL byte"),
            Self::Absolute => formatter.write_str("entry name is absolute"),
            Self::ParentTraversal => formatter.write_str("entry name contains parent traversal"),
            Self::CurrentDirectory => {
                formatter.write_str("entry name contains a current-directory component")
            }
            Self::DrivePrefix => formatter.write_str("entry name contains a drive prefix"),
            Self::EmptyComponent => formatter.write_str("entry name contains an empty component"),
            Self::TooLong { bytes, maximum } => {
                write!(
                    formatter,
                    "entry name is {bytes} bytes, maximum is {maximum}"
                )
            }
        }
    }
}

impl std::error::Error for EntryNameError {}

/// The semantic kind of an archive entry.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EntryKind {
    /// A regular file with byte content.
    File,
    /// A directory entry.
    Directory,
    /// A symbolic link.
    Symlink,
    /// A hard link.
    Hardlink,
    /// A special device, socket or other non-regular entry.
    Special,
}

/// A checksum attached to an archive entry.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Checksum {
    /// The algorithm name as reported by the engine.
    pub algorithm: String,
    /// The raw checksum bytes in archive order.
    pub value: Vec<u8>,
}

/// Metadata for one archive entry.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArchiveEntry {
    /// Stable operation-local identifier.
    pub id: EntryId,
    /// Validated archive-relative name.
    pub name: EntryName,
    /// Semantic entry kind.
    pub kind: EntryKind,
    /// Unpacked size when the archive reports it.
    pub size: Option<u64>,
    /// Packed size when the archive reports it.
    pub packed_size: Option<u64>,
    /// Modification timestamp.
    pub modified: Option<SystemTime>,
    /// Access timestamp.
    pub accessed: Option<SystemTime>,
    /// Creation timestamp.
    pub created: Option<SystemTime>,
    /// Format-specific attributes.
    pub attributes: Option<u64>,
    /// POSIX mode and file-type bits when the archive exposes them.
    pub posix_attributes: Option<u64>,
    /// Optional checksum.
    pub checksum: Option<Checksum>,
    /// Whether the entry or its header is encrypted.
    pub encrypted: bool,
    /// Link target for a link entry.
    pub link_target: Option<String>,
    /// Compression method reported for this item.
    pub method: Option<String>,
}

impl ArchiveEntry {
    /// Creates an entry with no optional metadata.
    #[must_use]
    pub fn new(id: EntryId, name: EntryName, kind: EntryKind) -> Self {
        Self {
            id,
            name,
            kind,
            size: None,
            packed_size: None,
            modified: None,
            accessed: None,
            created: None,
            attributes: None,
            posix_attributes: None,
            checksum: None,
            encrypted: false,
            link_target: None,
            method: None,
        }
    }

    /// Creates a minimal regular-file entry.
    pub fn file(id: EntryId, name: EntryName, size: Option<u64>) -> Self {
        let mut entry = Self::new(id, name, EntryKind::File);
        entry.size = size;
        entry
    }

    /// Returns whether the entry has byte content that can be streamed.
    #[must_use]
    pub const fn has_content(&self) -> bool {
        matches!(self.kind, EntryKind::File)
    }
}
