//! 视图层：只暴露接口；实现拆分在同目录其他文件中

mod app_bar;
mod context_menu;
mod drawer;
mod files_view;
mod layout;
mod sidebar;
mod status_bar;
mod table_header;
mod virtual_list;

pub use app_bar::app_bar;
pub use context_menu::context_menu_for_entry;
pub use drawer::drawer;
pub use files_view::file_entries;
pub use layout::{app_shell, main_split};
pub use sidebar::sidebar;
pub use status_bar::status_bar;
pub use table_header::table_header;
pub use virtual_list::{VirtualListConfig, virtual_list};
