//! 底部状态栏组件

use iced::widget::{column, container, row, rule, text};
use iced::{Alignment, Element, Length, Theme};

#[derive(Copy, Clone)]
pub struct StatusBarStyle {
    pub panel_style: fn(&Theme) -> iced::widget::container::Style,
}

pub fn status_bar<'a, M: 'a>(
    items_label: String,
    status: String,
    busy: bool,
    spinner_index: usize,
    style: StatusBarStyle,
) -> Element<'a, M> {
    // 中文注释：由上层传入已本地化文本，组件不再依赖 i18n。
    let left = text(items_label).size(12);
    let mid = text(status).size(12);

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
    .style(style.panel_style)
    .into()
}
