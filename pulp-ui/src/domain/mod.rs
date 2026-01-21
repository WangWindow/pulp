//! UI 类型层：统一导出供 app/views/utils 使用的结构与枚举。

mod actions;
mod constants;
mod entries;
mod events;
mod messages;
mod ui;

pub use actions::ContextAction;
pub use constants::{
    APP_GAP_PX, APP_PADDING_PX, DOUBLE_CLICK_GAP_MS, DRAWER_DEFAULT_WIDTH_PX,
    DRAWER_MAX_WIDTH_RATIO, DRAWER_MIN_WIDTH_PX, LIST_OVERSCAN, LIST_ROW_HEIGHT_PX,
    SIDEBAR_WIDTH_PX,
};
pub use entries::{EntryRow, EntrySource, FileEntry};
pub use events::{GlobalEvent, handle_global_event};
pub use messages::Message;
pub use ui::{DrawerPanel, Page, THEME_MODES, ThemeMode, ViewMode};
