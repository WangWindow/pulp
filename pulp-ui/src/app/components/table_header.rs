//! 文件表头组件

use crate::app::themes;
use crate::domain::Message;
use crate::utils;
use iced::widget::{button, container, row, text};
use iced::{Alignment, Element, Length};
use icondata::RiLayoutDesignLine;
use rust_i18n::t;

pub fn table_header() -> Element<'static, Message> {
    let icon_view =
        iced::widget::svg(utils::icon_handle(RiLayoutDesignLine)).style(|theme, _status| {
            iced::widget::svg::Style {
                color: Some(themes::icon_color(theme)),
            }
        });

    let switch_btn = button(icon_view)
        .padding([4, 8])
        .style(themes::styles::ghost_button_style)
        .on_press(Message::ToggleFileViewMode);

    let header = row![
        container(text(t!("files.column.name"))).width(Length::FillPortion(4)),
        container(text(t!("files.column.size"))).width(Length::FillPortion(2)),
        container(text(t!("files.column.type"))).width(Length::FillPortion(2)),
        container(text(t!("files.column.modified"))).width(Length::FillPortion(3)),
        container(switch_btn).width(Length::Shrink),
    ]
    .spacing(10)
    .align_y(Alignment::Center);

    container(header)
        .padding([6, 10])
        .style(themes::styles::appbar_style)
        .into()
}
