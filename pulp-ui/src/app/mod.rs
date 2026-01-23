//! 应用层：只暴露对外接口；实现拆分在同目录其他 rs 文件中。

mod config;
mod states;
pub mod themes;
mod ui_rules;

use iced::{Size, window};
use states::App;

/// 启动应用。
///
/// 说明：
/// - 入口放在 app 模块，便于 UI crate 做集成/复用。
pub fn run() -> iced::Result {
    iced::application(App::new, App::update, App::view)
        .theme(App::theme)
        .subscription(App::subscription)
        .window(window::Settings {
            size: Size::new(1100.0, 720.0),
            ..Default::default()
        })
        .centered()
        .run()
}
