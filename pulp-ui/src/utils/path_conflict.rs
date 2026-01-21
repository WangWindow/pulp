//! 文件名冲突处理工具：用于生成“不会覆盖已存在文件”的新路径。

use rust_i18n::t;
use std::path::{Path, PathBuf};

/// 冲突解决策略。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConflictStrategy {
    /// `name (n).ext` 形式（推荐，符合大多数桌面文件管理器习惯）
    ParenthesizedNumber,
}

/// 冲突解决配置。
#[derive(Debug, Clone)]
pub struct ConflictOptions {
    pub strategy: ConflictStrategy,
    /// 最大尝试次数（包含第 1 次 `(1)`）。
    pub max_tries: usize,
}

impl Default for ConflictOptions {
    fn default() -> Self {
        Self {
            strategy: ConflictStrategy::ParenthesizedNumber,
            max_tries: 10_000,
        }
    }
}

/// 为一个目标路径生成“不冲突”的候选路径。
///
/// `target` 可以是任意路径（文件或目录），本函数仅做存在性检查与字符串拼接。
///
/// 示例：
/// - `/tmp/foo.zip` 不存在 -> 返回 `/tmp/foo.zip`
/// - `/tmp/foo.zip` 存在 -> 返回 `/tmp/foo (1).zip` 或更高序号
pub fn resolve_path_conflict(target: &Path) -> Result<PathBuf, String> {
    resolve_path_conflict_with(target, &ConflictOptions::default())
}

/// 同 `resolve_path_conflict`，允许自定义策略与最大尝试次数。
pub fn resolve_path_conflict_with(
    target: &Path,
    options: &ConflictOptions,
) -> Result<PathBuf, String> {
    if !target.exists() {
        return Ok(target.to_path_buf());
    }

    let parent = target.parent().unwrap_or_else(|| Path::new("."));
    let file_name = target
        .file_name()
        .ok_or_else(|| t!("path_conflict.invalid_target_missing_name").to_string())?
        .to_string_lossy()
        .to_string();

    let (stem, ext) = split_stem_ext(&file_name);

    if stem.trim().is_empty() {
        return Err(t!("path_conflict.invalid_target_empty_name").to_string());
    }

    match options.strategy {
        ConflictStrategy::ParenthesizedNumber => {
            for i in 1..=options.max_tries {
                let candidate_name = if ext.is_empty() {
                    format!("{stem} ({i})")
                } else {
                    format!("{stem} ({i}).{ext}")
                };
                let candidate = parent.join(candidate_name);
                if !candidate.exists() {
                    return Ok(candidate);
                }
            }
            Err(t!(
                "path_conflict.max_tries_exceeded",
                tries = options.max_tries
            )
            .to_string())
        }
    }
}

/// 生成压缩包默认名称：与输入文件夹/文件同名，并追加扩展名；若冲突则自动编号。
///
/// `input_name`：通常来自被压缩对象的 `file_name()`（例如 `foo` 或 `foo.txt`）
/// `dest_dir`：输出目录
/// `archive_ext`：压缩包扩展名（不带点），例如 `"zip"`
///
/// 行为：
/// - 目标基础名优先使用 `input_name` 的 stem（去掉原扩展），更贴合“压缩包名与原文件夹一致”的期望
/// - 如果 `input_name` 本身没有 stem（异常情况），则退回 `"archive"`
///
/// 示例：
/// - 输入目录 `foo/` -> `foo.zip`
/// - 输入文件 `bar.txt` -> `bar.zip`
/// - 若 `foo.zip` 已存在 -> `foo (1).zip`
pub fn suggest_archive_path(
    input_name: &str,
    dest_dir: &Path,
    archive_ext: &str,
) -> Result<PathBuf, String> {
    if archive_ext.trim().is_empty() {
        return Err(t!("path_conflict.archive_ext_empty").to_string());
    }

    let input_name = input_name.trim();
    let base = if input_name.is_empty() {
        "archive".to_string()
    } else {
        // 对文件：bar.txt => bar
        // 对目录：foo => foo
        // 对隐藏文件：.bashrc => .bashrc（file_stem 结果依平台/语义可能不同，这里做保守处理）
        let p = Path::new(input_name);
        p.file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| input_name.to_string())
    };

    let mut target = dest_dir.join(format!("{base}.{archive_ext}"));
    if !target.exists() {
        return Ok(target);
    }

    // 冲突：使用通用冲突处理解决
    target = resolve_path_conflict(&target)?;
    Ok(target)
}

/// 将文件名拆分为（stem, ext）。ext 不包含点。
///
/// - `foo.zip` -> (`foo`, `zip`)
/// - `foo` -> (`foo`, ``)
/// - `foo.tar.gz` -> (`foo.tar`, `gz`)（本工具用于“生成不冲突文件名”，不做多段扩展名语义）
fn split_stem_ext(file_name: &str) -> (String, String) {
    // 使用最后一个 '.' 分割
    if let Some((left, right)) = file_name.rsplit_once('.') {
        if left.is_empty() {
            // 形如 ".bashrc"：视为无扩展名
            return (file_name.to_string(), String::new());
        }
        return (left.to_string(), right.to_string());
    }
    (file_name.to_string(), String::new())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_works() {
        assert_eq!(
            split_stem_ext("foo.zip"),
            ("foo".to_string(), "zip".to_string())
        );
        assert_eq!(split_stem_ext("foo"), ("foo".to_string(), "".to_string()));
        assert_eq!(
            split_stem_ext(".bashrc"),
            (".bashrc".to_string(), "".to_string())
        );
        assert_eq!(
            split_stem_ext("foo.tar.gz"),
            ("foo.tar".to_string(), "gz".to_string())
        );
    }

    #[test]
    fn suggest_archive_base_uses_stem() {
        let dir = Path::new(".");
        let p = suggest_archive_path("bar.txt", dir, "zip").unwrap();
        assert!(p.file_name().unwrap().to_string_lossy().starts_with("bar."));
    }
}
