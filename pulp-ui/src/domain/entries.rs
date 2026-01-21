//! 文件/压缩包条目与统一渲染模型。

use std::path::PathBuf;

/// 行数据来源（真实文件系统 / 压缩包虚拟视图）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EntrySource {
    FileSystem,
    /// 压缩包预览：`archive_path` 是实际压缩包文件路径。
    Archive {
        archive_path: PathBuf,
    },
}

/// 通用“行模型”：文件系统与压缩包预览统一使用这一个结构渲染。
///
/// 关键点：
/// - `path` 始终是“可识别的路径字符串载体”：
///   - 文件系统：就是磁盘路径；
///   - 压缩包：使用虚拟前缀：`archive.zip::/src/main.rs`。
/// - `depth` 用于树状列表缩进；平铺视图中固定为 0。
#[derive(Debug, Clone)]
pub struct EntryRow {
    pub display_name: String,
    pub path: PathBuf,
    pub is_dir: bool,
    pub depth: usize,

    // 可选的“表格列”信息：平铺和树状都能复用同一套列展示。
    pub kind: String,
    pub size: Option<u64>,
    pub modified: Option<std::time::SystemTime>,

    pub source: EntrySource,
}

/// 文件系统条目（原始信息）：用于 IO、排序、过滤。
#[derive(Clone, Debug)]
pub struct FileEntry {
    pub name: String,
    pub size: Option<u64>,
    pub kind: String,
    pub modified: Option<std::time::SystemTime>,
    pub checked: bool,
    pub is_dir: bool,
    pub is_archive: bool,
    pub path: PathBuf,
}
