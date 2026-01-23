//! 菜单相关的可复用子组件集合。
//!
//! 目标：
//! - 统一菜单（标题栏汉堡菜单、右键菜单等）的视觉与交互封装；
//! - 让调用方只关注“菜单项数据”与“触发位置”，避免在各视图重复拼装。
//!
//! 模块划分：
//! - `dropdown_menu`：DropDown 菜单的通用渲染（面板、菜单项、分隔线等）
//! - `entry_context_menu`：针对 `EntryRow` 的右键菜单“内容规则”（生成菜单项列表）
//! - `context_dropdown`：右键菜单封装：`ContextMenu` + dropdown overlay（统一风格）
//!
//! 公共 API 约定：
//! - 调用方优先从 `components::menus::*` 引入；
//! - 内部实现模块保持在 `menus::*` 下，避免到处写深层路径。

pub mod context_dropdown;
pub mod dropdown_menu;

// -------------------------------
// Re-exports: menus 公共 API
// -------------------------------

// 右键菜单封装（ContextMenu + dropdown overlay）
pub use context_dropdown::context_dropdown;

// DropDown 菜单基础构件
pub use dropdown_menu::{MenuEntry, MenuStyle};
