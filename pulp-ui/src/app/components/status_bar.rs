//! 底部状态栏组件

use crate::app::themes;
use crate::domain::Message;
use iced::widget::{column, container, row, rule, text};
use iced::{Alignment, Element, Length};
use rust_i18n::t;

pub fn status_bar<'a>(
    item_count: usize,
    status: &'a str,
    busy: bool,
    spinner_index: usize,
) -> Element<'a, Message> {
    let status_owned = status.to_string();

    let left = text(t!("status.items", count = item_count)).size(12);
    let mid = text(status_owned).size(12);

    let right = if busy {
        let frames = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
        let s = frames[spinner_index % frames.len()];
        text(s).size(12)
    } else {
        text("").size(12)
    };

    let bar = row![
        container(left).width(Length::Shrink),
        container(mid).width(Length::Fill),
        container(right).width(Length::Shrink),
    ]
    .spacing(10)
    .align_y(Alignment::Center);

    container(column![
        rule::horizontal(1),
        container(bar).padding([6, 10])
    ])
    .style(themes::styles::panel_style)
    .into()
}
