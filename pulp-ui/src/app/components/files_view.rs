//! 文件区：平铺列表（默认视图）

use super::context_menu_for_entry;
use crate::app::components::{VirtualListConfig, virtual_list};
use crate::app::themes;
use crate::domain::{EntryRow, EntrySource, LIST_OVERSCAN, LIST_ROW_HEIGHT_PX, Message};
use crate::utils;
use iced::widget::scrollable::Viewport;
use iced::widget::{container, row, text};
use iced::{Element, Length};
use iced_aw::ContextMenu;
use icondata::{RiArchive2BusinessLine, RiFile2DocumentLine, RiFolder2DocumentLine};
use pulp_core::ArchiveFormat;

pub fn file_entries<'a>(
    entries: &'a [EntryRow],
    selected_entry: Option<&'a std::path::PathBuf>,
    viewport: Option<Viewport>,
) -> Element<'a, Message> {
    let config = VirtualListConfig::new(LIST_ROW_HEIGHT_PX, LIST_OVERSCAN, viewport);

    virtual_list(entries, config, move |entry| {
        // 显式类型注解：避免在后续重构中触发 `clone()` 的类型推断失败（E0282）。
        // 这里的目标很明确：我们需要一个“拥有所有权的行模型”，用于消息与闭包捕获。
        let row_model: EntryRow = entry.clone();
        let path = row_model.path.clone();

        // 行内容（图标 + 名称）
        let icon = entry_icon(&row_model);
        let icon_view = iced::widget::svg(utils::icon_handle(icon))
            .width(Length::Fixed(16.0))
            .height(Length::Fixed(16.0))
            .style(|theme, _status| iced::widget::svg::Style {
                color: Some(themes::icon_color(theme)),
            });

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

        // 行点击：单击选中（由 app/state 处理双击阈值等）
        let is_selected = selected_entry.map(|p| *p == path).unwrap_or(false);
        let base = iced::widget::button(content)
            .padding([6, 8])
            .width(Length::Fill)
            .style(move |theme, status| themes::styles::list_row_style(theme, status, is_selected))
            .on_press(Message::RowClicked(row_model.clone()));

        // 右键菜单（ContextMenu）：传入行模型，便于区分文件系统/压缩包条目。
        let menu = move || context_menu_for_entry(row_model.clone());

        container(ContextMenu::new(base, menu))
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
