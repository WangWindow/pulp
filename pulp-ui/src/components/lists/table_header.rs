//! 文件表头组件（纯数据驱动，文案和样式参数外部传入）

use iced::widget::{button, container, row, text};
use iced::{Alignment, Element, Length, Theme};
use icondata::RiLayoutDesignLine;

/// 表头样式参数（全部为闭包 trait object，彻底解耦主题）
#[derive(Copy, Clone)]
pub struct TableHeaderStyle {
    pub icon_color: iced::Color,
    pub button_style: fn(&Theme, iced::widget::button::Status) -> iced::widget::button::Style,
    pub container_style: fn(&Theme) -> iced::widget::container::Style,
}

/// 文件表头组件（解耦 i18n/主题，参数全部外部传入）
///
/// - `col_names`：四列的标题文案
/// - `on_toggle`：切换视图的消息
/// - `style`：样式参数
pub fn table_header<'a, M: Clone + 'a>(
    col_names: [String; 4],
    on_toggle: M,
    style: TableHeaderStyle,
) -> Element<'a, M> {
    let icon_view = iced::widget::svg(crate::utils::icon_handle(RiLayoutDesignLine))
        .width(Length::Fixed(16.0))
        .height(Length::Fixed(16.0))
        .style(move |_theme, _status| iced::widget::svg::Style {
            color: Some(style.icon_color),
        });

    let switch_btn = button(icon_view)
        .padding([4, 8])
        .style(style.button_style)
        .on_press(on_toggle);

    let [col0, col1, col2, col3] = col_names;
    let header = row![
        container(text(col0)).width(Length::FillPortion(4)),
        container(text(col1)).width(Length::FillPortion(2)),
        container(text(col2)).width(Length::FillPortion(2)),
        container(text(col3)).width(Length::FillPortion(3)),
        container(switch_btn).width(Length::Shrink),
    ]
    .spacing(10)
    .align_y(Alignment::Center);

    container(header)
        .padding([6, 10])
        .style(style.container_style)
        .into()
}
