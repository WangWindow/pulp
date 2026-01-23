//! 状态层的纯辅助函数：不持有 App，仅做转换与渲染拼装。

use crate::app::themes;
use crate::app::ui_rules::{
    entry_context_menu_entries, menu_spec::MenuIcon, menu_spec::MenuSpecItem,
};
use crate::components;
use crate::components::menus::{MenuEntry, MenuStyle};
use crate::domain::{EntryRow, EntrySource, FileEntry, LIST_OVERSCAN, LIST_ROW_HEIGHT_PX, Message};
use iced::widget::scrollable::Viewport;
use iced::widget::{button, container, row, text};
use iced::{Alignment, Element, Length};
use icondata::{RiArchive2BusinessLine, RiFile2DocumentLine, RiFolder2DocumentLine};
use rust_i18n::t;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;

/// 尝试从 `archive.zip::/path/to/file` 中解析出压缩包名与内部路径（以 `/` 开头）。
///
/// 说明：
/// - 这是 UI 层用于“虚拟文件视图”的简单解析器；
/// - 真实压缩包文件路径不从这里解析，而是来自 `EntrySource::Archive { archive_path }`；
/// - 我们只需要内部路径来做“进入目录（A 方案）”与“定位 children index”。
pub(super) fn parse_virtual_path(p: &PathBuf) -> Option<(String, String)> {
    let s = p.to_string_lossy();
    let (left, right) = s.split_once("::")?;
    let inner = if right.is_empty() { "/" } else { right };
    let inner_norm = if inner.starts_with('/') {
        inner.to_string()
    } else {
        format!("/{inner}")
    };
    Some((left.to_string(), inner_norm))
}

/// 将文件系统的 `FileEntry` 列表转换为统一渲染用的 `EntryRow`。
///
/// 关键点：
/// - 这是“统一渲染”的核心：view 层只看 `EntryRow`；
/// - 文件系统条目 `path` 直接使用真实磁盘路径；
/// - `depth` 在平铺视图里固定为 0（树状视图会在生成树状渲染行时覆盖）。
pub(super) fn build_rows_from_fs(entries: &[FileEntry]) -> Vec<EntryRow> {
    entries
        .iter()
        .map(|e| EntryRow {
            display_name: e.name.clone(),
            path: e.path.clone(),
            is_dir: e.is_dir,
            depth: 0,
            kind: e.kind.clone(),
            size: e.size,
            modified: e.modified,
            source: EntrySource::FileSystem,
        })
        .collect()
}

/// 从压缩包条目列表构建一个“用于树状展开的目录索引”。
///
/// 说明：
/// - 压缩包预览是“虚拟文件视图”，展开不应读磁盘；
/// - 我们用 `HashMap<虚拟目录路径, 直接子项列表>` 来表示树；
/// - 虚拟路径使用约定：`archive.zip::/src/main.rs`。
pub(super) fn build_archive_children_index(
    archive_name: &str,
    archive_path: &PathBuf,
    entries: &[pulp_core::ArchiveEntry],
) -> HashMap<PathBuf, Vec<EntryRow>> {
    let mut map: HashMap<PathBuf, Vec<EntryRow>> = HashMap::new();

    // 根目录：`archive.zip::/`
    let root = PathBuf::from(format!("{archive_name}::/"));
    map.entry(root.clone()).or_default();

    for e in entries {
        // 统一把内部路径变成 `/a/b/c` 形式
        let inner = if e.path.starts_with('/') {
            e.path.clone()
        } else {
            format!("/{}", e.path)
        };

        // 分段：["a","b","c"]
        let parts: Vec<&str> = inner.split('/').filter(|s| !s.is_empty()).collect();

        // 对于 `a/b/c`：需要确保
        // - 根 -> a(目录)
        // - a -> b(目录)
        // - b -> c(文件/目录)
        let mut dir_prefix = String::new(); // 形如 "" / "/a" / "/a/b"
        for (i, part) in parts.iter().enumerate() {
            let is_last = i == parts.len().saturating_sub(1);

            // 当前父目录虚拟路径
            let parent_virtual = if dir_prefix.is_empty() {
                format!("{archive_name}::/")
            } else {
                format!("{archive_name}::{dir_prefix}/")
            };
            let parent_path = PathBuf::from(parent_virtual);

            // 计算当前节点虚拟路径（目录以 `/` 结尾仅用于 index-key；行本身用不带结尾也可）
            let node_prefix = if dir_prefix.is_empty() {
                format!("/{part}")
            } else {
                format!("{dir_prefix}/{part}")
            };

            // 当前节点是否目录：
            // - 非最后段：一定是目录（路径中间节点）
            // - 最后段：取自 e.is_dir
            let node_is_dir = if !is_last { true } else { e.is_dir };

            let node_virtual_path = format!("{archive_name}::{node_prefix}");
            let node_row = EntryRow {
                display_name: (*part).to_string(),
                path: PathBuf::from(node_virtual_path),
                is_dir: node_is_dir,
                depth: 0,
                kind: if node_is_dir {
                    t!("fs.kind.folder").to_string()
                } else {
                    t!("fs.kind.file").to_string()
                },
                size: if node_is_dir { None } else { e.size },
                modified: e.modified,
                source: EntrySource::Archive {
                    archive_path: archive_path.clone(),
                },
            };

            let children = map.entry(parent_path.clone()).or_default();
            if !children.iter().any(|c| c.path == node_row.path) {
                children.push(node_row.clone());
            }

            if node_is_dir {
                let dir_key = PathBuf::from(format!("{archive_name}::{node_prefix}/"));
                map.entry(dir_key).or_default();
            }

            dir_prefix = node_prefix;
        }
    }

    map
}

/// 生成树状视图需要渲染的“扁平行”列表。
pub(super) fn build_tree_render_rows(
    roots: &[EntryRow],
    expanded: &HashSet<PathBuf>,
    children_index: &HashMap<PathBuf, Vec<EntryRow>>,
) -> Vec<EntryRow> {
    fn push_children(
        out: &mut Vec<EntryRow>,
        dir_key: &PathBuf,
        depth: usize,
        expanded: &HashSet<PathBuf>,
        children_index: &HashMap<PathBuf, Vec<EntryRow>>,
    ) {
        let Some(children) = children_index.get(dir_key) else {
            return;
        };

        for child in children {
            let mut row = child.clone();
            row.depth = depth;
            out.push(row.clone());

            if child.is_dir {
                let key = PathBuf::from(format!("{}/", child.path.display()));
                if expanded.contains(&key) {
                    push_children(out, &key, depth + 1, expanded, children_index);
                }
            }
        }
    }

    let mut out = Vec::new();

    for r in roots {
        let mut root_row = r.clone();
        root_row.depth = 0;
        out.push(root_row.clone());

        if r.is_dir {
            let key = PathBuf::from(format!("{}/", r.path.display()));
            if expanded.contains(&key) {
                push_children(&mut out, &key, 1, expanded, children_index);
            }
        }
    }

    out
}

/// 树状列表行的渲染（缩进 + 展开箭头）。
pub(super) fn tree_view_rows<'a>(
    tree_rows: &'a [EntryRow],
    expanded: &'a HashSet<PathBuf>,
    selected_path: &'a std::path::Path,
    viewport: Option<Viewport>,
    menu_style: MenuStyle,
    build_context_menu: fn(&EntryRow) -> Arc<Vec<MenuEntry<Message>>>,
) -> Element<'a, Message> {
    const INDENT_PX: f32 = 14.0;
    let config =
        crate::components::VirtualListConfig::new(LIST_ROW_HEIGHT_PX, LIST_OVERSCAN, viewport);

    crate::components::virtual_list::<_, Message, _>(tree_rows, config, move |r| {
        let is_dir = r.is_dir;

        let arrow: Element<'a, Message> = if is_dir {
            let key = PathBuf::from(format!("{}/", r.path.display()));
            let is_expanded = expanded.contains(&key);
            let glyph = if is_expanded { "▾" } else { "▸" };

            button(text(glyph).size(12))
                .padding([2, 6])
                .style(themes::styles::ghost_button_style)
                .on_press(Message::TreeToggle(key, !is_expanded))
                .into()
        } else {
            container(text("")).width(Length::Fixed(18.0)).into()
        };

        let name = text(r.display_name.clone()).size(13);

        let content = row![arrow, name]
            .spacing(6)
            .width(Length::Fill)
            .align_y(Alignment::Center);

        let is_selected = r.path.as_path() == selected_path;

        let base = button(content)
            .padding([6, 8])
            .width(Length::Fill)
            .height(Length::Fixed(LIST_ROW_HEIGHT_PX))
            .style(move |theme, status| {
                if is_selected {
                    themes::styles::list_row_selected_style(theme, status)
                } else {
                    themes::styles::list_row_style(theme, status)
                }
            })
            .on_press(Message::RowClicked(r.clone()));

        // 树视图右键菜单：由上层注入菜单构建逻辑，避免 helpers 直接依赖 i18n。
        let items = build_context_menu(&r);

        let indent_px = r.depth as f32 * INDENT_PX;
        let indented_row = row![
            container(text("")).width(Length::Fixed(indent_px)),
            container(components::context_dropdown(
                base.into(),
                items,
                Message::DismissContextMenu,
                menu_style,
            ))
            .width(Length::Fill),
        ]
        .spacing(0)
        .width(Length::Fill)
        .height(Length::Fixed(LIST_ROW_HEIGHT_PX))
        .align_y(Alignment::Center);

        indented_row.into()
    })
}

/// 将 domain 的菜单规格映射为具体的菜单条目（含 i18n 文案）。
pub(super) fn build_entry_context_menu(row: &EntryRow) -> Arc<Vec<MenuEntry<Message>>> {
    let items: Vec<MenuEntry<Message>> = entry_context_menu_entries(row)
        .into_iter()
        .map(|spec| match spec {
            MenuSpecItem::Item {
                label_key,
                icon,
                action,
            } => MenuEntry::item(
                t!(label_key).to_string(),
                map_menu_icon(icon),
                Message::ContextActionFor(action, row.clone()),
            ),
            MenuSpecItem::Separator => MenuEntry::separator(),
        })
        .collect();

    Arc::new(items)
}

fn map_menu_icon(icon: MenuIcon) -> icondata::Icon {
    // 中文注释：未知/不常用 icon 先回退为文件图标。
    match icon {
        MenuIcon::File => RiFile2DocumentLine,
        MenuIcon::Folder => RiFolder2DocumentLine,
        MenuIcon::Archive => RiArchive2BusinessLine,
    }
}
