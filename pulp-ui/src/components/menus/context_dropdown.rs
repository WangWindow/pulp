//! 可复用的右键下拉菜单包装组件：封装 `iced_aw::ContextMenu` + 统一风格的 dropdown overlay。
//!
//! 目标：
//! - 调用方只需要提供：
//!   - `base`：被右键的内容（通常是一行/一个卡片）
//!   - `items`：菜单项列表（`Arc<Vec<MenuEntry>>`），用于减少克隆开销
//! - 菜单样式与 `app_bar` 的汉堡菜单保持一致（复用 `dropdown_menu`）。
//!
//! 说明：
//! - `iced_aw::ContextMenu` 负责处理“右键触发/关闭”的交互。
//! - 我们在这里把 overlay 渲染为 `dropdown_menu::dropdown_menu(...)`，以复用统一的菜单面板与菜单项样式。
//! - `open` 参数对右键菜单而言没有实际意义：ContextMenu 决定是否显示 overlay。
//!   因此这里固定传 `open = true`，仅用于让 DropDown 按“打开状态”渲染 overlay。

use crate::components::menus::{self, MenuEntry};
use iced::Element;
use std::sync::Arc;

/// 构造一个“右键菜单（统一 dropdown 风格）”。
///
/// - `base`：右键目标内容（比如列表行按钮）
/// - `items`：菜单项（支持 separator，建议复用共享列表）
///
/// 返回的 Element 会在右键时弹出菜单 overlay。
pub fn context_dropdown<'a, M: Clone + 'a>(
    base: Element<'a, M>,
    items: Arc<Vec<MenuEntry<M>>>,
    on_dismiss: M,
    style: menus::dropdown_menu::MenuStyle,
) -> Element<'a, M> {
    // 注意：`Element` 不是 `Clone`，不能缓存一个 overlay 然后在 closure 里 clone。
    // 正确做法是在 closure 内重新构建 overlay（成本很低，且避免 clone 约束）。
    iced_aw::ContextMenu::new(base, move || {
        let _ = on_dismiss.clone();
        menus::dropdown_menu::context_menu_panel(items.iter().cloned(), style)
    })
    .into()
}
