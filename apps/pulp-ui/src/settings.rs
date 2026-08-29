//! Small, typed TOML preference model for the desktop application.

use std::env;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use pulp::{
    ExtractionPolicy, LinkPolicy, MetadataPolicy, OverwritePolicy, ResourceLimits,
    SpecialFilePolicy,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

const SCHEMA_VERSION: u32 = 1;

/// Complete persisted desktop configuration.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(default)]
pub struct SettingsFile {
    /// Schema version used to validate future migrations.
    pub schema_version: u32,
    /// Presentation preferences.
    pub ui: UiSettings,
    /// Archive creation defaults.
    pub archive: ArchiveSettings,
    /// Extraction behavior.
    pub extraction: ExtractionSettings,
    /// Safety limits and link policy.
    pub security: SecuritySettings,
}

impl Default for SettingsFile {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            ui: UiSettings::default(),
            archive: ArchiveSettings::default(),
            extraction: ExtractionSettings::default(),
            security: SecuritySettings::default(),
        }
    }
}

/// UI presentation preferences.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(default)]
pub struct UiSettings {
    /// Theme selection.
    pub theme: ThemePreference,
    /// Locale selection.
    pub language: LanguagePreference,
    /// Use shorter archive rows.
    pub compact_layout: bool,
    /// Keep the archive's host path in the breadcrumb.
    pub show_archive_path: bool,
    /// Show the resizable folder tree beside the entry list.
    pub show_folder_pane: bool,
    /// Initial list presentation.
    pub list_mode: ListMode,
}

impl Default for UiSettings {
    fn default() -> Self {
        Self {
            theme: ThemePreference::System,
            language: LanguagePreference::System,
            compact_layout: false,
            show_archive_path: true,
            show_folder_pane: true,
            list_mode: ListMode::Details,
        }
    }
}

/// Theme selected by the user.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemePreference {
    /// Follow the system appearance.
    #[default]
    System,
    /// Always use the light theme.
    Light,
    /// Always use the dark theme.
    Dark,
}

/// Language selected by the user.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LanguagePreference {
    /// Follow the system locale.
    #[default]
    System,
    /// Use English.
    English,
    /// Use Simplified Chinese.
    ZhCn,
}

/// Archive list layout.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ListMode {
    /// Show only names.
    List,
    /// Show name, unpacked size, packed size and type.
    #[default]
    Details,
}

/// Defaults for archive creation.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(default)]
pub struct ArchiveSettings {
    /// Handler identifier used when the output extension is not decisive.
    pub default_format: String,
    /// Optional provider compression method.
    pub compression_method: Option<String>,
    /// Compression level from 0 to 9.
    pub compression_level: u8,
    /// Verify the archive after creation.
    pub test_after_create: bool,
}

impl Default for ArchiveSettings {
    fn default() -> Self {
        Self {
            default_format: String::from("zip"),
            compression_method: None,
            compression_level: 5,
            test_after_create: true,
        }
    }
}

/// Extraction preferences.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(default)]
pub struct ExtractionSettings {
    /// Choose a safe, predictable directory from archive contents.
    pub smart: bool,
    /// Behavior when a destination already exists.
    pub overwrite: OverwriteSetting,
    /// Restore safe timestamps and POSIX modes.
    pub restore_metadata: bool,
}

impl Default for ExtractionSettings {
    fn default() -> Self {
        Self {
            smart: true,
            overwrite: OverwriteSetting::Error,
            restore_metadata: true,
        }
    }
}

/// Existing-path behavior during extraction.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum OverwriteSetting {
    /// Ask by refusing the operation until the caller chooses a policy.
    #[default]
    Error,
    /// Replace regular files and links.
    Replace,
    /// Leave existing paths untouched.
    Skip,
}

/// Extraction safety settings.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(default)]
pub struct SecuritySettings {
    /// Reject symbolic and hard links.
    pub reject_links: bool,
    /// Maximum number of entries.
    pub max_entries: u64,
    /// Maximum unpacked size of one entry.
    pub max_entry_bytes: u64,
    /// Maximum total unpacked bytes.
    pub max_total_bytes: u64,
    /// Maximum UTF-8 path bytes.
    pub max_path_bytes: u64,
    /// Maximum path depth.
    pub max_depth: u64,
}

impl Default for SecuritySettings {
    fn default() -> Self {
        Self {
            reject_links: true,
            max_entries: 1_000_000,
            max_entry_bytes: 16 * 1024 * 1024 * 1024,
            max_total_bytes: 64 * 1024 * 1024 * 1024,
            max_path_bytes: 16 * 1024,
            max_depth: 1024,
        }
    }
}

impl SettingsFile {
    /// Validates values consumed by the archive library.
    pub fn validate(&self) -> Result<(), SettingsError> {
        if self.schema_version != SCHEMA_VERSION {
            return Err(SettingsError::UnsupportedSchema(self.schema_version));
        }
        if self.archive.compression_level > 9 {
            return Err(SettingsError::InvalidValue(
                "archive.compression_level must be between 0 and 9".to_owned(),
            ));
        }
        if self.archive.default_format.trim().is_empty()
            || self
                .archive
                .default_format
                .chars()
                .any(|character| !character.is_ascii_alphanumeric() && character != '-')
        {
            return Err(SettingsError::InvalidValue(
                "archive.default_format must be a handler identifier".to_owned(),
            ));
        }
        if self.security.max_entries == 0
            || self.security.max_entry_bytes == 0
            || self.security.max_total_bytes == 0
            || self.security.max_path_bytes == 0
            || self.security.max_path_bytes > 16 * 1024
            || self.security.max_depth == 0
        {
            return Err(SettingsError::InvalidValue(
                "security limits must be positive and max_path_bytes must not exceed 16384"
                    .to_owned(),
            ));
        }
        Ok(())
    }

    /// Converts the settings into core resource limits.
    #[must_use]
    pub fn resource_limits(&self) -> ResourceLimits {
        ResourceLimits {
            max_entries: self.security.max_entries,
            max_entry_bytes: self.security.max_entry_bytes,
            max_total_bytes: self.security.max_total_bytes,
            max_path_bytes: self.security.max_path_bytes as usize,
            max_depth: self.security.max_depth as usize,
        }
    }

    /// Converts the settings into the core extraction policy.
    #[must_use]
    pub fn extraction_policy(&self) -> ExtractionPolicy {
        ExtractionPolicy {
            links: if self.security.reject_links {
                LinkPolicy::Reject
            } else {
                LinkPolicy::Preserve
            },
            special_files: SpecialFilePolicy::Reject,
            metadata: if self.extraction.restore_metadata {
                MetadataPolicy::RestoreSafe
            } else {
                MetadataPolicy::Ignore
            },
        }
    }

    /// Converts the saved overwrite preference into the core filesystem policy.
    #[must_use]
    pub const fn overwrite_policy(&self) -> OverwritePolicy {
        match self.extraction.overwrite {
            OverwriteSetting::Error => OverwritePolicy::Error,
            OverwriteSetting::Replace => OverwritePolicy::Replace,
            OverwriteSetting::Skip => OverwritePolicy::Skip,
        }
    }
}

/// Failure while reading or writing settings.
#[derive(Debug, Error)]
pub enum SettingsError {
    /// The settings file could not be read or written.
    #[error("settings I/O error: {0}")]
    Io(#[from] io::Error),
    /// The TOML document was invalid.
    #[error("settings parse error: {0}")]
    Parse(#[from] toml::de::Error),
    /// The settings value could not be serialized.
    #[error("settings serialization error: {0}")]
    Serialize(#[from] toml::ser::Error),
    /// A future schema was encountered.
    #[error("unsupported settings schema version: {0}")]
    UnsupportedSchema(u32),
    /// A setting violates its documented range.
    #[error("invalid settings value: {0}")]
    InvalidValue(String),
}

/// Returns the conventional XDG configuration path when available.
#[must_use]
pub fn default_settings_path() -> Option<PathBuf> {
    if let Some(config) = env::var_os("XDG_CONFIG_HOME") {
        let config = PathBuf::from(config);
        if config.is_absolute() {
            return Some(config.join("pulp/settings.toml"));
        }
    }
    env::var_os("HOME").map(|home| PathBuf::from(home).join(".config/pulp/settings.toml"))
}

/// Loads settings, using defaults when the file does not exist.
pub fn load_settings(path: &Path) -> Result<SettingsFile, SettingsError> {
    let contents = match fs::read_to_string(path) {
        Ok(contents) => contents,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Ok(SettingsFile::default());
        }
        Err(error) => return Err(error.into()),
    };
    let settings = toml::from_str::<SettingsFile>(&contents)?;
    settings.validate()?;
    Ok(settings)
}

/// Writes settings through a same-directory temporary file and rename.
pub fn save_settings_atomic(path: &Path, settings: &SettingsFile) -> Result<(), SettingsError> {
    settings.validate()?;
    let contents = toml::to_string_pretty(settings)?;
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;
    for sequence in 0..1024_u32 {
        let temporary = parent.join(format!(
            ".settings.toml-{}-{sequence}.tmp",
            std::process::id()
        ));
        let mut file = match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
        {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error.into()),
        };
        let result = (|| -> Result<(), SettingsError> {
            file.write_all(contents.as_bytes())?;
            file.flush()?;
            file.sync_all()?;
            fs::rename(&temporary, path)?;
            Ok(())
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        return result;
    }
    Err(SettingsError::Io(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "could not allocate a settings temporary file",
    )))
}

#[cfg(test)]
mod tests {
    use super::{SettingsFile, load_settings, save_settings_atomic};

    #[test]
    fn defaults_round_trip_through_toml() {
        let root = std::env::temp_dir().join(format!("pulp-settings-{}", std::process::id()));
        let path = root.join("settings.toml");
        std::fs::create_dir_all(&root).expect("test directory should be created");
        let original = SettingsFile::default();
        save_settings_atomic(&path, &original).expect("settings should save");
        assert_eq!(
            load_settings(&path).expect("settings should load"),
            original
        );
        std::fs::remove_dir_all(root).expect("test directory should be removed");
    }
}
