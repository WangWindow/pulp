//! 全局事件与跨组件交互的提取器。

use iced::{event, window};
use std::path::PathBuf;

/// 从 iced 事件中提取我们关心的交互（拖放/右键）。
pub fn handle_global_event(event: &event::Event) -> GlobalEvent {
    match event {
        event::Event::Window(window::Event::FileDropped(path)) => {
            GlobalEvent::FileDropped(path.clone())
        }
        _ => GlobalEvent::None,
    }
}

#[derive(Debug, Clone)]
pub enum GlobalEvent {
    None,
    FileDropped(PathBuf),
}
