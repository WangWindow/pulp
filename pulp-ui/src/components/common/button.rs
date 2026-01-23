//! 通用按钮组件：统一按钮/图标按钮的构建方式与样式传入。

use crate::components::common::icon::{IconStyle, icon as render_icon};
use iced::widget::{button, text};
use iced::{Element, Length, Theme};

/// 通用按钮样式（来自主题注入）。
#[derive(Copy, Clone)]
pub struct ButtonStyle {
    pub style: fn(&Theme, iced::widget::button::Status) -> iced::widget::button::Style,
}

/// 创建一个文本按钮。
pub fn text_button<'a, M: Clone + 'a>(
    label: impl Into<String>,
    on_press: Option<M>,
    style: ButtonStyle,
) -> Element<'a, M> {
    let mut b = button(text(label.into()))
        .padding([6, 10])
        .style(style.style);
    if let Some(msg) = on_press {
        b = b.on_press(msg);
    }
    b.into()
}

/// 创建一个图标按钮（统一尺寸与颜色）。
pub fn icon_button<'a, M: Clone + 'a>(
    icon: icondata::Icon,
    icon_style: IconStyle,
    on_press: Option<M>,
    button_style: ButtonStyle,
) -> Element<'a, M> {
    let icon_view = render_icon::<M>(icon, icon_style);
    let mut b = button(icon_view)
        .padding(8)
        .width(Length::Shrink)
        .height(Length::Shrink)
        .style(button_style.style);

    if let Some(msg) = on_press {
        b = b.on_press(msg);
    }

    b.into()
}
