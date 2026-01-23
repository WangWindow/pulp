//! 文件区：平铺列表（默认视图）

use crate::components::common::icon::{IconStyle, icon as render_icon};
use crate::components::menus::{MenuEntry, MenuStyle, context_dropdown};
use crate::components::{VirtualListConfig, virtual_list};
use crate::domain::{EntryRow, EntrySource, LIST_OVERSCAN, LIST_ROW_HEIGHT_PX};
use crate::utils;
use iced::widget::scrollable::Viewport;
use iced::widget::{container, row, text};
use iced::{Element, Length, Theme};
use icondata::{RiArchive2BusinessLine, RiFile2DocumentLine, RiFolder2DocumentLine};
use pulp_core::ArchiveFormat;
use std::sync::Arc;

#[derive(Copy, Clone)]
pub struct FileListStyle {
    pub icon_color: iced::Color,
    pub list_row_style: fn(&Theme, iced::widget::button::Status) -> iced::widget::button::Style,
}

pub struct FileListActions<M> {
    pub on_row_clicked: fn(EntryRow) -> M,
    pub on_dismiss_menu: M,
}

pub fn file_entries<'a, M: Clone + 'a>(
    entries: &'a [EntryRow],
    selected_path: &'a std::path::Path,
    viewport: Option<Viewport>,
    menu_style: MenuStyle,
    style: FileListStyle,
    actions: FileListActions<M>,
    build_context_menu: fn(&EntryRow) -> Arc<Vec<MenuEntry<M>>>,
) -> Element<'a, M> {
    let config = VirtualListConfig::new(LIST_ROW_HEIGHT_PX, LIST_OVERSCAN, viewport);

    virtual_list::<_, M, _>(entries, config, move |entry| {
        // 显式类型注解：避免在后续重构中触发 `clone()` 的类型推断失败（E0282）。
        // 这里的目标很明确：我们需要一个“拥有所有权的行模型”，用于消息与闭包捕获。
        let row_model: EntryRow = entry.clone();

        // 行内容（图标 + 名称）
        let icon = entry_icon(&row_model);
        let icon_view = render_icon::<M>(icon, IconStyle::new(style.icon_color, Some(16.0)));

        let name = text(row_model.display_name.clone()).size(13);
        let size_text = row_model
            .size
            .map(utils::format_size)
            .unwrap_or_else(|| "—".into());
        let kind = row_model.kind.clone();
        let modified = utils::format_time(row_model.modified);

        let content = row![
            container(row![icon_view, name].spacing(8)).width(Length::FillPortion(4)),
            container(text(size_text)).width(Length::FillPortion(2)),
            container(text(kind)).width(Length::FillPortion(2)),
            container(text(modified)).width(Length::FillPortion(3)),
        ]
        .spacing(10)
        .width(Length::Fill)
        .align_y(iced::Alignment::Center);

        let is_selected = row_model.path.as_path() == selected_path;

        // 行点击：单击选中（由 app/state 处理双击阈值等）
        let base: Element<'a, M> = iced::widget::button(content)
            .padding([6, 8])
            .width(Length::Fill)
            .height(Length::Fixed(LIST_ROW_HEIGHT_PX))
            .style(move |theme, status| {
                if is_selected {
                    crate::app::themes::styles::list_row_selected_style(theme, status)
                } else {
                    (style.list_row_style)(theme, status)
                }
            })
            .on_press((actions.on_row_clicked)(row_model.clone()))
            .into();

        // 右键菜单：统一封装（ContextMenu + dropdown overlay），菜单内容由单一来源生成。
        let items = build_context_menu(&row_model);
        container(context_dropdown(
            base,
            items,
            actions.on_dismiss_menu.clone(),
            menu_style,
        ))
        .width(Length::Fill)
        .height(Length::Fixed(LIST_ROW_HEIGHT_PX))
        .into()
    })
}

fn entry_icon(entry: &EntryRow) -> icondata::Icon {
    if entry.is_dir {
        RiFolder2DocumentLine
    } else {
        // 说明：
        // - 压缩包“容器文件”的图标应由 FileSystem 视图中的真实文件条目决定；
        // - 压缩包预览里的条目本身按普通文件/目录展示即可。
        // - 这里保留一个小分支：如果来源是 FileSystem 且 kind 提示为 archive，可继续用 archive 图标。
        match &entry.source {
            EntrySource::FileSystem => {
                if ArchiveFormat::from_path(&entry.path).is_some()
                    || entry.kind.to_lowercase().contains("archive")
                {
                    RiArchive2BusinessLine
                } else {
                    RiFile2DocumentLine
                }
            }
            EntrySource::Archive { .. } => RiFile2DocumentLine,
        }
    }
}
