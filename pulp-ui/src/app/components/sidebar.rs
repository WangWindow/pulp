//! 左侧栏：Places + Volumes（Nautilus 风格）

use crate::app::themes;
use crate::domain::{Message, SIDEBAR_WIDTH_PX};
use crate::utils;
use crate::utils::mounts::{SidebarItem, SidebarItemKind};
use iced::widget::{button, column, container, row, rule, scrollable, text};
use iced::{Alignment, Element, Length};
use rust_i18n::t;

pub fn sidebar<'a>(selected_path: &std::path::Path) -> Element<'a, Message> {
    // mounts 模块对外暴露的是 `load_sidebar_items`（一次性返回 Places + Volumes）
    let items = crate::utils::mounts::load_sidebar_items();

    // 重要：不要把本函数栈上的 Vec 借用塞进返回的 `Element<'a, _>`，否则会触发生命周期错误。
    // 解决方式：在循环里把每个条目即时转换为“拥有所有权”的 UI 节点。
    let mut places_col = column![].spacing(2).width(Length::Fill);
    let mut volumes_col = column![].spacing(2).width(Length::Fill);

    let mut places_count: usize = 0;
    let mut volumes_count: usize = 0;

    for it in items {
        match it.kind {
            SidebarItemKind::Place => {
                places_count += 1;
                places_col = places_col.push(sidebar_item_row(&it, selected_path));
            }
            SidebarItemKind::Volume => {
                volumes_count += 1;
                volumes_col = volumes_col.push(sidebar_item_row(&it, selected_path));
            }
        }
    }

    // 组装内容：
    // - 这里要显式把 `Column` 转成 `Element`，避免 `.into()` 在 if/else 分支里触发类型推断失败（E0283）。
    let places_list: Element<'a, Message> = if places_count == 0 {
        empty_hint()
    } else {
        places_col.into()
    };

    let volumes_list: Element<'a, Message> = if volumes_count == 0 {
        empty_hint()
    } else {
        volumes_col.into()
    };

    let content = column![
        section_header(t!("sidebar.places").to_string()),
        places_list,
        rule::horizontal(1),
        section_header(t!("sidebar.volumes").to_string()),
        volumes_list,
    ]
    .spacing(10)
    .padding([8, 8]);

    container(scrollable(content).height(Length::Fill))
        .width(Length::Fixed(SIDEBAR_WIDTH_PX))
        .style(themes::styles::panel_style)
        .into()
}

fn empty_hint<'a>() -> Element<'a, Message> {
    container(text("—").size(12))
        .padding([6, 8])
        .width(Length::Fill)
        .into()
}

fn section_header<'a>(title: String) -> Element<'a, Message> {
    // 主题层当前没有 `muted_text_color`，这里先用默认文字颜色+字号表达层级。
    // 后续如果你希望更“灰”的标题色，可以在 themes/styles.rs 增加 `muted_text_color(theme)` 再替换回来。
    container(text(title).size(12))
        .padding([6, 8])
        .width(Length::Fill)
        .into()
}

fn sidebar_item_row<'a>(
    item: &SidebarItem,
    selected_path: &std::path::Path,
) -> Element<'a, Message> {
    let icon = icon_for_item(item);

    let icon_view = iced::widget::svg(utils::icon_handle(icon))
        .width(Length::Fixed(16.0))
        .height(Length::Fixed(16.0))
        .style(|theme, _status| iced::widget::svg::Style {
            color: Some(themes::icon_color(theme)),
        });

    let primary = row![icon_view, text(item.label.clone()).size(13)]
        .spacing(8)
        .align_y(Alignment::Center);

    let read_only = matches!(item.writable, Some(false));
    let content = if item.subtitle.is_some() || read_only {
        let mut sub_row = row![].spacing(6).align_y(Alignment::Center);
        if let Some(sub) = item.subtitle.as_ref() {
            sub_row = sub_row.push(text(sub.clone()).size(11));
        }
        if read_only {
            sub_row = sub_row.push(text(t!("sidebar.read_only").to_string()).size(11));
        }
        column![primary, sub_row].spacing(2)
    } else {
        column![primary]
    };

    let is_selected = item.path.as_path() == selected_path;

    let mut b = button(content)
        .padding([6, 8])
        .width(Length::Fill)
        .style(move |theme, status| themes::styles::list_row_style(theme, status, is_selected));

    if !is_selected {
        b = b.on_press(Message::NavigateTo(item.path.clone()));
    }

    container(b).width(Length::Fill).into()
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
