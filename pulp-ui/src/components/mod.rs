//! 视图层组件：对外尽量暴露稳定的顶层 API；内部实现按职责拆分到子模块。
//!
//! 目录结构约定：
//! - `common/`：无业务语义、可跨页面复用的基础组件（按钮、面板等）
//! - `menus/`：菜单相关的可复用子组件（DropDown 等）
//! - `lists/`：列表渲染相关组件（文件列表、虚拟列表封装、表头等）
//! - `shell/`：应用骨架组件（布局、侧边栏、抽屉、状态栏、顶栏等）
//!
//! 设计目标：
//! - 上层（states/view 等）尽量只依赖 `components::*` 的 re-export；
//! - 允许内部目录/文件重排而不影响外部引用路径；
//! - 新组件优先放入合适的子模块，再决定是否提升到顶层 re-export。

// 子模块目录
pub mod common;
pub mod lists;
pub mod menus;
pub mod shell;

// -------------------------------
// Re-exports: 顶层组件 API（尽量保持稳定）
// -------------------------------

// Shell（布局骨架 + 顶栏）
pub use shell::{
    AppBarActions, AppBarStyle, AppBarText, DrawerActions, DrawerStyle, DrawerText, LayoutStyle,
    SidebarActions, SidebarMenu, SidebarStyle, SidebarText, StatusBarStyle, app_bar, app_shell,
    drawer, main_split, sidebar, status_bar,
};

// Lists（文件列表 + 表头 + 虚拟列表）
pub use lists::{
    FileListActions, FileListStyle, VirtualListConfig, file_entries, table_header, virtual_list,
};

// Menus（按需导出：保持 API 面尽量小）
// - 右键菜单封装
pub use menus::context_dropdown::context_dropdown;
// - DropDown 菜单基础构件（app_bar / context_menu 统一风格）
pub use menus::dropdown_menu::{MenuEntry, MenuStyle};
