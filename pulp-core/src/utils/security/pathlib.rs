//! 路径安全与检测工具（ZipSlip 防护）。
//!
//! 目标：把压缩包内路径安全映射到目标目录下，防止目录穿越。
//!
//! 说明：
//! - 该模块是 core 级别的通用工具；
//! - 不依赖具体压缩格式；
//! - 不做磁盘 IO，仅做路径判定与映射。

use std::path::{Component, Path, PathBuf};

/// 解压选项（仅包含与路径映射相关的字段）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExtractPathOptions {
    /// 是否保留压缩包内路径结构。
    pub preserve_paths: bool,
    /// 剥离前 N 级路径（类似 tar --strip-components）。
    pub strip_components: Option<usize>,
}

impl Default for ExtractPathOptions {
    fn default() -> Self {
        Self {
            preserve_paths: true,
            strip_components: None,
        }
    }
}

/// 错误：路径不安全/不合法。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PathSafetyError {
    /// 路径包含 NUL 字节。
    NulByte,
    /// 绝对路径（Unix/Windows/UNC）。
    AbsolutePath,
    /// 路径穿越（含 `..`）。
    Traversal,
    /// 剥离组件后为空。
    EmptyAfterStrip,
    /// preserve_paths=false 时缺少有效文件名。
    MissingFileName,
}

/// 构建安全的输出路径。
///
/// 返回：
/// - `Ok(Some(path))`：该条目可安全写入
/// - `Ok(None)`：可恢复跳过
/// - `Err(err)`：非法路径
pub fn build_output_path(
    dest_dir: &Path,
    entry_path: &str,
    opts: &ExtractPathOptions,
) -> Result<Option<PathBuf>, PathSafetyError> {
    if entry_path.as_bytes().iter().any(|b| *b == 0) {
        return Err(PathSafetyError::NulByte);
    }

    let normalized = entry_path.replace('\\', "/");

    if normalized.starts_with('/') {
        return Err(PathSafetyError::AbsolutePath);
    }

    let mut parts: Vec<&str> = normalized.split('/').collect();
    while parts.last().is_some_and(|s| s.is_empty()) {
        parts.pop();
    }

    if let Some(n) = opts.strip_components {
        if n > 0 {
            if parts.len() <= n {
                return Ok(None);
            }
            parts.drain(0..n);
        }
    }

    if parts.is_empty() {
        return Ok(None);
    }

    let effective_parts: Vec<&str> = if opts.preserve_paths {
        parts
    } else {
        let last = parts.last().copied().unwrap_or_default();
        if last.is_empty() {
            return Err(PathSafetyError::MissingFileName);
        }
        vec![last]
    };

    let rel = safe_relative_path(&effective_parts)?;
    Ok(Some(dest_dir.join(rel)))
}

/// 将路径组件构造成安全的相对路径。
fn safe_relative_path(parts: &[&str]) -> Result<PathBuf, PathSafetyError> {
    let mut rel = PathBuf::new();

    for raw in parts {
        let seg = raw.trim();
        if seg.is_empty() || seg == "." {
            continue;
        }

        let p = Path::new(seg);
        for c in p.components() {
            match c {
                Component::Prefix(_) => return Err(PathSafetyError::AbsolutePath),
                Component::RootDir => return Err(PathSafetyError::AbsolutePath),
                Component::ParentDir => return Err(PathSafetyError::Traversal),
                Component::CurDir => {}
                Component::Normal(n) => {
                    if n == ".." {
                        return Err(PathSafetyError::Traversal);
                    }
                    rel.push(n);
                }
            }
        }
    }

    if rel.as_os_str().is_empty() {
        return Err(PathSafetyError::EmptyAfterStrip);
    }

    Ok(rel)
}

/// 判断 entry path 是否“看起来安全”。
pub fn is_safe_entry_path(entry_path: &str, opts: &ExtractPathOptions) -> bool {
    build_output_path(Path::new("."), entry_path, opts)
        .ok()
        .flatten()
        .is_some()
}
