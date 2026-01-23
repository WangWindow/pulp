//! 顶部应用栏（导航 + 位置栏 + 搜索 + 设置入口）

use crate::components::common::button::{ButtonStyle, icon_button, text_button};
use crate::components::common::icon::IconStyle;
use crate::components::menus::dropdown_menu::{self, MenuEntry, MenuStyle};
use iced::widget::{button, container, row, text, text_input};
use iced::{Alignment, Element, Length, Theme};
use iced_aw::drop_down;
use icondata::{
    RiArrowLeftSArrowsLine, RiArrowRightSArrowsLine, RiArrowUpSArrowsLine, RiHome4BuildingsLine,
    RiMenu3SystemLine, RiSettings3SystemLine,
};
use std::path::PathBuf;

pub struct AppBarText {
    pub path_input_placeholder: String,
    pub search_placeholder: String,
    pub location_done: String,
    pub location_edit: String,
}

#[derive(Copy, Clone)]
pub struct AppBarStyle {
    pub icon_color: iced::Color,
    pub button_style: fn(&Theme, iced::widget::button::Status) -> iced::widget::button::Style,
    pub container_style: fn(&Theme) -> iced::widget::container::Style,
}

pub struct AppBarActions<M> {
    pub on_toggle_menu: fn(bool) -> M,
    pub on_back: Option<M>,
    pub on_forward: Option<M>,
    pub on_up: M,
    pub on_home: M,
    pub on_path_changed: fn(String) -> M,
    pub on_path_submit: M,
    pub on_filter_changed: fn(String) -> M,
    pub on_toggle_location_edit: M,
    pub on_toggle_settings: M,
    pub on_toggle_title_menu: M,
    pub on_dismiss_title_menu: M,
    pub on_navigate_to: fn(PathBuf) -> M,
}

pub fn app_bar<'a, M: Clone + 'a>(
    sidebar_open: bool,
    selected_path: &std::path::Path,
    path_text: &str,
    filter: &str,
    title_menu_open: bool,
    location_editing: bool,
    labels: AppBarText,
    menu_entries: Vec<MenuEntry<M>>,
    menu_style: MenuStyle,
    style: AppBarStyle,
    actions: AppBarActions<M>,
) -> Element<'a, M> {
    // 通用的图标按钮（仅负责渲染与发消息）。
    let icon_btn = move |icon: icondata::Icon, msg: Option<M>| -> Element<'a, M> {
        icon_button(
            icon,
            IconStyle::new(style.icon_color, Some(16.0)),
            msg,
            ButtonStyle {
                style: style.button_style,
            },
        )
    };

    // 左侧：导航与侧栏开关
    let nav = row![
        icon_btn(
            RiMenu3SystemLine,
            Some((actions.on_toggle_menu)(sidebar_open)),
        ),
        icon_btn(RiArrowLeftSArrowsLine, actions.on_back.clone()),
        icon_btn(RiArrowRightSArrowsLine, actions.on_forward.clone()),
        icon_btn(RiArrowUpSArrowsLine, Some(actions.on_up.clone())),
        icon_btn(RiHome4BuildingsLine, Some(actions.on_home.clone())),
    ]
    .spacing(6)
    .align_y(Alignment::Center);

    let on_path_changed = actions.on_path_changed;
    let on_filter_changed = actions.on_filter_changed;

    // 中间：位置栏（面包屑 or 可编辑路径输入）
    let location: Element<'a, M> = if location_editing {
        text_input(labels.path_input_placeholder.as_str(), path_text)
            .on_input(move |value| (on_path_changed)(value))
            .on_submit(actions.on_path_submit.clone())
            .padding(8)
            .width(Length::Fill)
            .into()
    } else {
        breadcrumb_bar(selected_path, style, actions.on_navigate_to)
    };

    // 右侧：搜索、路径切换、设置、菜单
    let search = text_input(labels.search_placeholder.as_str(), filter)
        .on_input(move |value| (on_filter_changed)(value))
        .padding(8)
        .width(Length::Fixed(260.0));

    let location_toggle_label = if location_editing {
        labels.location_done.clone()
    } else {
        labels.location_edit.clone()
    };
    let location_toggle = text_button(
        location_toggle_label,
        Some(actions.on_toggle_location_edit.clone()),
        ButtonStyle {
            style: style.button_style,
        },
    );

    let settings_btn = icon_btn(
        RiSettings3SystemLine,
        Some(actions.on_toggle_settings.clone()),
    );

    // 标题栏菜单：仅保留全局动作（新建/视图切换/设置）。
    let menu_underlay: Element<'a, M> = container(
        button(text("⋯"))
            .padding(8)
            .width(Length::Fill)
            .height(Length::Fill)
            .style(style.button_style)
            .on_press(actions.on_toggle_title_menu.clone()),
    )
    .width(Length::Shrink)
    .height(Length::Shrink)
    .into();

    let menu = dropdown_menu::dropdown_menu(
        menu_underlay,
        menu_entries,
        title_menu_open,
        actions.on_dismiss_title_menu.clone(),
        drop_down::Alignment::BottomStart,
        menu_style,
    );

    let bar = row![nav, location, search, location_toggle, settings_btn, menu]
        .spacing(10)
        .align_y(Alignment::Center);

    container(bar)
        .padding(10)
        .style(style.container_style)
        .into()
}

/// 位置栏（面包屑）
///
/// 说明：
/// - 默认显示面包屑路径；
/// - 用户点“路径”按钮后可切换到可编辑路径输入。
pub fn breadcrumb_bar<'a, M: Clone + 'a>(
    path: &std::path::Path,
    style: AppBarStyle,
    on_navigate_to: fn(PathBuf) -> M,
) -> Element<'a, M> {
    use std::path::Component;

    let mut current = PathBuf::new();
    let mut items: Vec<Element<'a, M>> = Vec::new();

    for component in path.components() {
        match component {
            Component::RootDir => {
                current = PathBuf::from("/");
                let b = button(text("/"))
                    .padding([4, 8])
                    .style(style.button_style)
                    .on_press(on_navigate_to(current.clone()));
                items.push(b.into());
            }
            Component::Normal(part) => {
                current.push(part);
                items.push(text("›").size(14).into());
                let label = part.to_string_lossy().to_string();
                let b = button(text(label))
                    .padding([4, 8])
                    .style(style.button_style)
                    .on_press(on_navigate_to(current.clone()));
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
