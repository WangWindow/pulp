//! 右侧 Drawer（抽屉）视图：单窗口内的配置/任务面板容器

use crate::domain::{DIVIDER_HITBOX_PX, DIVIDER_VISUAL_PX, DrawerPanel};
use iced::widget::{button, column, container, row, rule, text as text_view};
use iced::{Alignment, Element, Length, Theme};

pub struct DrawerText {
    pub panel_task: String,
    pub panel_extract: String,
    pub panel_rename: String,
    pub panel_new_folder: String,
    pub panel_delete_confirm: String,
}

#[derive(Copy, Clone)]
pub struct DrawerStyle {
    pub divider_style: fn(&Theme) -> iced::widget::container::Style,
    pub divider_active_style: fn(&Theme) -> iced::widget::container::Style,
    pub panel_style: fn(&Theme) -> iced::widget::container::Style,
    pub button_style: fn(&Theme, iced::widget::button::Status) -> iced::widget::button::Style,
}

pub struct DrawerActions<M> {
    pub on_close: M,
    pub on_resize_start: M,
}

pub fn drawer<'a, M: Clone + 'a>(
    open: bool,
    panel: DrawerPanel,
    width_px: f32,
    resizing: bool,
    content: Element<'a, M>,
    labels: DrawerText,
    style: DrawerStyle,
    actions: DrawerActions<M>,
) -> Element<'a, M> {
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
            style.divider_active_style
        } else {
            style.divider_style
        });

    // 扩大 hitbox：用一个透明容器包住视觉线。
    // 为了能触发消息，这里用 button 作为“可点击区域”。
    // 后续如果要更细腻的拖拽（hover 高亮、cursor），再替换为自定义 widget。
    let divider: Element<'a, M> = button(
        container(divider_body)
            .width(Length::Fixed(DIVIDER_HITBOX_PX))
            .height(Length::Fill),
    )
    .padding(0)
    .style(style.button_style)
    .on_press(actions.on_resize_start.clone())
    .into();

    let header = drawer_header(panel, &labels, style, actions);
    let body = container(content)
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(12);

    let panel_container = container(column![header, rule::horizontal(1), body].spacing(10))
        .width(Length::Fixed(width_px))
        .height(Length::Fill)
        .style(style.panel_style);

    row![divider, panel_container]
        .spacing(0)
        .height(Length::Fill)
        .into()
}

/// Drawer 标题栏：标题 + 关闭按钮
fn drawer_header<'a, M: Clone + 'a>(
    panel: DrawerPanel,
    labels: &DrawerText,
    style: DrawerStyle,
    actions: DrawerActions<M>,
) -> Element<'a, M> {
    let title = match panel {
        DrawerPanel::Task => labels.panel_task.clone(),
        DrawerPanel::Extract => labels.panel_extract.clone(),
        DrawerPanel::Rename => labels.panel_rename.clone(),
        DrawerPanel::NewFolder => labels.panel_new_folder.clone(),
        DrawerPanel::DeleteConfirm => labels.panel_delete_confirm.clone(),
    };

    let close_btn = button(text_view("×"))
        .padding([4, 10])
        .style(style.button_style)
        .on_press(actions.on_close.clone());

    container(
        row![
            text_view(title).size(16),
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
