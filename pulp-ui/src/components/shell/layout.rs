//! 布局骨架：统一应用级布局结构。

use crate::domain::{APP_GAP_PX, APP_PADDING_PX};
use iced::widget::{column, container, row};
use iced::{Element, Length, Theme};

#[derive(Copy, Clone)]
pub struct LayoutStyle {
    pub container_style: fn(&Theme) -> iced::widget::container::Style,
}

pub fn main_split<'a, M: 'a>(
    left: Element<'a, M>,
    main: Element<'a, M>,
    drawer: Element<'a, M>,
) -> Element<'a, M> {
    row![left, main, drawer]
        .spacing(APP_GAP_PX)
        .height(Length::Fill)
        .into()
}

pub fn app_shell<'a, M: 'a>(
    top: Element<'a, M>,
    main_row: Element<'a, M>,
    status: Element<'a, M>,
    style: LayoutStyle,
) -> Element<'a, M> {
    let content = column![top, main_row, status]
        .spacing(APP_GAP_PX)
        .padding(APP_PADDING_PX)
        .height(Length::Fill);

    container(content)
        .style(style.container_style)
        .height(Length::Fill)
        .width(Length::Fill)
        .into()
}
