//! UI Rules：承载“与组件解耦”的 UI 规则（操作可见性、菜单项生成等）。
//!
//! 设计目标：
//! - `components/` 应当尽量通用、无业务语义；只负责“渲染 + 发消息”。
//! - “哪些操作应当出现/可用”属于 UI 层的规则，但它依赖领域模型（如 `EntryRow/EntrySource`）
//!   与应用消息（如 `Message::ContextActionFor`），因此更适合放在 `domain/`，作为可复用的规则层。
//! - 这里不做 IO、不做状态机更新；这些仍由 `app/states/update.rs` 处理。
//!
//! 约定：
//! - 本模块只暴露“纯函数规则”：输入领域数据，输出 UI 结构化结果（如菜单项列表）或布尔判断。
//! - 组件层只消费这些规则的输出（例如 `Vec<MenuSpecItem>`），并负责渲染。
//!
//! 当前：先提供模块入口，具体规则实现会逐步迁移到这里。
pub mod entry_context_menu;
pub mod menu_spec;

pub use entry_context_menu::entry_context_menu_entries;
