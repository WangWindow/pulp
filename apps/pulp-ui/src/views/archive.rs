//! Archive browser projection and rendering.

use std::{
    collections::{BTreeSet, HashSet},
    ops::Range,
    rc::Rc,
};

use gpui::{
    Context, InteractiveElement as _, IntoElement, MouseButton, Pixels, ScrollHandle, SharedString,
    UniformListScrollHandle, Window, div, prelude::*, px, uniform_list,
};
use gpui_component::button::ButtonVariants;
use gpui_component::resizable::{h_resizable, resizable_panel};
use gpui_component::scroll::{ScrollableElement as _, Scrollbar, ScrollbarShow};
use gpui_component::{ActiveTheme, InteractiveElementExt, h_flex};
use lucide_icons::Icon as LucideIcon;
use pulp::{ArchiveEntry, EntryId, EntryKind, EntryName};

use crate::archive::{self, LoadedArchive};
use crate::i18n::MessageKey;
use crate::settings::ListMode;
use crate::workspace::Workspace;

/// Renders the archive browser, including the draggable folder/list split.
pub fn render(
    workspace: &Workspace,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) -> impl IntoElement {
    let Some(archive) = workspace.archive() else {
        return render_welcome(workspace, cx).into_any_element();
    };
    let prefix = workspace.current_prefix().to_owned();
    let rows = visible_rows(archive, &prefix);
    let tree = folder_rows(archive, workspace.expanded_folders());
    let list = render_list(workspace, rows, window, cx);
    let content = if workspace.show_folder_pane() {
        h_resizable("archive-browser-split")
            .child(
                resizable_panel()
                    .size(px(260.))
                    .size_range(px(180.)..px(460.))
                    .child(render_tree(workspace, tree, window, cx)),
            )
            .child(
                resizable_panel()
                    .size_range(px(360.)..Pixels::MAX)
                    .child(list),
            )
            .into_any_element()
    } else {
        list
    };
    div()
        .size_full()
        .min_w_0()
        .min_h_0()
        .overflow_hidden()
        .flex()
        .flex_col()
        .child(render_breadcrumb(workspace, archive, &prefix, cx))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .min_h_0()
                .overflow_hidden()
                .child(content),
        )
        .into_any_element()
}

fn render_welcome(workspace: &Workspace, cx: &mut Context<Workspace>) -> gpui::Div {
    let open = workspace.action_button(
        "welcome-open",
        LucideIcon::FolderOpen,
        MessageKey::OpenArchive,
        true,
        cx,
    );
    let new = workspace.action_button(
        "welcome-new",
        LucideIcon::Plus,
        MessageKey::NewArchive,
        true,
        cx,
    );
    div()
        .size_full()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap_3()
        .text_color(cx.theme().muted_foreground)
        .child(icon(LucideIcon::FileArchive, 28.))
        .child(
            div()
                .text_lg()
                .child(workspace.text(MessageKey::OpenOrCreateArchive)),
        )
        .child(h_flex().gap_2().child(open).child(new))
        .child(
            div()
                .text_sm()
                .child(workspace.text(MessageKey::DropArchiveHere)),
        )
}

fn render_breadcrumb(
    workspace: &Workspace,
    archive: &LoadedArchive,
    prefix: &str,
    cx: &mut Context<Workspace>,
) -> gpui::AnyElement {
    let mut segments = Vec::new();
    let archive_label = if workspace.show_archive_path() {
        archive.path.display().to_string()
    } else {
        archive.path.file_name().map_or_else(
            || archive.path.display().to_string(),
            |name| name.to_string_lossy().into_owned(),
        )
    };
    segments.push((archive_label, String::new(), prefix.is_empty()));
    let mut current = String::new();
    for component in prefix.split('/').filter(|part| !part.is_empty()) {
        if !current.is_empty() {
            current.push('/');
        }
        current.push_str(component);
        segments.push((component.to_owned(), current.clone(), false));
    }
    let entity = cx.entity();
    div()
        .w_full()
        .h(px(38.))
        .flex_shrink_0()
        .px_2()
        .border_b_1()
        .border_color(cx.theme().border)
        .bg(cx.theme().list_head)
        .overflow_x_scrollbar()
        .child(
            h_flex().h_full().min_w(px(600.)).gap_1().children(
                segments
                    .into_iter()
                    .enumerate()
                    .map(move |(index, (label, target, current))| {
                        let entity = entity.clone();
                        let separator = (index > 0)
                            .then(|| div().text_color(cx.theme().muted_foreground).child("›"));
                        h_flex()
                            .gap_1()
                            .when_some(separator, |this, separator| this.child(separator))
                            .child(
                                gpui_component::button::Button::new(SharedString::from(format!(
                                    "breadcrumb-{index}"
                                )))
                                .text()
                                .child(label)
                                .when(current, |this| this.text_color(cx.theme().foreground))
                                .on_click(move |_, _, app| {
                                    entity.update(app, |workspace, cx| {
                                        workspace.navigate_to(target.clone(), cx);
                                    });
                                }),
                            )
                    }),
            ),
        )
        .into_any_element()
}

fn render_tree(
    workspace: &Workspace,
    rows: Vec<FolderRow>,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) -> gpui::AnyElement {
    let entity = cx.entity();
    let scroll_handle = window
        .use_keyed_state("archive-folder-tree-scroll", cx, |_, _| ScrollHandle::new())
        .read(cx)
        .clone();
    div()
        .id("archive-folder-tree")
        .size_full()
        .min_w_0()
        .relative()
        .border_r_1()
        .border_color(cx.theme().border)
        .bg(cx.theme().list)
        .child(
            div()
                .id("archive-folder-tree-scroll-area")
                .size_full()
                .track_scroll(&scroll_handle)
                .overflow_y_scroll()
                .min_w(px(220.))
                .p_1()
                .children(rows.into_iter().map(|row| {
                    tree_row(
                        &row.label,
                        &row.prefix,
                        row.depth,
                        row.expanded,
                        workspace.current_prefix() == row.prefix,
                        entity.clone(),
                        cx,
                    )
                })),
        )
        .child(
            div()
                .absolute()
                .top_0()
                .right_0()
                .bottom_0()
                .w(px(12.))
                .child(Scrollbar::vertical(&scroll_handle).scrollbar_show(ScrollbarShow::Always)),
        )
        .into_any_element()
}

fn tree_row(
    label: &str,
    prefix: &str,
    depth: usize,
    expanded: bool,
    active: bool,
    entity: gpui::Entity<Workspace>,
    cx: &mut Context<Workspace>,
) -> gpui::AnyElement {
    let prefix = prefix.to_owned();
    let target = prefix.clone();
    div()
        .id(SharedString::from(format!(
            "tree-{}",
            prefix.replace('/', "-")
        )))
        .h(px(28.))
        .w_full()
        .pl(px(6. + depth as f32 * 16.))
        .pr_2()
        .rounded(cx.theme().radius)
        .when(active, |this| this.bg(cx.theme().list_active))
        .hover(|this| this.bg(cx.theme().list_hover))
        .on_click(move |event, _, app| {
            let target = target.clone();
            entity.update(app, move |workspace, cx| {
                workspace.navigate_to(target.clone(), cx);
                if event.click_count() == 2 || !expanded {
                    workspace.toggle_folder(target, cx);
                }
            });
        })
        .child(
            h_flex()
                .gap_1()
                .child(icon(
                    if expanded {
                        LucideIcon::ChevronDown
                    } else {
                        LucideIcon::ChevronRight
                    },
                    14.,
                ))
                .child(icon(LucideIcon::Folder, 16.))
                .child(div().flex_1().truncate().child(label.to_owned())),
        )
        .into_any_element()
}

fn render_list(
    workspace: &Workspace,
    rows: Vec<DisplayRow>,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) -> gpui::AnyElement {
    let list_mode = workspace.list_mode();
    let compact_layout = workspace.settings().ui.compact_layout;
    let empty = rows.is_empty();
    let rows = Rc::new(rows);
    let scroll_handle = window
        .use_keyed_state("archive-file-list-scroll", cx, |_, _| {
            UniformListScrollHandle::new()
        })
        .read(cx)
        .clone();
    let row_count = rows.len();
    let row_data = rows.clone();
    let visible_names = Rc::new(
        rows.iter()
            .map(|row| row.entry.name.to_string())
            .collect::<Vec<_>>(),
    );
    let row_names = visible_names.clone();
    let virtual_rows = uniform_list(
        "archive-file-list-rows",
        row_count,
        cx.processor(move |workspace, visible_range: Range<usize>, _, cx| {
            let entity = cx.entity();
            visible_range
                .filter_map(|index| row_data.get(index).cloned().map(|row| (index, row)))
                .map(|(index, row)| {
                    let selected = workspace.is_selected(row.entry.name.as_str());
                    let kind_label = workspace.text(kind_message_key(row.entry.kind));
                    list_row(
                        row,
                        ListRowOptions {
                            index,
                            selected,
                            mode: list_mode,
                            compact_layout,
                            kind_label,
                            visible_names: row_names.clone(),
                            entity: entity.clone(),
                        },
                        cx,
                    )
                })
                .collect::<Vec<_>>()
        }),
    )
    .size_full()
    .track_scroll(scroll_handle.clone());
    let viewport = div()
        .id("archive-file-list-viewport")
        .flex_1()
        .min_h_0()
        .relative()
        .overflow_hidden()
        .when(!empty, |this| {
            this.child(virtual_rows).child(
                div()
                    .absolute()
                    .top_0()
                    .right_0()
                    .bottom_0()
                    .w(px(12.))
                    .child(
                        Scrollbar::vertical(&scroll_handle).scrollbar_show(ScrollbarShow::Always),
                    ),
            )
        })
        .when(empty, |this| {
            this.child(
                div()
                    .p_6()
                    .text_color(cx.theme().muted_foreground)
                    .child(workspace.text(MessageKey::EmptyArchive)),
            )
        });
    div()
        .id("archive-file-list")
        .size_full()
        .min_w_0()
        .min_h_0()
        .flex()
        .flex_col()
        .child(list_header(workspace, list_mode, cx).flex_shrink_0())
        .child(viewport)
        .into_any_element()
}

fn list_header(workspace: &Workspace, mode: ListMode, cx: &mut Context<Workspace>) -> gpui::Div {
    let mut header = h_flex()
        .h(px(32.))
        .px_3()
        .gap_2()
        .bg(cx.theme().list_head)
        .border_b_1()
        .border_color(cx.theme().border)
        .text_sm()
        .text_color(cx.theme().muted_foreground)
        .child(div().w(px(420.)).child(workspace.text(MessageKey::Name)));
    if mode == crate::settings::ListMode::Details {
        header = header
            .child(div().w(px(140.)).child(workspace.text(MessageKey::Size)))
            .child(
                div()
                    .w(px(140.))
                    .child(workspace.text(MessageKey::PackedSize)),
            )
            .child(div().w(px(120.)).child(workspace.text(MessageKey::Type)));
    }
    header
}

fn list_row(
    row: DisplayRow,
    options: ListRowOptions,
    cx: &mut Context<Workspace>,
) -> gpui::AnyElement {
    let ListRowOptions {
        index,
        selected,
        mode,
        compact_layout,
        kind_label,
        visible_names,
        entity,
    } = options;
    let is_directory = row.entry.kind == EntryKind::Directory;
    let click_name = row.entry.name.to_string();
    let select_entity = entity.clone();
    let drag_entity = entity.clone();
    let release_entity = entity.clone();
    let navigate_entity = entity;
    let select_name = click_name.clone();
    let drag_name = click_name.clone();
    let navigate_name = click_name;
    let select_names = visible_names.clone();
    let drag_names = visible_names.clone();
    let release_out_entity = release_entity.clone();
    let mut view = h_flex()
        .id(SharedString::from(archive::row_id(&row.entry)))
        .h(px(if compact_layout { 26. } else { 30. }))
        .px_3()
        .gap_2()
        .when(index.is_multiple_of(2), |this| {
            this.bg(cx.theme().list_even)
        })
        .when(selected, |this| this.bg(cx.theme().list_active))
        .hover(|this| this.bg(cx.theme().list_hover))
        .on_mouse_down(MouseButton::Left, move |event, _, app| {
            select_entity.update(app, |workspace, cx| {
                workspace.begin_selection(
                    select_name.clone(),
                    select_names.as_slice(),
                    event.modifiers,
                    cx,
                );
            });
        })
        .on_mouse_move(move |event, _, app| {
            if event.dragging() {
                drag_entity.update(app, |workspace, cx| {
                    workspace.extend_selection(&drag_name, drag_names.as_slice(), cx);
                });
            }
        })
        .on_mouse_up(MouseButton::Left, move |_, _, app| {
            release_entity.update(app, |workspace, cx| workspace.end_selection(cx));
        })
        .on_mouse_up_out(MouseButton::Left, move |_, _, app| {
            release_out_entity.update(app, |workspace, cx| workspace.end_selection(cx));
        })
        .on_double_click(move |_, _, app| {
            if is_directory {
                navigate_entity.update(app, |workspace, cx| {
                    workspace.navigate_to(navigate_name.clone(), cx);
                });
            }
        })
        .child(icon(entry_icon(row.entry.kind), 16.))
        .child(div().w(px(420.)).truncate().child(row.name));
    if mode == crate::settings::ListMode::Details {
        view = view
            .child(
                div()
                    .w(px(140.))
                    .text_color(cx.theme().muted_foreground)
                    .child(row.entry.size.map_or_else(|| "—".to_owned(), format_bytes)),
            )
            .child(
                div()
                    .w(px(140.))
                    .text_color(cx.theme().muted_foreground)
                    .child(
                        row.entry
                            .packed_size
                            .map_or_else(|| "—".to_owned(), format_bytes),
                    ),
            )
            .child(
                div()
                    .w(px(120.))
                    .text_color(cx.theme().muted_foreground)
                    .child(kind_label),
            );
    }
    view.into_any_element()
}

struct ListRowOptions {
    index: usize,
    selected: bool,
    mode: ListMode,
    compact_layout: bool,
    kind_label: String,
    visible_names: Rc<Vec<String>>,
    entity: gpui::Entity<Workspace>,
}

#[derive(Clone)]
struct DisplayRow {
    entry: ArchiveEntry,
    name: String,
}

#[derive(Clone)]
struct FolderRow {
    prefix: String,
    label: String,
    depth: usize,
    expanded: bool,
}

fn visible_rows(archive: &LoadedArchive, prefix: &str) -> Vec<DisplayRow> {
    let mut rows = Vec::new();
    let mut directories = BTreeSet::new();
    let prefix_with_separator = (!prefix.is_empty()).then(|| format!("{prefix}/"));
    for entry in &archive.entries {
        let name = entry.name.as_str();
        let Some(remainder) = prefix_with_separator
            .as_deref()
            .map_or(Some(name), |prefix| name.strip_prefix(prefix))
        else {
            continue;
        };
        if remainder.is_empty() {
            continue;
        }
        let component = remainder.split('/').next().unwrap_or_default();
        let full = if prefix.is_empty() {
            component.to_owned()
        } else {
            format!("{prefix}/{component}")
        };
        if remainder.contains('/') || entry.kind == EntryKind::Directory {
            directories.insert(full);
        } else {
            rows.push(DisplayRow {
                entry: entry.clone(),
                name: component.to_owned(),
            });
        }
    }
    rows.extend(directories.into_iter().filter_map(|name| {
        EntryName::new(&name).ok().map(|name_value| DisplayRow {
            entry: ArchiveEntry::new(EntryId::new(u64::MAX), name_value, EntryKind::Directory),
            name: name.rsplit('/').next().unwrap_or(&name).to_owned(),
        })
    }));
    rows.sort_by(|left, right| {
        (left.entry.kind != EntryKind::Directory)
            .cmp(&(right.entry.kind != EntryKind::Directory))
            .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
            .then_with(|| left.name.cmp(&right.name))
    });
    rows
}

fn folder_rows(archive: &LoadedArchive, expanded: &HashSet<String>) -> Vec<FolderRow> {
    let mut all = BTreeSet::new();
    for entry in &archive.entries {
        let mut prefix = String::new();
        let components = entry.name.as_str().split('/').collect::<Vec<_>>();
        let limit = if entry.kind == EntryKind::Directory {
            components.len()
        } else {
            components.len().saturating_sub(1)
        };
        for component in components.into_iter().take(limit) {
            if !prefix.is_empty() {
                prefix.push('/');
            }
            prefix.push_str(component);
            all.insert(prefix.clone());
        }
    }
    all.into_iter()
        .filter(|prefix| ancestors_expanded(prefix, expanded))
        .map(|prefix| FolderRow {
            depth: prefix.matches('/').count(),
            label: prefix.rsplit('/').next().unwrap_or(&prefix).to_owned(),
            expanded: expanded.contains(&prefix),
            prefix,
        })
        .collect()
}

fn ancestors_expanded(prefix: &str, expanded: &HashSet<String>) -> bool {
    let mut parent = String::new();
    for component in prefix.split('/').take(prefix.matches('/').count()) {
        if !parent.is_empty() {
            parent.push('/');
        }
        parent.push_str(component);
        if !expanded.contains(&parent) {
            return false;
        }
    }
    true
}

fn entry_icon(kind: EntryKind) -> LucideIcon {
    match kind {
        EntryKind::Directory => LucideIcon::Folder,
        EntryKind::Symlink | EntryKind::Hardlink => LucideIcon::Link,
        EntryKind::Special => LucideIcon::FileWarning,
        EntryKind::File => LucideIcon::File,
    }
}

fn kind_message_key(kind: EntryKind) -> MessageKey {
    match kind {
        EntryKind::Directory => MessageKey::Directory,
        EntryKind::File => MessageKey::FileEntry,
        EntryKind::Symlink => MessageKey::SymbolicLink,
        EntryKind::Hardlink => MessageKey::HardLink,
        EntryKind::Special => MessageKey::SpecialFile,
    }
}

fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024. && unit < UNITS.len() - 1 {
        value /= 1024.;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

fn icon(icon: LucideIcon, size: f32) -> gpui::Div {
    div()
        .w(px(size))
        .h(px(size))
        .flex()
        .items_center()
        .justify_center()
        .font_family("lucide")
        .text_size(px(size))
        .child(icon.unicode().to_string())
}
