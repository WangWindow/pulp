//! 文件系统条目模型（与 UI 无关）。

use crate::domain::ArchiveFormat;
use std::path::{Path, PathBuf};

/// 目录遍历选项（用于递归列出文件）。
#[derive(Debug, Clone)]
pub struct WalkOptions {
    /// 是否跟随符号链接。
    pub follow_links: bool,
    /// 最小深度（0 表示包含根本身）。
    pub min_depth: usize,
    /// 最大深度（None 表示不限制）。
    pub max_depth: Option<usize>,
    /// 是否包含隐藏文件（以 '.' 开头）。
    pub include_hidden: bool,
}

impl Default for WalkOptions {
    fn default() -> Self {
        Self {
            follow_links: false,
            min_depth: 1,
            max_depth: None,
            include_hidden: false,
        }
    }
}

/// 文件系统条目（文件/目录）元信息。
#[derive(Debug, Clone)]
pub struct FileEntry {
    pub name: String,
    pub path: PathBuf,
    pub is_dir: bool,
    pub size: Option<u64>,
    pub modified: Option<std::time::SystemTime>,
    /// 是否为压缩包（由扩展名推断）。
    pub is_archive: bool,
    /// 规范化后的扩展名（小写，不含点）。
    pub extension: Option<String>,
}

impl FileEntry {
    pub fn from_path_and_metadata(path: PathBuf, md: std::fs::Metadata) -> Self {
        let name = path
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        let is_dir = md.is_dir();
        let size = if is_dir { None } else { Some(md.len()) };
        let modified = md.modified().ok();
        let extension = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|s| s.to_lowercase());
        let is_archive = !is_dir && ArchiveFormat::from_path(&path).is_some();

        Self {
            name,
            path,
            is_dir,
            size,
            modified,
            is_archive,
            extension,
        }
    }

    pub fn is_hidden(&self) -> bool {
        self.name.starts_with('.')
    }

    pub fn matches_extension(&self, ext: &str) -> bool {
        let ext = ext.trim_start_matches('.').to_lowercase();
        self.extension.as_deref() == Some(ext.as_str())
    }

    pub fn is_same_dir(&self, dir: &Path) -> bool {
        self.path.parent().map(|p| p == dir).unwrap_or(false)
    }
}
