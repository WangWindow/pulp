use crate::format::LicenseNotice;

/// Identity and diagnostics for an archive engine instance.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EngineInfo {
    /// Engine name.
    pub name: String,
    /// Provider version when available.
    pub version: Option<String>,
    /// Loaded library path when a native provider is used.
    pub library_path: Option<String>,
    /// License notice for the engine/provider.
    pub license: LicenseNotice,
    /// Diagnostics retained during loading.
    pub diagnostics: Vec<String>,
}
