//! Domain 层的菜单规格模型（与 UI 组件解耦）。
//!
//! 设计目标：
//! - 只包含“菜单展示所需的纯数据”，不依赖 UI 组件类型。
//! - 可被 UI rules 产出，再由 components 映射为具体渲染元素。
//! - 保持稳定、可测试，便于未来扩展（如快捷键、危险操作标识等）。

use crate::domain::ContextAction;

/// 菜单项的纯数据规格。
#[derive(Debug, Clone)]
pub enum MenuSpecItem {
    /// 普通菜单项：包含 i18n 文案 key、图标标识、以及领域动作。
    Item {
        label_key: &'static str,
        icon: MenuIcon,
        action: ContextAction,
    },
    /// 分隔线。
    Separator,
}

impl MenuSpecItem {
    pub fn item(label_key: &'static str, icon: MenuIcon, action: ContextAction) -> Self {
        Self::Item {
            label_key,
            icon,
            action,
        }
    }

    pub fn separator() -> Self {
        Self::Separator
    }
}

/// 与 UI Icon 解耦的图标标识。
/// 组件层可以把 `MenuIcon` 映射为具体的 `icondata::Icon`。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MenuIcon {
    File,
    Folder,
    Archive,
}
