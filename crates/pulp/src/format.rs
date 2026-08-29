use std::fmt;

/// A stable identifier for a runtime archive handler.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ArchiveFormatId(String);

impl ArchiveFormatId {
    /// Creates a normalized identifier.
    #[must_use]
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into().trim().to_ascii_lowercase())
    }

    /// Returns the identifier as text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ArchiveFormatId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl AsRef<str> for ArchiveFormatId {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl From<&str> for ArchiveFormatId {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

impl From<String> for ArchiveFormatId {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

/// A runtime capability exposed by an archive handler.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum FormatCapability {
    /// Metadata listing is supported.
    List,
    /// Extraction is supported.
    Extract,
    /// Data verification is supported.
    Test,
    /// New archive creation is supported.
    Create,
    /// Existing archive update is supported.
    Update,
    /// Password-protected archives are supported.
    Password,
    /// Header encryption is supported.
    HeaderEncryption,
    /// Solid archive mode is supported.
    Solid,
    /// Multi-volume archives are supported.
    MultiVolume,
    /// The handler accepts streaming input.
    StreamingInput,
    /// The handler produces streaming output.
    StreamingOutput,
    /// The handler requires or supports seekable input.
    SeekableInput,
    /// The handler requires or supports seekable output.
    SeekableOutput,
    /// The handler represents one decoded byte stream and may wrap another
    /// archive format.
    Transparent,
}

/// A method exposed by a runtime handler.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompressionMethod {
    /// Stable method identifier.
    pub id: String,
    /// Human-readable method name.
    pub name: String,
    /// Whether the loaded handler reports decoder support.
    pub can_decode: bool,
    /// Whether the loaded handler reports encoder support.
    pub can_encode: bool,
}

impl CompressionMethod {
    /// Creates a method descriptor.
    #[must_use]
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            can_decode: true,
            can_encode: true,
        }
    }
}

/// A byte signature used for content-based format detection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Signature {
    /// Offset at which the signature is expected.
    pub offset: u64,
    /// Bytes expected at that offset.
    pub bytes: Vec<u8>,
}

/// A license reference attached to an engine or runtime handler.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LicenseNotice {
    /// Stable notice identifier.
    pub id: String,
    /// Short description of the applicable license.
    pub summary: String,
    /// Canonical upstream URL.
    pub url: String,
    /// Whether redistribution is permitted under the referenced terms.
    pub redistributable: bool,
    /// Additional restrictions that callers should display.
    pub restrictions: Vec<String>,
}

impl LicenseNotice {
    /// Creates a concise license notice.
    #[must_use]
    pub fn new(id: impl Into<String>, summary: impl Into<String>, url: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            summary: summary.into(),
            url: url.into(),
            redistributable: true,
            restrictions: Vec::new(),
        }
    }
}

/// All runtime metadata needed to choose an archive operation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FormatDescriptor {
    /// Runtime handler identifier.
    pub id: ArchiveFormatId,
    /// Display name reported by the provider.
    pub name: String,
    /// Filename extensions accepted by the handler.
    pub extensions: Vec<String>,
    /// Additional extensions used when creating archives.
    pub add_extensions: Vec<String>,
    /// Content signatures reported by the provider.
    pub signatures: Vec<Signature>,
    /// Dynamic operation and stream capabilities.
    pub capabilities: Vec<FormatCapability>,
    /// Compression methods associated with this handler.
    pub methods: Vec<CompressionMethod>,
    /// Native class identifier, when available.
    pub class_id: Option<[u8; 16]>,
    /// Handler priority used for ambiguous detection.
    pub priority: u16,
    /// Unmapped provider properties retained for diagnostics.
    pub diagnostics: Vec<String>,
    /// Applicable third-party license notice.
    pub license: Option<LicenseNotice>,
}

impl FormatDescriptor {
    /// Creates a descriptor with a stable initial capability set.
    pub fn new(
        id: impl Into<ArchiveFormatId>,
        name: impl Into<String>,
        extensions: impl IntoIterator<Item = impl Into<String>>,
        capabilities: impl IntoIterator<Item = FormatCapability>,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            extensions: extensions.into_iter().map(Into::into).collect(),
            add_extensions: Vec::new(),
            signatures: Vec::new(),
            capabilities: capabilities.into_iter().collect(),
            methods: Vec::new(),
            class_id: None,
            priority: 0,
            diagnostics: Vec::new(),
            license: None,
        }
    }

    /// Returns whether a capability is exposed by the handler.
    #[must_use]
    pub fn supports(&self, capability: FormatCapability) -> bool {
        self.capabilities.contains(&capability)
    }

    /// Returns whether a path or extension string matches this descriptor.
    #[must_use]
    pub fn matches_extension(&self, hint: &str) -> bool {
        let hint = hint.trim().to_ascii_lowercase();
        let hint = hint.strip_prefix('.').unwrap_or(&hint);
        extension_matches(&self.extensions, hint) || extension_matches(&self.add_extensions, hint)
    }

    /// Returns whether a path matches one of the handler's primary extensions.
    #[must_use]
    pub fn matches_primary_extension(&self, hint: &str) -> bool {
        let hint = hint.trim().to_ascii_lowercase();
        let hint = hint.strip_prefix('.').unwrap_or(&hint);
        extension_matches(&self.extensions, hint)
    }

    /// Returns the first method with the requested identifier.
    #[must_use]
    pub fn method(&self, id: &str) -> Option<&CompressionMethod> {
        self.methods
            .iter()
            .find(|method| method.id.eq_ignore_ascii_case(id))
    }
}

fn extension_matches(extensions: &[String], hint: &str) -> bool {
    extensions.iter().any(|extension| {
        let extension = extension
            .trim()
            .trim_start_matches('.')
            .to_ascii_lowercase();
        !extension.is_empty()
            && extension != "*"
            && (hint == extension || hint.ends_with(&format!(".{extension}")))
    })
}
