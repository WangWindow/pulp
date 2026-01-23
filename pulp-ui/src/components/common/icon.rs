//! 通用图标组件：统一 svg icon 渲染与尺寸/颜色策略。

use crate::utils;
use iced::widget::svg;
use iced::{Color, Element, Length, Theme};

/// 图标渲染样式（颜色 + 尺寸）。
#[derive(Copy, Clone)]
pub struct IconStyle {
    pub color: Color,
    pub size: f32,
}

impl IconStyle {
    /// 创建一个 IconStyle，默认尺寸为 16.0。
    pub fn new(color: Color, size: Option<f32>) -> Self {
        Self {
            color,
            size: size.unwrap_or(16.0),
        }
    }
}

/// 渲染一个 SVG 图标为 `Element`。
pub fn icon<'a, M: 'a>(icon: icondata::Icon, style: IconStyle) -> Element<'a, M> {
    svg(utils::icon_handle(icon))
        .width(Length::Fixed(style.size))
        .height(Length::Fixed(style.size))
        .style(move |_theme: &Theme, _status| svg::Style {
            color: Some(style.color),
        })
        .into()
}
