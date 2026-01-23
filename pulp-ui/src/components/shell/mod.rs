//! 应用骨架（Shell）相关组件：布局、侧边栏、抽屉、状态栏等。
//!
//! 约定：
//! - 该目录下的组件负责“页面骨架/布局拼装”，不直接持有业务状态机；
//! - 与具体交互细节无关的可复用小组件应放在 `components/common/`；
//! - 菜单相关的可复用组件放在 `components/menus/`；
//! - 列表渲染相关组件放在 `components/lists/`。

pub mod app_bar;
pub mod drawer;
pub mod layout;
pub mod sidebar;
pub mod status_bar;

// Re-exports: 对外暴露更稳定的 API，减少上层引用路径的噪音。
pub use app_bar::{AppBarActions, AppBarStyle, AppBarText, app_bar};
pub use drawer::{DrawerActions, DrawerStyle, DrawerText, drawer};
pub use layout::{LayoutStyle, app_shell, main_split};
pub use sidebar::{SidebarActions, SidebarMenu, SidebarStyle, SidebarText, sidebar};
pub use status_bar::{StatusBarStyle, status_bar};
