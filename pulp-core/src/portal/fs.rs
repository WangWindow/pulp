//! 文件系统操作（对外门面）。
//!
//! 说明：
//! - 只封装“通用且安全”的基础 IO；
//! - 不做 UI 语义，不依赖 i18n；
//! - 复杂批量操作由上层组合。

use crate::domain::{FileEntry, WalkOptions};
use crate::portal::error::{PulpError, Result};
use std::path::{Path, PathBuf};

use walkdir::WalkDir;
/// 列出目录（非递归）。
pub async fn list_dir(path: PathBuf) -> Result<Vec<FileEntry>> {
    let mut out = Vec::new();
    let mut rd = tokio::fs::read_dir(&path).await.map_err(PulpError::from)?;

    while let Some(entry) = rd.next_entry().await.map_err(PulpError::from)? {
        let p = entry.path();
        let md = entry.metadata().await.map_err(PulpError::from)?;
        out.push(FileEntry::from_path_and_metadata(p, md));
    }

    // 排序：目录在前，名称按字母序
    out.sort_by(|a, b| {
        b.is_dir
            .cmp(&a.is_dir)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });

    Ok(out)
}

/// 递归列出目录。
pub async fn list_dir_recursive(path: PathBuf, options: WalkOptions) -> Result<Vec<FileEntry>> {
    tokio::task::spawn_blocking(move || {
        let mut entries = Vec::new();

        let mut walker = WalkDir::new(&path)
            .follow_links(options.follow_links)
            .min_depth(options.min_depth);

        if let Some(max_depth) = options.max_depth {
            walker = walker.max_depth(max_depth);
        }

        for entry in walker
            .into_iter()
            .filter_entry(|entry| options.include_hidden || !is_hidden(entry))
        {
            let entry = match entry {
                Ok(entry) => entry,
                Err(_) => continue,
            };
            let metadata = match entry.metadata() {
                Ok(metadata) => metadata,
                Err(_) => continue,
            };
            entries.push(FileEntry::from_path_and_metadata(
                entry.path().to_path_buf(),
                metadata,
            ));
        }

        entries.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(entries)
    })
    .await
    .map_err(|error| PulpError::backend("fs", error.to_string()))?
}

fn is_hidden(entry: &walkdir::DirEntry) -> bool {
    entry
        .file_name()
        .to_str()
        .map(|name| name.starts_with('.'))
        .unwrap_or(false)
}

/// 创建目录（递归）。
pub async fn create_dir(path: PathBuf) -> Result<()> {
    tokio::fs::create_dir_all(path)
        .await
        .map_err(PulpError::from)
}

/// 重命名/移动。
pub async fn rename(from: PathBuf, to: PathBuf) -> Result<()> {
    tokio::fs::rename(&from, &to).await.map_err(PulpError::from)
}

/// 删除路径（文件或目录）。
pub async fn remove_path(target: PathBuf) -> Result<()> {
    let md = tokio::fs::metadata(&target)
        .await
        .map_err(PulpError::from)?;
    if md.is_dir() {
        tokio::fs::remove_dir_all(&target)
            .await
            .map_err(PulpError::from)
    } else {
        tokio::fs::remove_file(&target)
            .await
            .map_err(PulpError::from)
    }
}

/// 判断路径是否存在。
pub fn exists(path: &Path) -> bool {
    path.exists()
}
