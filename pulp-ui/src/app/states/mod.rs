//! iced 状态层：按职责拆分为多个子模块。

mod app;
mod helpers;
mod tasks;
mod update;
mod view;

pub use app::App;
