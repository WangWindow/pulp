//! 构造文件/文件夹条目的右键菜单（标准样式）

use crate::app::themes;
use crate::domain::{ContextAction, EntryRow, EntrySource, Message};
use iced::widget::{button, column, container, mouse_area, row, rule, text};
use iced::{Alignment, Element, Length};
use icondata::{RiArchive2BusinessLine, RiFile2DocumentLine, RiFolder2DocumentLine};
use rust_i18n::t;

pub fn context_menu_for_entry<'a>(row: EntryRow) -> Element<'a, Message> {
    let is_fs = matches!(row.source, EntrySource::FileSystem);
    let is_archive_file = is_fs && pulp_core::ArchiveFormat::from_path(&row.path).is_some();

    let mut items = column![].spacing(4).width(Length::Fill);

    // Open group
    items = items.push(item(
        t!("files.context.open").to_string(),
        if row.is_dir {
            RiFolder2DocumentLine
        } else {
            RiFile2DocumentLine
        },
        Message::ContextActionFor(ContextAction::Open, row.clone()),
    ));

    // Extract group (archive file only)
    if is_archive_file {
        items = items.push(rule::horizontal(1));
        items = items.push(item(
            t!("files.context.extract_smart").to_string(),
            RiArchive2BusinessLine,
            Message::ContextActionFor(ContextAction::SmartExtract, row.clone()),
        ));
        items = items.push(item(
            t!("files.context.extract_to").to_string(),
            RiArchive2BusinessLine,
            Message::ContextActionFor(ContextAction::ExtractTo, row.clone()),
        ));
    }

    // Compress group (file system only)
    if is_fs {
        items = items.push(rule::horizontal(1));
        items = items.push(item(
            t!("files.context.compress_zip").to_string(),
            RiArchive2BusinessLine,
            Message::ContextActionFor(ContextAction::CompressZip, row.clone()),
        ));
    }

    // File operations group (file system only)
    if is_fs {
        items = items.push(rule::horizontal(1));
        items = items.push(item(
            t!("files.context.rename").to_string(),
            RiFile2DocumentLine,
            Message::ContextActionFor(ContextAction::Rename, row.clone()),
        ));
        items = items.push(item(
            t!("files.context.delete").to_string(),
            RiFile2DocumentLine,
            Message::ContextActionFor(ContextAction::Delete, row.clone()),
        ));
    }

    // Properties group (optional)
    items = items.push(rule::horizontal(1));
    items = items.push(item(
        t!("files.context.properties").to_string(),
        RiFile2DocumentLine,
        Message::ContextActionFor(ContextAction::Properties, row),
    ));

    mouse_area(
        container(items)
            .padding(8)
            .width(Length::Fixed(220.0))
            .style(themes::styles::context_menu_panel_style),
    )
    .on_press(Message::Noop)
    .into()
}

fn item<'a>(label: String, icon: icondata::Icon, msg: Message) -> Element<'a, Message> {
    let icon_view = iced::widget::svg(crate::utils::icon_handle(icon))
        .width(Length::Fixed(14.0))
        .height(Length::Fixed(14.0))
        .style(|theme, _status| iced::widget::svg::Style {
            color: Some(themes::icon_color(theme)),
        });

    button(
        row![icon_view, text(label).size(12)]
            .spacing(8)
            .align_y(Alignment::Center),
    )
    .padding([6, 10])
    .width(Length::Fill)
    .style(themes::styles::menu_item_style)
    .on_press(msg)
    .into()
}
