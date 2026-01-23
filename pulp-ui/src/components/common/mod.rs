//! 通用可复用基础组件（Common Components）入口。
//!
//! 约定：
//! - `common/` 只放“无业务语义”的基础组件（如 icon button、分隔线、panel、badge、empty state 等）。
//! - 与菜单相关的结构化组件放在 `components/menus/`。
//! - 与列表渲染相关的组件放在 `components/lists/`。
//! - 与页面骨架相关的组件放在 `components/shell/`。

// 通用组件子模块
pub mod button;
pub mod dialog;
pub mod icon;
