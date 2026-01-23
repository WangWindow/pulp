extern crate rust_i18n;

mod app;
mod components;
mod domain;
mod i18n;

mod utils;

// 初始化 rust-i18n
rust_i18n::i18n!("locales", fallback = "en");

fn main() -> iced::Result {
    app::run()
}
