use std::{
    fmt,
    path::{Path, PathBuf},
    time::SystemTime,
};

/// 用户认知层面的压缩格式（与具体后端解耦）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ArchiveFormat {
    Zip,
    SevenZ,
    Rar,
    Tar,
    TarGz,
    TarBz2,
    TarXz,
}

impl ArchiveFormat {
    /// 基于文件扩展名推断格式（优先匹配复合扩展名）。
    pub fn from_path(path: &Path) -> Option<Self> {
        let name = path.file_name()?.to_string_lossy().to_lowercase();

        if name.ends_with(".tar.gz") || name.ends_with(".tgz") {
            return Some(Self::TarGz);
        }
        if name.ends_with(".tar.bz2") || name.ends_with(".tbz2") || name.ends_with(".tbz") {
            return Some(Self::TarBz2);
        }
        if name.ends_with(".tar.xz") || name.ends_with(".txz") {
            return Some(Self::TarXz);
        }

        match path
            .extension()
            .and_then(|e| e.to_str())
            .map(|s| s.to_lowercase())
        {
            Some(ext) if ext == "zip" => Some(Self::Zip),
            Some(ext) if ext == "7z" => Some(Self::SevenZ),
            Some(ext) if ext == "rar" => Some(Self::Rar),
            Some(ext) if ext == "tar" => Some(Self::Tar),
            _ => None,
        }
    }

    /// 返回首选扩展名（用于 UI 默认文件名建议）。
    pub fn preferred_extension(self) -> &'static str {
        match self {
            ArchiveFormat::Zip => "zip",
            ArchiveFormat::SevenZ => "7z",
            ArchiveFormat::Rar => "rar",
            ArchiveFormat::Tar => "tar",
            ArchiveFormat::TarGz => "tar.gz",
            ArchiveFormat::TarBz2 => "tar.bz2",
            ArchiveFormat::TarXz => "tar.xz",
        }
    }
}

impl fmt::Display for ArchiveFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            ArchiveFormat::Zip => "zip",
            ArchiveFormat::SevenZ => "7z",
            ArchiveFormat::Rar => "rar",
            ArchiveFormat::Tar => "tar",
            ArchiveFormat::TarGz => "tar.gz",
            ArchiveFormat::TarBz2 => "tar.bz2",
            ArchiveFormat::TarXz => "tar.xz",
        };
        write!(f, "{s}")
    }
}

/// 压缩包来源（当前仅支持文件路径）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArchiveSource {
    pub path: PathBuf,
    pub format_hint: Option<ArchiveFormat>,
}

impl ArchiveSource {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        let path = path.into();
        let format_hint = ArchiveFormat::from_path(&path);
        Self { path, format_hint }
    }

    pub fn with_format_hint(path: impl Into<PathBuf>, format_hint: ArchiveFormat) -> Self {
        Self {
            path: path.into(),
            format_hint: Some(format_hint),
        }
    }
}

/// 压缩包条目（文件/目录）元信息。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArchiveEntry {
    /// 压缩包内路径（统一使用 `/` 分隔）。
    pub path: String,
    pub is_dir: bool,
    pub size: Option<u64>,
    pub compressed_size: Option<u64>,
    pub modified: Option<SystemTime>,
}
