use crate::domain::FileEntry;
use pulp_core::ArchiveFormat;
use rust_i18n::t;
use std::path::PathBuf;

pub fn apply_filter(entries: Vec<FileEntry>, filter: &str) -> Vec<FileEntry> {
    let query = filter.trim().to_lowercase();
    if query.is_empty() {
        return entries;
    }
    entries
        .into_iter()
        .filter(|entry| entry.name.to_lowercase().contains(&query))
        .collect()
}

/// 加载目录：仅返回当前目录下的条目列表。
///
/// 说明：
/// - 旧的左侧“目录树”已移除，因此不再构造/返回 `FileNode` 或 children tree；
/// - “树状展开列表”会在文件显示区域按需加载，这里先保持最小职责（列出当前目录）。
pub async fn load_directory(path: PathBuf) -> (PathBuf, Vec<FileEntry>) {
    let mut entries = Vec::new();

    if let Ok(read_dir) = std::fs::read_dir(&path) {
        for item in read_dir.flatten() {
            let file_path = item.path();
            let name = item.file_name().to_string_lossy().to_string();
            let metadata = item.metadata().ok();
            let is_dir = metadata.as_ref().map(|m| m.is_dir()).unwrap_or(false);
            let size = metadata
                .as_ref()
                .and_then(|m| (!m.is_dir()).then_some(m.len()));
            let modified = metadata.and_then(|m| m.modified().ok());
            let is_archive = !is_dir && ArchiveFormat::from_path(&file_path).is_some();
            let kind = if is_dir {
                t!("fs.kind.folder").to_string()
            } else {
                file_path
                    .extension()
                    .and_then(|ext| ext.to_str())
                    .map(|ext| ext.to_uppercase())
                    .unwrap_or_else(|| t!("fs.kind.file").to_string())
            };

            entries.push(FileEntry {
                name,
                size,
                kind,
                modified,
                checked: false,
                is_dir,
                is_archive,
                path: file_path,
            });
        }
    }

    entries.sort_by(|a, b| {
        b.is_dir
            .cmp(&a.is_dir)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });

    (path, entries)
}

pub async fn create_folder(parent: PathBuf, name: String) -> Result<PathBuf, String> {
    if name.trim().is_empty() {
        return Err(t!("fs.error.empty_folder_name").to_string());
    }

    let mut candidate = parent.join(name.trim());
    if candidate.exists() {
        // 简单的冲突处理：追加 (n)
        let base = candidate
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| t!("fs.default_new_folder_name").to_string());
        for i in 1..=1000 {
            let alt = parent.join(format!("{base} ({i})"));
            if !alt.exists() {
                candidate = alt;
                break;
            }
        }
    }

    tokio::fs::create_dir_all(&candidate)
        .await
        .map_err(|e| e.to_string())?;
    Ok(candidate)
}

pub async fn rename_path(from: PathBuf, to: PathBuf) -> Result<PathBuf, String> {
    if from == to {
        return Ok(to);
    }
    tokio::fs::rename(&from, &to)
        .await
        .map_err(|e| e.to_string())?;
    Ok(to)
}

pub async fn delete_path(target: PathBuf) -> Result<(), String> {
    let md = tokio::fs::metadata(&target)
        .await
        .map_err(|e| e.to_string())?;
    if md.is_dir() {
        tokio::fs::remove_dir_all(&target)
            .await
            .map_err(|e| e.to_string())?;
    } else {
        tokio::fs::remove_file(&target)
            .await
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}
