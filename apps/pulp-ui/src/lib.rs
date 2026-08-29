#![forbid(unsafe_code)]
#![doc = "Desktop application entry point for Pulp."]

use std::borrow::Cow;

use gpui::{
    App, AppContext as _, Application, WindowBounds, WindowDecorations, WindowOptions, px, size,
};
use gpui_component::{Root, Theme, ThemeMode, TitleBar};

mod archive;
mod i18n;
mod password;
mod settings;
mod views;
mod workspace;

use i18n::{I18n, Locale};
use settings::{
    LanguagePreference, SettingsFile, ThemePreference, default_settings_path, load_settings,
};
use workspace::Workspace;

fn apply_theme_preference(
    preference: ThemePreference,
    window: Option<&mut gpui::Window>,
    cx: &mut App,
) {
    match preference {
        ThemePreference::Light => Theme::change(ThemeMode::Light, window, cx),
        ThemePreference::Dark => Theme::change(ThemeMode::Dark, window, cx),
        ThemePreference::System => match dark_light::detect() {
            Ok(dark_light::Mode::Dark) => Theme::change(ThemeMode::Dark, window, cx),
            Ok(dark_light::Mode::Light) => Theme::change(ThemeMode::Light, window, cx),
            Ok(dark_light::Mode::Unspecified) | Err(_) => {
                Theme::sync_system_appearance(window, cx);
            }
        },
    }
}

fn locale_for(settings: &SettingsFile) -> Locale {
    match settings.ui.language {
        LanguagePreference::System => Locale::from_system(),
        LanguagePreference::English => Locale::En,
        LanguagePreference::ZhCn => Locale::ZhCn,
    }
}

/// Starts the Pulp desktop application.
pub fn run() {
    Application::new().run(|cx| {
        gpui_component::init(cx);

        let settings_path = default_settings_path();
        let settings = settings_path
            .as_deref()
            .and_then(|path| load_settings(path).ok())
            .unwrap_or_default();
        apply_theme_preference(settings.ui.theme, None, cx);
        cx.text_system()
            .add_fonts(vec![Cow::Borrowed(lucide_icons::LUCIDE_FONT_BYTES)])
            .expect("Lucide icon font should load");

        let i18n = I18n::new(locale_for(&settings));
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::centered(size(px(1_180.), px(760.)), cx)),
            app_id: Some(String::from("pulp")),
            window_min_size: Some(size(px(760.), px(480.))),
            titlebar: Some(TitleBar::title_bar_options()),
            window_decorations: Some(WindowDecorations::Client),
            ..WindowOptions::default()
        };

        cx.open_window(options, move |window, cx| {
            let view = cx.new(|_| Workspace::new(settings.clone(), i18n.clone(), settings_path));
            view.update(cx, |workspace, cx| workspace.load_formats(cx));
            cx.new(|cx| Root::new(view, window, cx))
        })
        .expect("Pulp window should open");
    });
}
