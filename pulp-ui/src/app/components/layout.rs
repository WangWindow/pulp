//! 布局骨架：统一应用级布局结构。

use crate::app::themes;
use crate::domain::Message;
use crate::domain::{APP_GAP_PX, APP_PADDING_PX};
use iced::widget::{column, container, row};
use iced::{Element, Length};

pub fn main_split<'a>(
    left: Element<'a, Message>,
    main: Element<'a, Message>,
    drawer: Element<'a, Message>,
) -> Element<'a, Message> {
    row![left, main, drawer]
        .spacing(APP_GAP_PX)
        .height(Length::Fill)
        .into()
}

pub fn app_shell<'a>(
    top: Element<'a, Message>,
    main_row: Element<'a, Message>,
    status: Element<'a, Message>,
) -> Element<'a, Message> {
    let content = column![top, main_row, status]
        .spacing(APP_GAP_PX)
        .padding(APP_PADDING_PX)
        .height(Length::Fill);

    container(content)
        .style(themes::styles::app_style)
        .height(Length::Fill)
        .width(Length::Fill)
        .into()
}
