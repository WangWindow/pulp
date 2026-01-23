//! Entry context menu UI rules.
//!
//! 目标：将“根据 EntryRow 决定右键菜单有哪些项”的规则放在 domain 层，降低 components 耦合。
//! - 不做 IO
//! - 不更新状态机
//! - 只把“显示/分组/触发消息”的规则结构化输出为 `Vec<MenuSpecItem>`
//!
//! 说明：
//! - 该规则输出纯数据 `MenuSpecItem`，由组件层映射为具体菜单渲染结构（例如 `MenuEntry`）。

use super::menu_spec::{MenuIcon, MenuSpecItem};
use crate::domain::{ContextAction, EntryRow, EntrySource};

/// 为单个条目生成右键菜单项列表。
///
/// 分组（通过 `MenuSpecItem::Separator` 分隔）与旧行为保持一致：
/// - 打开
/// -（可选）解压：仅文件系统中的“归档容器文件”
/// -（可选）压缩：仅文件系统
/// -（可选）文件操作：仅文件系统（重命名/删除）
/// - 属性
pub fn entry_context_menu_entries(row: &EntryRow) -> Vec<MenuSpecItem> {
    let is_fs = matches!(row.source, EntrySource::FileSystem);
    let is_archive_file = is_fs && pulp_core::ArchiveFormat::from_path(&row.path).is_some();

    let mut items: Vec<MenuSpecItem> = Vec::new();

    // “打开”
    items.push(MenuSpecItem::item(
        "files.context.open",
        if row.is_dir {
            MenuIcon::Folder
        } else {
            MenuIcon::File
        },
        ContextAction::Open,
    ));

    // “解压”（仅归档文件：FileSystem 里的容器文件）
    if is_archive_file {
        items.push(MenuSpecItem::separator());
        items.push(MenuSpecItem::item(
            "files.context.extract_smart",
            MenuIcon::Archive,
            ContextAction::SmartExtract,
        ));
        items.push(MenuSpecItem::item(
            "files.context.extract_to",
            MenuIcon::Archive,
            ContextAction::ExtractTo,
        ));
    }

    // “压缩”（仅文件系统）
    if is_fs {
        items.push(MenuSpecItem::separator());
        items.push(MenuSpecItem::item(
            "files.context.compress_zip",
            MenuIcon::Archive,
            ContextAction::CompressZip,
        ));
    }

    // “文件操作”（仅文件系统）
    if is_fs {
        items.push(MenuSpecItem::separator());
        items.push(MenuSpecItem::item(
            "files.context.rename",
            MenuIcon::File,
            ContextAction::Rename,
        ));
        items.push(MenuSpecItem::item(
            "files.context.delete",
            MenuIcon::File,
            ContextAction::Delete,
        ));
    }

    // “属性”
    items.push(MenuSpecItem::separator());
    items.push(MenuSpecItem::item(
        "files.context.properties",
        MenuIcon::File,
        ContextAction::Properties,
    ));

    items
}
