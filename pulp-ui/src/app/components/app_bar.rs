//! 顶部应用栏（导航 + 位置栏 + 搜索 + 设置入口）

use crate::app::themes;
use crate::domain::Message;
use crate::utils;
use iced::widget::{button, container, row, rule, scrollable, text, text_input};
use iced::{Alignment, Element, Length};
use iced_aw::{DropDown, drop_down};
use icondata::{
    RiArrowLeftSArrowsLine, RiArrowRightSArrowsLine, RiArrowUpSArrowsLine, RiFile2DocumentLine,
    RiFolder2DocumentLine, RiHome4BuildingsLine, RiMenu3SystemLine, RiSettings3SystemLine,
};
use rust_i18n::t;

pub fn app_bar(
    sidebar_open: bool,
    can_back: bool,
    can_forward: bool,
    selected_path: &std::path::Path,
    path_text: &str,
    filter: &str,
    title_menu_open: bool,
    location_editing: bool,
) -> Element<'static, Message> {
    /// 通用的图标按钮（仅负责渲染与发消息）。
    fn icon_btn(icon: icondata::Icon, msg: Option<Message>) -> Element<'static, Message> {
        let icon_view = iced::widget::svg(utils::icon_handle(icon))
            .width(Length::Fixed(16.0))
            .height(Length::Fixed(16.0))
            .style(|theme, _status| iced::widget::svg::Style {
                color: Some(themes::icon_color(theme)),
            });

        let mut b = button(icon_view)
            .padding(8)
            .style(themes::styles::ghost_button_style);

        if let Some(m) = msg {
            b = b.on_press(m);
        }

        b.into()
    }

    // 左侧：导航与侧栏开关
    let nav = row![
        icon_btn(
            RiMenu3SystemLine,
            Some(if sidebar_open {
                Message::ToggleMenu
            } else {
                Message::ToggleMenu
            }),
        ),
        icon_btn(
            RiArrowLeftSArrowsLine,
            can_back.then_some(Message::NavigateBack),
        ),
        icon_btn(
            RiArrowRightSArrowsLine,
            can_forward.then_some(Message::NavigateForward),
        ),
        icon_btn(RiArrowUpSArrowsLine, Some(Message::NavigateUp)),
        icon_btn(RiHome4BuildingsLine, Some(Message::NavigateHome)),
    ]
    .spacing(6)
    .align_y(Alignment::Center);

    // 中间：位置栏（面包屑 or 可编辑路径输入）
    let location: Element<'static, Message> = if location_editing {
        text_input(t!("appbar.path_input_placeholder").as_ref(), path_text)
            .on_input(Message::PathChanged)
            .on_submit(Message::PathSubmitted)
            .padding(8)
            .width(Length::Fill)
            .into()
    } else {
        breadcrumb_bar(selected_path)
    };

    // 右侧：搜索、路径切换、设置、菜单
    let search = text_input(t!("appbar.search_placeholder").as_ref(), filter)
        .on_input(Message::FilterChanged)
        .padding(8)
        .width(Length::Fixed(260.0));

    let location_toggle = button(text(if location_editing {
        t!("appbar.location_done")
    } else {
        t!("appbar.location_edit")
    }))
    .style(themes::styles::ghost_button_style)
    .padding([6, 10])
    .on_press(Message::ToggleLocationEdit);

    let settings_btn = icon_btn(RiSettings3SystemLine, Some(Message::ToggleSettings));

    // 标题栏菜单：仅保留全局动作（新建/视图切换/设置）。
    let menu_underlay: Element<'static, Message> = container(
        button(text("⋯"))
            .padding(8)
            .width(Length::Fill)
            .height(Length::Fill)
            .style(themes::styles::ghost_button_style)
            .on_press(Message::ToggleTitleMenu),
    )
    .width(Length::Shrink)
    .height(Length::Shrink)
    .into();

    fn menu_item(label: String, icon: icondata::Icon, msg: Message) -> Element<'static, Message> {
        let icon_view = iced::widget::svg(utils::icon_handle(icon))
            .width(Length::Fixed(14.0))
            .height(Length::Fixed(14.0))
            .style(|theme, _status| iced::widget::svg::Style {
                color: Some(themes::icon_color(theme)),
            });

        button(
            row![icon_view, text(label).size(12)]
                .spacing(8)
                .align_y(Alignment::Center),
        )
        .padding([6, 10])
        .width(Length::Fill)
        .style(themes::styles::menu_item_style)
        .on_press(msg)
        .into()
    }

    let menu_items = iced::widget::column![
        menu_item(
            t!("appbar.new_folder").to_string(),
            RiFolder2DocumentLine,
            Message::NewFolderRequested,
        ),
        rule::horizontal(1),
        menu_item(
            t!("appbar.toggle_view").to_string(),
            RiFile2DocumentLine,
            Message::ToggleFileViewMode,
        ),
        rule::horizontal(1),
        menu_item(
            t!("menu.settings").to_string(),
            RiSettings3SystemLine,
            Message::ToggleSettings,
        ),
    ]
    .spacing(4);

    let menu_overlay = container(scrollable(menu_items).height(Length::Shrink))
        .padding(8)
        .width(Length::Fixed(220.0))
        .style(themes::styles::menu_panel_style);

    let menu = DropDown::new(menu_underlay, menu_overlay, title_menu_open)
        .on_dismiss(Message::DismissTitleMenu)
        .alignment(drop_down::Alignment::BottomEnd)
        .width(Length::Shrink);

    let bar = row![nav, location, search, location_toggle, settings_btn, menu]
        .spacing(10)
        .align_y(Alignment::Center);

    container(bar)
        .padding(10)
        .style(themes::styles::appbar_style)
        .into()
}

/// 位置栏（面包屑）
///
/// 说明：
/// - 默认显示面包屑路径；
/// - 用户点“路径”按钮后可切换到可编辑路径输入。
pub fn breadcrumb_bar(path: &std::path::Path) -> Element<'static, Message> {
    use std::path::{Component, PathBuf};

    let mut current = PathBuf::new();
    let mut items: Vec<Element<'static, Message>> = Vec::new();

    for component in path.components() {
        match component {
            Component::RootDir => {
                current = PathBuf::from("/");
                let b = button(text("/"))
                    .padding([4, 8])
                    .style(themes::styles::ghost_button_style)
                    .on_press(Message::NavigateTo(current.clone()));
                items.push(b.into());
            }
            Component::Normal(part) => {
                current.push(part);
                items.push(text("›").size(14).into());
                let label = part.to_string_lossy().to_string();
                let b = button(text(label))
                    .padding([4, 8])
                    .style(themes::styles::ghost_button_style)
                    .on_press(Message::NavigateTo(current.clone()));
                items.push(b.into());
            }
            _ => {}
        }
    }

    let mut r = row![]
        .spacing(6)
        .align_y(Alignment::Center)
        .width(Length::Fill);

    for e in items {
        r = r.push(e);
    }

    container(r).padding([0, 6]).width(Length::Fill).into()
}
