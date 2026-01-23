//! 可复用的下拉菜单组件：统一 app_bar、右键菜单等的菜单风格。
//!
//! 设计目标：
//! - 复用 app_bar 当前的「菜单面板 + 菜单项」样式（menu_panel_style / menu_item_style）
//! - 让调用方只关心“菜单项列表”与“打开状态/关闭消息”
//! - 支持分隔线（rule）
//!
//! 说明：
//! - iced_aw::DropDown 需要调用方提供 `open` 布尔状态；
//! - 对于“右键菜单”这种由 ContextMenu 控制开合的场景，通常不需要显式 open state，
//!   而是把 DropDown 当做 overlay 样式的菜单内容即可（由 ContextMenu 负责显示/隐藏）。

use crate::components::common::icon::{IconStyle, icon as render_icon};
use iced::widget::{button, column, container, row, rule, scrollable, text};
use iced::{Alignment, Element, Length, Theme};
use iced_aw::{DropDown, drop_down};

/// 可复用的菜单项定义。
#[derive(Debug, Clone)]
pub enum MenuEntry<M> {
    /// 普通菜单项：图标 + 文本 + 点击消息
    Item {
        label: String,
        icon: icondata::Icon,
        on_press: M,
    },
    /// 分隔线
    Separator,
}

impl<M> MenuEntry<M> {
    pub fn item(label: impl Into<String>, icon: icondata::Icon, on_press: M) -> Self {
        Self::Item {
            label: label.into(),
            icon,
            on_press,
        }
    }

    pub fn separator() -> Self {
        Self::Separator
    }
}

/// 菜单样式参数（解耦主题与组件）
#[derive(Copy, Clone)]
pub struct MenuStyle {
    pub icon_color: iced::Color,
    pub item_style: fn(&Theme, iced::widget::button::Status) -> iced::widget::button::Style,
    pub panel_style: fn(&Theme) -> iced::widget::container::Style,
}

/// 渲染单个菜单项（风格由调用方注入）。
pub fn menu_item<'a, M: Clone + 'a>(
    label: String,
    icon: icondata::Icon,
    msg: M,
    style: MenuStyle,
) -> Element<'a, M> {
    let icon_view = render_icon::<M>(icon, IconStyle::new(style.icon_color, Some(14.0)));

    button(
        row![icon_view, text(label).size(12)]
            .spacing(8)
            .align_y(Alignment::Center),
    )
    .padding([6, 10])
    .width(Length::Fill)
    .style(style.item_style)
    .on_press(msg)
    .into()
}

/// 渲染菜单条目列表（Item/Separator）。
pub fn menu_entries<'a, M: Clone + 'a>(
    entries: impl IntoIterator<Item = MenuEntry<M>>,
    style: MenuStyle,
) -> iced::widget::Column<'a, M> {
    let mut col = column![].spacing(4).width(Length::Fill);

    for e in entries {
        match e {
            MenuEntry::Item {
                label,
                icon,
                on_press,
            } => {
                col = col.push(menu_item(label, icon, on_press, style));
            }
            MenuEntry::Separator => {
                col = col.push(rule::horizontal(1));
            }
        }
    }

    col
}

/// 构造标准菜单面板（滚动 + padding + 固定宽度 + 统一 style）。
pub fn menu_panel<'a, M: Clone + 'a>(
    entries: impl IntoIterator<Item = MenuEntry<M>>,
    style: MenuStyle,
) -> Element<'a, M> {
    let items = menu_entries(entries, style);
    container(scrollable(items).height(Length::Shrink))
        .padding([10, 12])
        .width(Length::Fixed(240.0))
        .style(style.panel_style)
        .into()
}

/// 构造紧凑右键菜单面板（无滚动/无间隙，避免 hover 穿透）。
pub fn context_menu_panel<'a, M: Clone + 'a>(
    entries: impl IntoIterator<Item = MenuEntry<M>>,
    style: MenuStyle,
) -> Element<'a, M> {
    let items = menu_entries(entries, style).spacing(0);
    container(scrollable(items).height(Length::Shrink))
        .padding([10, 12])
        .width(Length::Fixed(240.0))
        .style(style.panel_style)
        .into()
}

/// 构造一个“标准下拉菜单”（DropDown）。
///
/// - `underlay`：锚点控件（通常是 ⋯ / 汉堡按钮）
/// - `overlay_entries`：菜单项列表（含分隔线）
/// - `open`：是否展开
/// - `on_dismiss`：点击外部/关闭时发出的消息
/// - `alignment`：下拉对齐方式（默认建议 BottomEnd）
/// - `style`：菜单样式参数
pub fn dropdown_menu<'a, M: Clone + 'a>(
    underlay: Element<'a, M>,
    overlay_entries: impl IntoIterator<Item = MenuEntry<M>>,
    open: bool,
    on_dismiss: M,
    alignment: drop_down::Alignment,
    style: MenuStyle,
) -> Element<'a, M> {
    let overlay = menu_panel(overlay_entries, style);

    DropDown::new(underlay, overlay, open)
        .on_dismiss(on_dismiss)
        .alignment(alignment)
        .width(Length::Shrink)
        .into()
}
