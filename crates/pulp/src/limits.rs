use std::fmt;

/// The resource category that exceeded a configured limit.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResourceLimitKind {
    /// Number of archive entries.
    Entries,
    /// One entry's unpacked byte count.
    EntryBytes,
    /// Total unpacked byte count.
    TotalBytes,
    /// Archive-relative path length.
    PathBytes,
    /// Recursion or nesting depth.
    Depth,
}

impl fmt::Display for ResourceLimitKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Entries => "entries",
            Self::EntryBytes => "entry-bytes",
            Self::TotalBytes => "total-bytes",
            Self::PathBytes => "path-bytes",
            Self::Depth => "depth",
        })
    }
}

/// Explicit safety limits applied by an operation context.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResourceLimits {
    /// Maximum number of entries processed in one operation.
    pub max_entries: u64,
    /// Maximum unpacked bytes accepted for one entry.
    pub max_entry_bytes: u64,
    /// Maximum unpacked bytes accepted for the whole operation.
    pub max_total_bytes: u64,
    /// Maximum archive-relative path length.
    pub max_path_bytes: usize,
    /// Maximum logical nesting depth.
    pub max_depth: usize,
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self {
            max_entries: 1_000_000,
            max_entry_bytes: 16 * 1024 * 1024 * 1024,
            max_total_bytes: 64 * 1024 * 1024 * 1024,
            max_path_bytes: 16 * 1024,
            max_depth: 1024,
        }
    }
}
