//! 右侧 Drawer（抽屉）视图：单窗口内的配置/任务面板容器

use crate::app::themes;
use crate::domain::{DrawerPanel, Message};
use iced::widget::{button, column, container, row, rule, text};
use iced::{Alignment, Element, Length};
use rust_i18n::t;

/// Drawer 的可见分割线宽度（视觉线本体）。可抓取区域在其外层容器里放大。
const DIVIDER_VISUAL_PX: f32 = 1.0;
/// 分割线的可抓取区域宽度（人体工学）；越宽越好抓，但也更容易误触。
const DIVIDER_HITBOX_PX: f32 = 8.0;

pub fn drawer<'a>(
    open: bool,
    panel: DrawerPanel,
    width_px: f32,
    resizing: bool,
    content: Element<'a, Message>,
) -> Element<'a, Message> {
    if !open {
        // Drawer 关闭时不占位；让主内容占满。
        return container(row![])
            .width(Length::Fixed(0.0))
            .height(Length::Fill)
            .into();
    }

    // 左侧“可见分割线 + 可拖拽 hitbox”
    let divider_body = container(row![])
        .width(Length::Fixed(DIVIDER_VISUAL_PX))
        .height(Length::Fill)
        .style(if resizing {
            themes::styles::drawer_divider_active_style
        } else {
            themes::styles::drawer_divider_style
        });

    // 扩大 hitbox：用一个透明容器包住视觉线。
    // 为了能触发消息，这里用 button 作为“可点击区域”。
    // 后续如果要更细腻的拖拽（hover 高亮、cursor），再替换为自定义 widget。
    let divider: Element<'a, Message> = button(
        container(divider_body)
            .width(Length::Fixed(DIVIDER_HITBOX_PX))
            .height(Length::Fill),
    )
    .padding(0)
    .style(themes::styles::ghost_button_style)
    .on_press(Message::DrawerResizeStart)
    .into();

    let header = drawer_header(panel);
    let body = container(content)
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(12);

    let panel_container = container(column![header, rule::horizontal(1), body].spacing(10))
        .width(Length::Fixed(width_px))
        .height(Length::Fill)
        .style(themes::styles::panel_style);

    row![divider, panel_container]
        .spacing(0)
        .height(Length::Fill)
        .into()
}

/// Drawer 标题栏：标题 + 关闭按钮
fn drawer_header(panel: DrawerPanel) -> Element<'static, Message> {
    let title = match panel {
        DrawerPanel::Task => t!("drawer.panel.task").to_string(),
        DrawerPanel::Extract => t!("drawer.panel.extract").to_string(),
        DrawerPanel::Rename => t!("drawer.panel.rename").to_string(),
        DrawerPanel::NewFolder => t!("drawer.panel.new_folder").to_string(),
        DrawerPanel::DeleteConfirm => t!("drawer.panel.delete_confirm").to_string(),
    };

    let close_btn = button(text("×"))
        .padding([4, 10])
        .style(themes::styles::ghost_button_style)
        .on_press(Message::CloseDrawer);

    container(
        row![
            text(title).size(16),
            row![close_btn]
                .width(Length::Shrink)
                .align_y(Alignment::Center)
        ]
        .spacing(10)
        .align_y(Alignment::Center),
    )
    .padding([8, 10])
    .width(Length::Fill)
    .into()
}
