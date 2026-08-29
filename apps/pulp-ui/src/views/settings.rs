//! Settings page built from typed setting fields.

use gpui::{Context, Entity, SharedString, Subscription, Window, div, prelude::*, px};
use gpui_component::button::{Button, ButtonVariants};
use gpui_component::input::{Input, InputEvent, InputState};
use gpui_component::setting::{SettingField, SettingGroup, SettingItem, SettingPage, Settings};
use gpui_component::sidebar::{SidebarMenu, SidebarMenuItem};
use gpui_component::{ActiveTheme, Sizable, h_flex, v_flex};
use lucide_icons::Icon as LucideIcon;

use crate::i18n::{I18n, MessageKey};
use crate::settings::{
    LanguagePreference, ListMode, OverwriteSetting, SettingsFile, ThemePreference,
};
use crate::workspace::Workspace;

#[derive(Clone, Copy, Eq, PartialEq)]
enum SettingsSection {
    General,
    Compression,
    Extraction,
    Security,
    Other,
}

impl SettingsSection {
    fn matches(self, query: &str, i18n: &I18n) -> bool {
        if query.is_empty() {
            return true;
        }
        let labels = match self {
            Self::General => &[
                MessageKey::General,
                MessageKey::Theme,
                MessageKey::Language,
                MessageKey::CompactLayout,
                MessageKey::ShowFolderPane,
                MessageKey::ShowArchivePath,
                MessageKey::DefaultListMode,
            ][..],
            Self::Compression => &[
                MessageKey::Compression,
                MessageKey::DefaultFormat,
                MessageKey::CompressionMethod,
                MessageKey::CompressionLevel,
                MessageKey::TestAfterCreate,
            ][..],
            Self::Extraction => &[
                MessageKey::Extraction,
                MessageKey::SmartExtraction,
                MessageKey::RestoreMetadata,
                MessageKey::OverwritePolicy,
            ][..],
            Self::Security => &[MessageKey::Security, MessageKey::RejectLinks][..],
            Self::Other => &[MessageKey::Other, MessageKey::RestoreDefaults][..],
        };
        labels
            .iter()
            .any(|label| i18n.text(*label).to_lowercase().contains(query))
    }
}

struct SettingsSidebarState {
    search: Entity<InputState>,
    query: String,
    section: SettingsSection,
    _subscription: Subscription,
}

/// Renders the settings navigation beside the active typed settings page.
pub fn render(
    workspace: &Workspace,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) -> impl gpui::IntoElement {
    let entity = cx.entity();
    let back_entity = entity.clone();
    let i18n = workspace.i18n().clone();
    let search_placeholder = i18n.text(MessageKey::SearchSettings);
    let sidebar_state = window.use_keyed_state("pulp-settings-sidebar", cx, |window, cx| {
        let search = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(search_placeholder)
                .default_value("")
        });
        let state_entity = cx.entity();
        let subscription = cx.subscribe_in(
            &search,
            window,
            move |_: &mut SettingsSidebarState, input, event: &InputEvent, _, app| {
                if matches!(event, InputEvent::Change) {
                    let query = input.read(app).value();
                    state_entity.update(app, |state, cx| {
                        state.query = query.to_string();
                        cx.notify();
                    });
                }
            },
        );
        SettingsSidebarState {
            search,
            query: String::new(),
            section: SettingsSection::General,
            _subscription: subscription,
        }
    });
    let selected_section = sidebar_state.read(cx).section;
    let query = sidebar_state.read(cx).query.to_lowercase();
    let format_options = workspace
        .formats()
        .iter()
        .map(|format| {
            (
                SharedString::from(format.id.to_string()),
                SharedString::from(format.name.clone()),
            )
        })
        .collect::<Vec<_>>();
    let format_options = if format_options.is_empty() {
        vec![(SharedString::from("zip"), SharedString::from("ZIP"))]
    } else {
        format_options
    };
    let selected_format = workspace.settings().archive.default_format.clone();
    let mut method_options = vec![(
        SharedString::from("default"),
        SharedString::from(i18n.text(MessageKey::ProviderDefault)),
    )];
    if let Some(format) = workspace
        .formats()
        .iter()
        .find(|format| format.id.as_str().eq_ignore_ascii_case(&selected_format))
    {
        method_options.extend(format.methods.iter().map(|method| {
            (
                SharedString::from(method.clone()),
                SharedString::from(method.clone()),
            )
        }));
    }

    let general = SettingPage::new(i18n.text(MessageKey::General))
        .default_open(true)
        .resettable(false)
        .group(
            SettingGroup::new()
                .title(i18n.text(MessageKey::General))
                .item(SettingItem::new(
                    i18n.text(MessageKey::Theme),
                    theme_field(entity.clone(), i18n.clone()),
                ))
                .item(SettingItem::new(
                    i18n.text(MessageKey::Language),
                    language_field(entity.clone(), i18n.clone()),
                ))
                .item(SettingItem::new(
                    i18n.text(MessageKey::CompactLayout),
                    bool_field(
                        entity.clone(),
                        |settings| settings.ui.compact_layout,
                        |settings, value| settings.ui.compact_layout = value,
                        false,
                    ),
                ))
                .item(SettingItem::new(
                    i18n.text(MessageKey::ShowFolderPane),
                    bool_field(
                        entity.clone(),
                        |settings| settings.ui.show_folder_pane,
                        |settings, value| settings.ui.show_folder_pane = value,
                        true,
                    ),
                ))
                .item(SettingItem::new(
                    i18n.text(MessageKey::ShowArchivePath),
                    bool_field(
                        entity.clone(),
                        |settings| settings.ui.show_archive_path,
                        |settings, value| settings.ui.show_archive_path = value,
                        true,
                    ),
                ))
                .item(SettingItem::new(
                    i18n.text(MessageKey::DefaultListMode),
                    list_mode_field(entity.clone(), i18n.clone()),
                )),
        );

    let creation = SettingPage::new(i18n.text(MessageKey::Compression))
        .resettable(false)
        .group(
            SettingGroup::new()
                .title(i18n.text(MessageKey::Compression))
                .item(SettingItem::new(
                    i18n.text(MessageKey::DefaultFormat),
                    dropdown_field(
                        entity.clone(),
                        format_options,
                        |settings| settings.archive.default_format.clone(),
                        |settings, value| settings.archive.default_format = value,
                    ),
                ))
                .item(SettingItem::new(
                    i18n.text(MessageKey::CompressionMethod),
                    compression_method_field(entity.clone(), method_options),
                ))
                .item(SettingItem::new(
                    i18n.text(MessageKey::CompressionLevel),
                    compression_level_field(entity.clone()),
                ))
                .item(SettingItem::new(
                    i18n.text(MessageKey::TestAfterCreate),
                    bool_field(
                        entity.clone(),
                        |settings| settings.archive.test_after_create,
                        |settings, value| settings.archive.test_after_create = value,
                        true,
                    ),
                )),
        );

    let extraction = SettingPage::new(i18n.text(MessageKey::Extraction))
        .resettable(false)
        .group(
            SettingGroup::new()
                .title(i18n.text(MessageKey::Extraction))
                .item(SettingItem::new(
                    i18n.text(MessageKey::SmartExtraction),
                    bool_field(
                        entity.clone(),
                        |settings| settings.extraction.smart,
                        |settings, value| settings.extraction.smart = value,
                        true,
                    ),
                ))
                .item(SettingItem::new(
                    i18n.text(MessageKey::RestoreMetadata),
                    bool_field(
                        entity.clone(),
                        |settings| settings.extraction.restore_metadata,
                        |settings, value| settings.extraction.restore_metadata = value,
                        true,
                    ),
                ))
                .item(SettingItem::new(
                    i18n.text(MessageKey::OverwritePolicy),
                    overwrite_field(entity.clone(), i18n.clone()),
                )),
        );

    let security = SettingPage::new(i18n.text(MessageKey::Security))
        .resettable(false)
        .group(
            SettingGroup::new()
                .title(i18n.text(MessageKey::Security))
                .item(SettingItem::new(
                    i18n.text(MessageKey::RejectLinks),
                    bool_field(
                        entity.clone(),
                        |settings| settings.security.reject_links,
                        |settings, value| settings.security.reject_links = value,
                        true,
                    ),
                )),
        );

    let other = SettingPage::new(i18n.text(MessageKey::Other))
        .resettable(false)
        .group(
            SettingGroup::new()
                .title(i18n.text(MessageKey::Other))
                .item(SettingItem::new(
                    i18n.text(MessageKey::RestoreDefaults),
                    restore_defaults_field(entity, i18n.clone()),
                )),
        );

    let active_page = match selected_section {
        SettingsSection::General => general,
        SettingsSection::Compression => creation,
        SettingsSection::Extraction => extraction,
        SettingsSection::Security => security,
        SettingsSection::Other => other,
    };

    let back = Button::new("pulp-settings-back")
        .ghost()
        .small()
        .tooltip(i18n.text(MessageKey::Back))
        .child(icon(LucideIcon::ArrowLeft, 16.))
        .on_click(move |_, _, app| {
            back_entity.update(app, |workspace, cx| workspace.close_settings(cx));
        });

    let sections = [
        (SettingsSection::General, i18n.text(MessageKey::General)),
        (
            SettingsSection::Compression,
            i18n.text(MessageKey::Compression),
        ),
        (
            SettingsSection::Extraction,
            i18n.text(MessageKey::Extraction),
        ),
        (SettingsSection::Security, i18n.text(MessageKey::Security)),
        (SettingsSection::Other, i18n.text(MessageKey::Other)),
    ];
    let navigation = SidebarMenu::new().children(
        sections
            .into_iter()
            .filter(|(section, _)| section.matches(&query, &i18n))
            .map(|(section, title)| {
                let state = sidebar_state.clone();
                SidebarMenuItem::new(title)
                    .active(section == selected_section)
                    .on_click(move |_, _, cx| {
                        state.update(cx, |state, cx| {
                            state.section = section;
                            cx.notify();
                        });
                    })
            }),
    );
    let sidebar = v_flex()
        .id("pulp-settings-sidebar")
        .absolute()
        .top_0()
        .left_0()
        .bottom_0()
        .w(px(220.))
        .bg(cx.theme().sidebar)
        .text_color(cx.theme().sidebar_foreground)
        .border_r_1()
        .border_color(cx.theme().sidebar_border)
        .child(
            h_flex()
                .id("pulp-settings-sidebar-header")
                .h(px(48.))
                .flex_shrink_0()
                .gap_2()
                .px_3()
                .child(back)
                .child(
                    Input::new(&sidebar_state.read(cx).search)
                        .prefix(icon(LucideIcon::Search, 14.))
                        .cleanable(true)
                        .flex_1()
                        .min_w_0(),
                ),
        )
        .child(
            v_flex()
                .id("pulp-settings-navigation")
                .p_3()
                .child(navigation),
        );

    div()
        .size_full()
        .relative()
        .bg(cx.theme().sidebar)
        .child(
            div()
                .absolute()
                .top_0()
                .left_0()
                .right_0()
                .bottom_0()
                .child(
                    Settings::new("pulp-settings")
                        .sidebar_width(px(220.))
                        .pages([active_page]),
                ),
        )
        .child(sidebar)
}

struct CompressionLevelState {
    input: Entity<InputState>,
    _subscription: Subscription,
}

fn compression_level_field(entity: Entity<Workspace>) -> SettingField<SharedString> {
    let value_entity = entity.clone();
    SettingField::render(move |options, window, cx| {
        let value = value_entity.read(cx).settings().archive.compression_level;
        let state = window
            .use_keyed_state("compression-level-input", cx, |window, cx| {
                let input =
                    cx.new(|cx| InputState::new(window, cx).default_value(value.to_string()));
                let entity = value_entity.clone();
                let subscription = cx.subscribe_in(
                    &input,
                    window,
                    move |_, input, event: &InputEvent, _, app| {
                        if matches!(event, InputEvent::Change)
                            && let Ok(value) = input.read(app).value().parse::<u8>()
                        {
                            let value = value.min(9);
                            entity.update(app, |workspace, cx| {
                                workspace.update_settings(
                                    |settings| settings.archive.compression_level = value,
                                    cx,
                                );
                            });
                        }
                    },
                );
                CompressionLevelState {
                    input,
                    _subscription: subscription,
                }
            })
            .read(cx)
            .input
            .clone();

        let value_text = value.to_string();
        if state.read(cx).value().as_str() != value_text {
            state.update(cx, |input, cx| input.set_value(value_text, window, cx));
        }

        h_flex()
            .h(px(32.))
            .w(gpui::px(128.))
            .child(level_step_button(
                "compression-level-decrement",
                LucideIcon::Minus,
                -1,
                state.clone(),
                options.size,
            ))
            .child(
                Input::new(&state)
                    .with_size(options.size)
                    .appearance(true)
                    .text_center()
                    .flex_1(),
            )
            .child(level_step_button(
                "compression-level-increment",
                LucideIcon::Plus,
                1,
                state,
                options.size,
            ))
    })
}

fn level_step_button(
    id: &'static str,
    button_icon: LucideIcon,
    delta: i8,
    input: Entity<InputState>,
    size: gpui_component::Size,
) -> Button {
    Button::new(id)
        .outline()
        .compact()
        .with_size(size)
        .child(icon(button_icon, 14.))
        .on_click(move |_, window, app| {
            input.update(app, |input, cx| {
                let current = input.value().parse::<i16>().unwrap_or_default();
                let value = (current + i16::from(delta)).clamp(0, 9);
                input.set_value(value.to_string(), window, cx);
            });
        })
}

fn compression_method_field(
    entity: gpui::Entity<Workspace>,
    options: Vec<(SharedString, SharedString)>,
) -> SettingField<SharedString> {
    dropdown_field(
        entity,
        options,
        |settings| {
            settings
                .archive
                .compression_method
                .clone()
                .unwrap_or_else(|| "default".to_owned())
        },
        |settings, value| {
            settings.archive.compression_method = if value == "default" {
                None
            } else {
                Some(value)
            };
        },
    )
}

/// Keeps every typed control on the same baseline and row height.
fn standard_field<T>(field: SettingField<T>) -> SettingField<T> {
    field.h(px(32.)).items_center()
}

fn bool_field(
    entity: gpui::Entity<Workspace>,
    get: fn(&SettingsFile) -> bool,
    set: fn(&mut SettingsFile, bool),
    default: bool,
) -> SettingField<bool> {
    let value_entity = entity.clone();
    standard_field(
        SettingField::switch(
            move |app| get(value_entity.read(app).settings()),
            move |value, app| {
                entity.update(app, |workspace, cx| {
                    workspace.update_settings(|settings| set(settings, value), cx);
                });
            },
        )
        .default_value(default),
    )
}

fn theme_field(entity: gpui::Entity<Workspace>, i18n: I18n) -> SettingField<SharedString> {
    let options = vec![
        (
            SharedString::from("system"),
            SharedString::from(i18n.text(MessageKey::System)),
        ),
        (
            SharedString::from("light"),
            SharedString::from(i18n.text(MessageKey::Light)),
        ),
        (
            SharedString::from("dark"),
            SharedString::from(i18n.text(MessageKey::Dark)),
        ),
    ];
    dropdown_field(
        entity,
        options,
        |settings| match settings.ui.theme {
            ThemePreference::System => "system".to_owned(),
            ThemePreference::Light => "light".to_owned(),
            ThemePreference::Dark => "dark".to_owned(),
        },
        |settings, value| {
            settings.ui.theme = match value.as_str() {
                "light" => ThemePreference::Light,
                "dark" => ThemePreference::Dark,
                _ => ThemePreference::System,
            };
        },
    )
}

fn language_field(entity: gpui::Entity<Workspace>, i18n: I18n) -> SettingField<SharedString> {
    let options = vec![
        (
            SharedString::from("system"),
            SharedString::from(i18n.text(MessageKey::System)),
        ),
        (
            SharedString::from("en"),
            SharedString::from(i18n.text(MessageKey::English)),
        ),
        (
            SharedString::from("zh_cn"),
            SharedString::from(i18n.text(MessageKey::SimplifiedChinese)),
        ),
    ];
    dropdown_field(
        entity,
        options,
        |settings| match settings.ui.language {
            LanguagePreference::System => "system".to_owned(),
            LanguagePreference::English => "en".to_owned(),
            LanguagePreference::ZhCn => "zh_cn".to_owned(),
        },
        |settings, value| {
            settings.ui.language = match value.as_str() {
                "en" => LanguagePreference::English,
                "zh_cn" => LanguagePreference::ZhCn,
                _ => LanguagePreference::System,
            };
        },
    )
}

fn list_mode_field(entity: gpui::Entity<Workspace>, i18n: I18n) -> SettingField<SharedString> {
    dropdown_field(
        entity,
        vec![
            (
                SharedString::from("details"),
                SharedString::from(i18n.text(MessageKey::Details)),
            ),
            (
                SharedString::from("list"),
                SharedString::from(i18n.text(MessageKey::List)),
            ),
        ],
        |settings| match settings.ui.list_mode {
            ListMode::Details => "details".to_owned(),
            ListMode::List => "list".to_owned(),
        },
        |settings, value| {
            settings.ui.list_mode = if value.as_str() == "list" {
                ListMode::List
            } else {
                ListMode::Details
            };
        },
    )
}

fn overwrite_field(entity: gpui::Entity<Workspace>, i18n: I18n) -> SettingField<SharedString> {
    dropdown_field(
        entity,
        vec![
            (
                SharedString::from("error"),
                SharedString::from(i18n.text(MessageKey::AskBeforeReplacing)),
            ),
            (
                SharedString::from("replace"),
                SharedString::from(i18n.text(MessageKey::ReplaceExisting)),
            ),
            (
                SharedString::from("skip"),
                SharedString::from(i18n.text(MessageKey::SkipExisting)),
            ),
        ],
        |settings| match settings.extraction.overwrite {
            OverwriteSetting::Error => "error".to_owned(),
            OverwriteSetting::Replace => "replace".to_owned(),
            OverwriteSetting::Skip => "skip".to_owned(),
        },
        |settings, value| {
            settings.extraction.overwrite = match value.as_str() {
                "replace" => OverwriteSetting::Replace,
                "skip" => OverwriteSetting::Skip,
                _ => OverwriteSetting::Error,
            };
        },
    )
}

fn dropdown_field(
    entity: gpui::Entity<Workspace>,
    options: Vec<(SharedString, SharedString)>,
    get: fn(&SettingsFile) -> String,
    set: fn(&mut SettingsFile, String),
) -> SettingField<SharedString> {
    let value_entity = entity.clone();
    standard_field(SettingField::dropdown(
        options,
        move |app| SharedString::from(get(value_entity.read(app).settings())),
        move |value, app| {
            entity.update(app, |workspace, cx| {
                workspace.update_settings(|settings| set(settings, value.to_string()), cx);
            });
        },
    ))
}

fn restore_defaults_field(
    entity: gpui::Entity<Workspace>,
    i18n: I18n,
) -> SettingField<SharedString> {
    SettingField::render(move |_, _, _| {
        let entity = entity.clone();
        Button::new("settings-restore-defaults")
            .outline()
            .h(px(32.))
            .child(
                h_flex()
                    .gap_1()
                    .child(icon(LucideIcon::Undo2, 14.))
                    .child(i18n.text(MessageKey::RestoreDefaults)),
            )
            .on_click(move |_, _, app| {
                entity.update(app, |workspace, cx| workspace.restore_defaults(cx));
            })
    })
}

fn icon(icon: LucideIcon, size: f32) -> gpui::Div {
    div()
        .w(px(size))
        .h(px(size))
        .flex()
        .items_center()
        .justify_center()
        .font_family("lucide")
        .text_size(px(size))
        .child(icon.unicode().to_string())
}
