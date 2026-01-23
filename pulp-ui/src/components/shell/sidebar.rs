//! 左侧栏：Places + Volumes（Nautilus 风格）

use crate::components::common::icon::{IconStyle, icon as render_icon};
use crate::components::menus::{MenuEntry, MenuStyle, context_dropdown};
use crate::domain::SIDEBAR_WIDTH_PX;
use crate::utils::mounts::{SidebarItem, SidebarItemKind};
use iced::widget::{button, column, container, row, rule, scrollable, text};
use iced::{Alignment, Element, Length, Theme};
use rust_i18n::t;
use std::path::PathBuf;
use std::sync::Arc;

pub struct SidebarText {
    pub places: String,
    pub volumes: String,
    pub read_only: String,
}

#[derive(Copy, Clone)]
pub struct SidebarStyle {
    pub icon_color: iced::Color,
    pub panel_style: fn(&Theme) -> iced::widget::container::Style,
    pub list_row_style: fn(&Theme, iced::widget::button::Status) -> iced::widget::button::Style,
    pub action_button_style:
        fn(&Theme, iced::widget::button::Status) -> iced::widget::button::Style,
}

#[derive(Clone)]
pub struct SidebarActions<M> {
    pub on_navigate: fn(PathBuf) -> M,
    pub on_swipe_start: fn(String, PathBuf) -> M,
}

#[derive(Clone)]
pub struct SidebarMenu<M> {
    pub mount_supported: bool,
    pub menu_style: MenuStyle,
    pub on_mount: fn(String) -> M,
    pub on_unmount: fn(String) -> M,
    pub on_dismiss: M,
    pub swipe_open_device: Option<String>,
}

pub fn sidebar<'a, M: Clone + 'a>(
    selected_path: &std::path::Path,
    items: Vec<SidebarItem>,
    labels: SidebarText,
    style: SidebarStyle,
    actions: SidebarActions<M>,
    menu: Option<SidebarMenu<M>>,
) -> Element<'a, M> {
    let mut places_col = column![].spacing(2).width(Length::Fill);
    let mut volumes_col = column![].spacing(2).width(Length::Fill);

    let mut places_count: usize = 0;
    let mut volumes_count: usize = 0;

    for it in items.iter() {
        match it.kind {
            SidebarItemKind::Place => {
                places_count += 1;
                places_col = places_col.push(sidebar_item_row(
                    it,
                    selected_path,
                    &labels,
                    style,
                    actions.clone(),
                    menu.as_ref(),
                ));
            }
            SidebarItemKind::Volume => {
                volumes_count += 1;
                volumes_col = volumes_col.push(sidebar_item_row(
                    it,
                    selected_path,
                    &labels,
                    style,
                    actions.clone(),
                    menu.as_ref(),
                ));
            }
        }
    }

    // 组装内容：
    // - 这里要显式把 `Column` 转成 `Element`，避免 `.into()` 在 if/else 分支里触发类型推断失败（E0283）。
    let places_list: Element<'a, M> = if places_count == 0 {
        empty_hint()
    } else {
        places_col.into()
    };

    let volumes_list: Element<'a, M> = if volumes_count == 0 {
        empty_hint()
    } else {
        volumes_col.into()
    };

    let content = column![
        section_header(labels.places.clone()),
        places_list,
        rule::horizontal(1),
        section_header(labels.volumes.clone()),
        volumes_list,
    ]
    .spacing(10)
    .padding([8, 8]);

    container(scrollable(content).height(Length::Fill))
        .width(Length::Fixed(SIDEBAR_WIDTH_PX))
        .style(style.panel_style)
        .into()
}

fn empty_hint<'a, M: 'a>() -> Element<'a, M> {
    container(text("—").size(12))
        .padding([6, 8])
        .width(Length::Fill)
        .into()
}

fn section_header<'a, M: 'a>(title: String) -> Element<'a, M> {
    // 主题层当前没有 `muted_text_color`，这里先用默认文字颜色+字号表达层级。
    // 后续如果你希望更“灰”的标题色，可以在 themes/styles.rs 增加 `muted_text_color(theme)` 再替换回来。
    container(text(title).size(12))
        .padding([6, 8])
        .width(Length::Fill)
        .into()
}

fn sidebar_item_row<'a, M: Clone + 'a>(
    item: &SidebarItem,
    selected_path: &std::path::Path,
    labels: &SidebarText,
    style: SidebarStyle,
    actions: SidebarActions<M>,
    menu: Option<&SidebarMenu<M>>,
) -> Element<'a, M> {
    let icon = icon_for_item(item);

    let icon_view = render_icon::<M>(icon, IconStyle::new(style.icon_color, Some(16.0)));

    let primary = row![icon_view, text(item.label.clone()).size(13)]
        .spacing(8)
        .align_y(Alignment::Center);

    let read_only = matches!(item.writable, Some(false));
    let removable = matches!(item.removable, Some(true));
    let fs_label = item.fs_type.clone();
    let content = if item.subtitle.is_some() || read_only || removable || fs_label.is_some() {
        let mut sub_row = row![].spacing(6).align_y(Alignment::Center);
        if let Some(sub) = item.subtitle.as_ref() {
            sub_row = sub_row.push(text(sub.clone()).size(11));
        }
        if let Some(fs) = fs_label.as_ref() {
            sub_row = sub_row.push(text(fs.clone()).size(11));
        }
        if removable {
            sub_row = sub_row.push(text(t!("sidebar.removable").to_string()).size(11));
        }
        if read_only {
            sub_row = sub_row.push(text(labels.read_only.clone()).size(11));
        }
        column![primary, sub_row].spacing(2)
    } else {
        column![primary]
    };

    let is_selected = item.path.as_path() == selected_path;

    let mut b = button(content)
        .padding([6, 8])
        .width(Length::Fill)
        .style(move |theme, status| (style.list_row_style)(theme, status));

    let mut press_msg: Option<M> = None;
    if item.kind == SidebarItemKind::Volume {
        if let Some(menu) = menu {
            if menu.mount_supported {
                if let Some(device) = item.device.clone() {
                    press_msg = Some((actions.on_swipe_start)(device, item.path.clone()));
                }
            }
        }
    }

    if press_msg.is_none() && item.mounted && !is_selected {
        press_msg = Some((actions.on_navigate)(item.path.clone()));
    }

    if let Some(msg) = press_msg {
        b = b.on_press(msg);
    }

    let base: Element<'a, M> = container(b).width(Length::Fill).into();

    let Some(menu) = menu else {
        return base;
    };

    let mut action_button: Option<Element<'a, M>> = None;

    if menu.mount_supported && item.kind == SidebarItemKind::Volume {
        if let Some(device) = item.device.clone() {
            let (label, action, disabled) = if item.mounted {
                ("⏏", (menu.on_unmount)(device.clone()), item.system)
            } else {
                ("⏺", (menu.on_mount)(device.clone()), false)
            };

            let mut btn = button(text(label).size(12))
                .padding(4)
                .width(Length::Fixed(24.0))
                .height(Length::Fixed(24.0))
                .style(style.action_button_style);

            if !disabled {
                btn = btn.on_press(action);
            }

            if menu.swipe_open_device.as_deref() == Some(device.as_str()) {
                action_button = Some(container(btn).width(Length::Shrink).into());
            }
        }
    }

    let row = if let Some(btn) = action_button {
        row![container(base).width(Length::Fill), btn]
            .spacing(6)
            .align_y(Alignment::Center)
            .into()
    } else {
        base
    };

    if !menu.mount_supported || item.kind != SidebarItemKind::Volume {
        return row;
    }

    let Some(device) = item.device.clone() else {
        return row;
    };

    let mut entries: Vec<MenuEntry<M>> = Vec::new();
    if item.mounted {
        if !item.system {
            entries.push(MenuEntry::item(
                t!("sidebar.menu.unmount").to_string(),
                icondata::RiFile2DocumentLine,
                (menu.on_unmount)(device),
            ));
        }
    } else {
        entries.push(MenuEntry::item(
            t!("sidebar.menu.mount").to_string(),
            icondata::RiFolder2DocumentLine,
            (menu.on_mount)(device),
        ));
    }

    let items = Arc::new(entries);
    context_dropdown(row, items, menu.on_dismiss.clone(), menu.menu_style)
}

fn icon_for_item(item: &SidebarItem) -> icondata::Icon {
    // 说明：
    // - 这里先保持“保守选择”，避免因为 icon 常量不存在导致编译失败。
    // - 下一步会按你的要求替换为真正的“硬盘/设备”图标，并区分 Places（主目录/下载/桌面等）。
    match item.kind {
        SidebarItemKind::Place => icondata::RiFolder2DocumentLine,
        SidebarItemKind::Volume => icondata::RiFolder2DocumentLine,
    }
}
