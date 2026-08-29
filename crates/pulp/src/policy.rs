/// Policy for symbolic and hard links encountered during extraction.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LinkPolicy {
    /// Reject links by default.
    Reject,
    /// Preserve links after validating their targets.
    Preserve,
}

/// Policy for special files such as devices and sockets.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SpecialFilePolicy {
    /// Reject special files.
    Reject,
    /// Preserve special files where the caller explicitly allows it.
    Preserve,
}

/// Policy for timestamps, modes and other optional metadata.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MetadataPolicy {
    /// Do not restore metadata.
    Ignore,
    /// Restore safe regular-file and directory metadata.
    RestoreSafe,
}

/// The default extraction safety policy.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExtractionPolicy {
    /// Link handling.
    pub links: LinkPolicy,
    /// Special-file handling.
    pub special_files: SpecialFilePolicy,
    /// Metadata handling.
    pub metadata: MetadataPolicy,
}

impl Default for ExtractionPolicy {
    fn default() -> Self {
        Self {
            links: LinkPolicy::Reject,
            special_files: SpecialFilePolicy::Reject,
            metadata: MetadataPolicy::RestoreSafe,
        }
    }
}
