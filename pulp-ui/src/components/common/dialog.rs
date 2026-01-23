//! 通用对话框组件（确认/提示）。

use iced::widget::{Stack, button, column, container, row, rule, text};
use iced::{Alignment, Element, Length, Theme};

#[derive(Copy, Clone)]
pub struct DialogStyle {
    pub panel_style: fn(&Theme) -> iced::widget::container::Style,
    pub backdrop_style: fn(&Theme) -> iced::widget::container::Style,
    pub button_style: fn(&Theme, iced::widget::button::Status) -> iced::widget::button::Style,
}

pub fn confirm_dialog<'a, M: Clone + 'a>(
    title: String,
    body: String,
    confirm_label: String,
    cancel_label: String,
    on_confirm: M,
    on_cancel: M,
    style: DialogStyle,
) -> Element<'a, M> {
    let content = container(
        column![
            text(title).size(16),
            rule::horizontal(1),
            text(body).size(12),
            row![
                button(text(cancel_label))
                    .style(style.button_style)
                    .padding([6, 10])
                    .on_press(on_cancel.clone()),
                button(text(confirm_label))
                    .style(style.button_style)
                    .padding([6, 10])
                    .on_press(on_confirm),
            ]
            .spacing(10)
            .align_y(Alignment::Center),
        ]
        .spacing(12),
    )
    .padding(12)
    .width(Length::Fixed(420.0))
    .style(style.panel_style);

    let overlay: Element<'a, M> = Stack::with_children(vec![
        button(
            container(text(""))
                .center_x(Length::Fill)
                .center_y(Length::Fill)
                .style(style.backdrop_style),
        )
        .padding(0)
        .style(style.button_style)
        .on_press(on_cancel)
        .into(),
        container(content)
            .center_x(Length::Fill)
            .center_y(Length::Fill)
            .padding(20)
            .into(),
    ])
    .into();

    overlay
}
