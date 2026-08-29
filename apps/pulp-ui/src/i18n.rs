//! YAML-backed localization for the desktop application.

use std::collections::HashMap;
use std::env;
use std::sync::Arc;

use serde::Deserialize;

/// Locales shipped with Pulp.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Locale {
    /// English, used as the fallback locale.
    #[default]
    En,
    /// Simplified Chinese.
    ZhCn,
}

impl Locale {
    /// Selects a locale from the process environment.
    #[must_use]
    pub fn from_system() -> Self {
        let language = env::var_os("LC_ALL")
            .or_else(|| env::var_os("LANG"))
            .map(|value| value.to_string_lossy().to_ascii_lowercase());
        if language.is_some_and(|value| value.starts_with("zh")) {
            Self::ZhCn
        } else {
            Self::En
        }
    }
}

/// Stable message identifiers used by views and menus.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum MessageKey {
    File,
    Edit,
    View,
    Help,
    AboutPulp,
    OpenArchive,
    NewArchive,
    ExtractTo,
    Test,
    CloseArchive,
    CancelOperation,
    SelectAll,
    OpenSettings,
    Back,
    RestoreDefaults,
    QuickExtract,
    StopOperation,
    SearchSettings,
    OpenOrCreateArchive,
    DropArchiveHere,
    Ready,
    NoArchiveOpen,
    OpeningArchive,
    ArchiveOpened,
    ExtractionCompleted,
    CreationCompleted,
    TestCompleted,
    OperationCancelled,
    Processing,
    Extracting,
    Creating,
    Testing,
    Name,
    Size,
    PackedSize,
    Type,
    Directory,
    FileEntry,
    SymbolicLink,
    HardLink,
    SpecialFile,
    Objects,
    EmptyArchive,
    General,
    Compression,
    Extraction,
    Security,
    Other,
    Theme,
    Language,
    System,
    Light,
    Dark,
    English,
    SimplifiedChinese,
    DefaultListMode,
    CompactLayout,
    Details,
    List,
    ShowFolderPane,
    ShowArchivePath,
    SmartExtraction,
    RestoreMetadata,
    OverwritePolicy,
    AskBeforeReplacing,
    ReplaceExisting,
    SkipExisting,
    RejectLinks,
    DefaultFormat,
    CompressionLevel,
    CompressionMethod,
    ProviderDefault,
    TestAfterCreate,
    SettingsSaved,
    SelectDestination,
    SourcePaths,
    PasswordRequired,
    PasswordPrompt,
    WrongPassword,
    Warning,
    Error,
}

impl MessageKey {
    const fn as_str(self) -> &'static str {
        match self {
            Self::File => "file",
            Self::Edit => "edit",
            Self::View => "view",
            Self::Help => "help",
            Self::AboutPulp => "about_pulp",
            Self::OpenArchive => "open_archive",
            Self::NewArchive => "new_archive",
            Self::ExtractTo => "extract_to",
            Self::Test => "test",
            Self::CloseArchive => "close_archive",
            Self::CancelOperation => "cancel_operation",
            Self::SelectAll => "select_all",
            Self::OpenSettings => "open_settings",
            Self::Back => "back",
            Self::RestoreDefaults => "restore_defaults",
            Self::QuickExtract => "quick_extract",
            Self::StopOperation => "stop_operation",
            Self::SearchSettings => "search_settings",
            Self::OpenOrCreateArchive => "open_or_create_archive",
            Self::DropArchiveHere => "drop_archive_here",
            Self::Ready => "ready",
            Self::NoArchiveOpen => "no_archive_open",
            Self::OpeningArchive => "opening_archive",
            Self::ArchiveOpened => "archive_opened",
            Self::ExtractionCompleted => "extraction_completed",
            Self::CreationCompleted => "creation_completed",
            Self::TestCompleted => "test_completed",
            Self::OperationCancelled => "operation_cancelled",
            Self::Processing => "processing",
            Self::Extracting => "extracting",
            Self::Creating => "creating",
            Self::Testing => "testing",
            Self::Name => "name",
            Self::Size => "size",
            Self::PackedSize => "packed_size",
            Self::Type => "type",
            Self::Directory => "directory",
            Self::FileEntry => "file_entry",
            Self::SymbolicLink => "symbolic_link",
            Self::HardLink => "hard_link",
            Self::SpecialFile => "special_file",
            Self::Objects => "objects",
            Self::EmptyArchive => "empty_archive",
            Self::General => "general",
            Self::Compression => "compression",
            Self::Extraction => "extraction",
            Self::Security => "security",
            Self::Other => "other",
            Self::Theme => "theme",
            Self::Language => "language",
            Self::System => "system",
            Self::Light => "light",
            Self::Dark => "dark",
            Self::English => "english",
            Self::SimplifiedChinese => "simplified_chinese",
            Self::DefaultListMode => "default_list_mode",
            Self::CompactLayout => "compact_layout",
            Self::Details => "details",
            Self::List => "list",
            Self::ShowFolderPane => "show_folder_pane",
            Self::ShowArchivePath => "show_archive_path",
            Self::SmartExtraction => "smart_extraction",
            Self::RestoreMetadata => "restore_metadata",
            Self::OverwritePolicy => "overwrite_policy",
            Self::AskBeforeReplacing => "ask_before_replacing",
            Self::ReplaceExisting => "replace_existing",
            Self::SkipExisting => "skip_existing",
            Self::RejectLinks => "reject_links",
            Self::DefaultFormat => "default_format",
            Self::CompressionLevel => "compression_level",
            Self::CompressionMethod => "compression_method",
            Self::ProviderDefault => "provider_default",
            Self::TestAfterCreate => "test_after_create",
            Self::SettingsSaved => "settings_saved",
            Self::SelectDestination => "select_destination",
            Self::SourcePaths => "source_paths",
            Self::PasswordRequired => "password_required",
            Self::PasswordPrompt => "password_prompt",
            Self::WrongPassword => "wrong_password",
            Self::Warning => "warning",
            Self::Error => "error",
        }
    }
}

/// A localized message catalog loaded from a YAML resource.
#[derive(Clone, Debug)]
pub struct I18n {
    messages: Arc<HashMap<String, String>>,
    fallback: Arc<HashMap<String, String>>,
}

impl Default for I18n {
    fn default() -> Self {
        Self::new(Locale::from_system())
    }
}

impl I18n {
    /// Loads one of the embedded locale files.
    #[must_use]
    pub fn new(locale: Locale) -> Self {
        let fallback = parse_catalog(include_str!("../locales/en.yml"));
        let messages = match locale {
            Locale::En => fallback.clone(),
            Locale::ZhCn => parse_catalog(include_str!("../locales/zh_cn.yml")),
        };
        Self {
            messages: Arc::new(messages),
            fallback: Arc::new(fallback),
        }
    }

    /// Resolves a message key, falling back to English if necessary.
    #[must_use]
    pub fn text(&self, key: MessageKey) -> String {
        self.messages
            .get(key.as_str())
            .or_else(|| self.fallback.get(key.as_str()))
            .cloned()
            .unwrap_or_else(|| key.as_str().replace('_', " "))
    }
}

fn parse_catalog(source: &str) -> HashMap<String, String> {
    #[derive(Deserialize)]
    struct Catalog(HashMap<String, String>);

    serde_yaml::from_str::<HashMap<String, String>>(source)
        .or_else(|_| serde_yaml::from_str::<Catalog>(source).map(|catalog| catalog.0))
        .expect("embedded locale catalog must be valid YAML")
}

#[cfg(test)]
mod tests {
    use super::{I18n, Locale, MessageKey};

    #[test]
    fn all_runtime_catalogs_have_core_messages() {
        for locale in [Locale::En, Locale::ZhCn] {
            let catalog = I18n::new(locale);
            assert!(!catalog.text(MessageKey::OpenArchive).is_empty());
        }
    }
}
