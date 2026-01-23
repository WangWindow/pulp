//! 视图拼装：将状态转换为 iced 组件树。

use super::App;
use super::helpers::{build_entry_context_menu, tree_view_rows};
use crate::app::config;
use crate::app::themes;
use crate::components;
use crate::components::common::dialog::{DialogStyle, confirm_dialog};
use crate::components::{
    AppBarActions, AppBarStyle, AppBarText, DrawerActions, DrawerStyle, DrawerText,
    FileListActions, FileListStyle, LayoutStyle, MenuEntry, MenuStyle, SidebarActions, SidebarMenu,
    SidebarStyle, SidebarText, StatusBarStyle,
};
use crate::domain::{DrawerPanel, Message, Page, THEME_MODES, ViewMode};
use iced::widget::{
    Stack, button, column, container, pick_list, row, rule, scrollable, text, text_input,
};
use iced::{Alignment, Element, Length};
use icondata::{RiFile2DocumentLine, RiFolder2DocumentLine, RiSettings3SystemLine};
use rust_i18n::t;

fn toggle_menu(_open: bool) -> Message {
    Message::ToggleMenu
}

impl App {
    pub fn view(&self) -> Element<'_, Message> {
        let theme = self.theme();
        let menu_style = MenuStyle {
            icon_color: themes::icon_color(&theme),
            item_style: themes::styles::menu_item_style,
            panel_style: themes::styles::menu_panel_style,
        };

        let app_bar_style = AppBarStyle {
            icon_color: themes::icon_color(&theme),
            button_style: themes::styles::ghost_button_style,
            container_style: themes::styles::appbar_style,
        };

        let layout_style = LayoutStyle {
            container_style: themes::styles::app_style,
        };

        let drawer_style = DrawerStyle {
            divider_style: themes::styles::drawer_divider_style,
            divider_active_style: themes::styles::drawer_divider_active_style,
            panel_style: themes::styles::panel_style,
            button_style: themes::styles::ghost_button_style,
        };

        let sidebar_style = SidebarStyle {
            icon_color: themes::icon_color(&theme),
            panel_style: themes::styles::panel_style,
            list_row_style: themes::styles::list_row_style,
            action_button_style: themes::styles::round_action_button_style,
        };

        let status_bar_style = StatusBarStyle {
            panel_style: themes::styles::panel_style,
        };

        let file_list_style = FileListStyle {
            icon_color: themes::icon_color(&theme),
            list_row_style: themes::styles::list_row_style,
        };

        let can_back = !self.back_stack.is_empty();
        let can_forward = !self.forward_stack.is_empty();

        let app_bar_actions = AppBarActions {
            on_toggle_menu: toggle_menu,
            on_back: if can_back {
                Some(Message::NavigateBack)
            } else {
                None
            },
            on_forward: if can_forward {
                Some(Message::NavigateForward)
            } else {
                None
            },
            on_up: Message::NavigateUp,
            on_home: Message::NavigateHome,
            on_path_changed: Message::PathChanged,
            on_path_submit: Message::PathSubmitted,
            on_filter_changed: Message::FilterChanged,
            on_toggle_location_edit: Message::ToggleLocationEdit,
            on_toggle_settings: Message::ToggleSettings,
            on_toggle_title_menu: Message::ToggleTitleMenu,
            on_dismiss_title_menu: Message::DismissTitleMenu,
            on_navigate_to: Message::NavigateTo,
        };

        let file_list_actions = FileListActions {
            on_row_clicked: Message::RowClicked,
            on_dismiss_menu: Message::DismissContextMenu,
        };

        let drawer_actions = DrawerActions {
            on_close: Message::CloseDrawer,
            on_resize_start: Message::DrawerResizeStart,
        };

        let sidebar_actions = SidebarActions {
            on_navigate: Message::NavigateTo,
            on_swipe_start: Message::SidebarSwipeStart,
        };

        let app_bar_text = AppBarText {
            path_input_placeholder: t!("appbar.path_input_placeholder").to_string(),
            search_placeholder: t!("appbar.search_placeholder").to_string(),
            location_done: t!("appbar.location_done").to_string(),
            location_edit: t!("appbar.location_edit").to_string(),
        };

        let title_menu_entries = vec![
            MenuEntry::item(
                t!("appbar.new_folder").to_string(),
                RiFolder2DocumentLine,
                Message::NewFolderRequested,
            ),
            MenuEntry::separator(),
            MenuEntry::item(
                t!("appbar.toggle_view").to_string(),
                RiFile2DocumentLine,
                Message::ToggleFileViewMode,
            ),
            MenuEntry::separator(),
            MenuEntry::item(
                t!("menu.settings").to_string(),
                RiSettings3SystemLine,
                Message::ToggleSettings,
            ),
        ];

        let top = components::app_bar(
            self.menu_open,
            &self.selected_path,
            &self.path,
            &self.filter,
            self.title_menu_open,
            self.location_editing,
            app_bar_text,
            title_menu_entries,
            menu_style,
            app_bar_style,
            app_bar_actions,
        );

        let mount_supported = crate::utils::mounts::mount_supported();
        let sidebar_items =
            crate::utils::mounts::load_sidebar_items_cached(self.effective_locale.as_str());
        let left_panel: Element<'_, Message> = if self.menu_open {
            components::sidebar(
                &self.selected_path,
                sidebar_items,
                SidebarText {
                    places: t!("sidebar.places").to_string(),
                    volumes: t!("sidebar.volumes").to_string(),
                    read_only: t!("sidebar.read_only").to_string(),
                },
                sidebar_style,
                sidebar_actions,
                Some(SidebarMenu {
                    mount_supported,
                    menu_style,
                    on_mount: Message::MountRequested,
                    on_unmount: Message::UnmountConfirmRequested,
                    on_dismiss: Message::DismissContextMenu,
                    swipe_open_device: self.sidebar_swipe_open.clone(),
                }),
            )
        } else {
            container(column![]).width(Length::Fixed(0.0)).into()
        };

        let main_content: Element<'_, Message> = if self.page == Page::Settings {
            let theme_labels: Vec<String> = THEME_MODES
                .iter()
                .map(|m| t!(m.label_key()).to_string())
                .collect();

            let theme_selected_index = THEME_MODES.iter().position(|m| *m == self.theme_mode);

            let theme_pick = pick_list(
                theme_labels.clone(),
                theme_selected_index.map(|i| theme_labels[i].clone()),
                move |picked: String| {
                    let idx = theme_labels.iter().position(|s| *s == picked).unwrap_or(0);
                    Message::ThemeModeChanged(THEME_MODES[idx])
                },
            )
            .placeholder(t!("menu.settings.theme").to_string())
            .width(Length::Fixed(180.0));

            let cfg = config::load().unwrap_or_default();
            let current_pref = cfg.locale_preference();

            let locale_options: [crate::i18n::LocalePreference; 3] = [
                crate::i18n::LocalePreference::FollowSystem,
                crate::i18n::LocalePreference::Fixed(crate::i18n::AppLocale::En),
                crate::i18n::LocalePreference::Fixed(crate::i18n::AppLocale::ZhCn),
            ];

            let selected_index: Option<usize> =
                locale_options.iter().position(|p| *p == current_pref);

            let locale_labels: Vec<String> = locale_options
                .iter()
                .map(|p| rust_i18n::t!(p.label_key()).to_string())
                .collect();

            let language_pick = pick_list(
                locale_labels.clone(),
                selected_index.map(|i| locale_labels[i].clone()),
                move |picked: String| {
                    let idx = locale_labels.iter().position(|s| *s == picked).unwrap_or(0);
                    Message::LocalePreferenceChanged(locale_options[idx])
                },
            )
            .placeholder(rust_i18n::t!("menu.settings.language").to_string())
            .width(Length::Fixed(220.0));

            let rollback_btn = button(text(rust_i18n::t!(
                "menu.settings.language.rollback_to_system"
            )))
            .style(themes::styles::ghost_button_style)
            .padding([6, 10])
            .on_press(Message::LocaleRollbackToSystem);

            container(
                column![
                    text(rust_i18n::t!("menu.settings")).size(20),
                    rule::horizontal(1),
                    row![
                        text(rust_i18n::t!("menu.settings.language")),
                        language_pick,
                        rollback_btn
                    ]
                    .spacing(12)
                    .align_y(iced::Alignment::Center),
                    row![text(rust_i18n::t!("menu.settings.theme")), theme_pick]
                        .spacing(12)
                        .align_y(iced::Alignment::Center),
                    text(rust_i18n::t!("app.system_default")).size(12),
                ]
                .spacing(12)
                .padding(12),
            )
            .style(themes::styles::panel_style)
            .into()
        } else {
            let header = components::table_header(
                [
                    rust_i18n::t!("files.column.name").to_string(),
                    rust_i18n::t!("files.column.size").to_string(),
                    rust_i18n::t!("files.column.type").to_string(),
                    rust_i18n::t!("files.column.modified").to_string(),
                ],
                Message::ToggleFileViewMode,
                components::lists::table_header::TableHeaderStyle {
                    icon_color: themes::icon_color(&theme),
                    button_style: themes::styles::ghost_button_style,
                    container_style: themes::styles::appbar_style,
                },
            );
            let selected_row_path = self
                .selected_entry
                .as_deref()
                .unwrap_or(self.selected_path.as_path());

            let rows = match self.view_mode {
                ViewMode::FileSystem => components::file_entries(
                    &self.rendered_rows,
                    selected_row_path,
                    self.list_viewport,
                    menu_style,
                    file_list_style,
                    file_list_actions,
                    build_entry_context_menu,
                ),
                ViewMode::FileSystemTree | ViewMode::ArchiveTree => tree_view_rows(
                    &self.tree_render_rows,
                    &self.tree_expanded,
                    selected_row_path,
                    self.list_viewport,
                    menu_style,
                    build_entry_context_menu,
                ),
                ViewMode::Archive => components::file_entries(
                    &self.archive_rendered_rows,
                    selected_row_path,
                    self.list_viewport,
                    menu_style,
                    file_list_style,
                    file_list_actions,
                    build_entry_context_menu,
                ),
            };
            let entries = column![header, rule::horizontal(1), rows].spacing(6);
            container(
                scrollable(entries)
                    .on_scroll(Message::ListViewportChanged)
                    .height(Length::Fill),
            )
            .padding(10)
            .width(Length::Fill)
            .style(themes::styles::panel_style)
            .into()
        };

        let drawer_panel_content: Element<'_, Message> = match self.drawer_panel {
            DrawerPanel::Extract => {
                if self.extract_to_target.is_some() {
                    container(
                        column![
                            text(t!("drawer.extract.title")).size(16),
                            rule::horizontal(1),
                            text_input(
                                t!("drawer.extract.path_label").as_ref(),
                                &self.extract_to_path
                            )
                            .on_input(Message::ExtractToChanged)
                            .padding(8)
                            .width(Length::Fill),
                            row![
                                button(text(t!("drawer.common.cancel")))
                                    .style(themes::styles::ghost_button_style)
                                    .padding([6, 10])
                                    .on_press(Message::ExtractToCancel),
                                button(text(t!("drawer.extract.confirm")))
                                    .style(themes::styles::ghost_button_style)
                                    .padding([6, 10])
                                    .on_press(Message::ExtractToConfirm),
                            ]
                            .spacing(10)
                            .align_y(iced::Alignment::Center),
                        ]
                        .spacing(12),
                    )
                    .width(Length::Fill)
                    .into()
                } else {
                    container(
                        column![
                            text(t!("drawer.extract.title")).size(16),
                            rule::horizontal(1),
                            text(t!("drawer.extract.empty_hint")).size(12),
                        ]
                        .spacing(10),
                    )
                    .width(Length::Fill)
                    .into()
                }
            }
            DrawerPanel::Rename => {
                if self.rename_target.is_some() {
                    container(
                        column![
                            text(t!("drawer.rename.title")).size(16),
                            rule::horizontal(1),
                            text_input(t!("drawer.rename.input_label").as_ref(), &self.rename_name)
                                .on_input(Message::RenameChanged)
                                .padding(8)
                                .width(Length::Fill),
                            row![
                                button(text(t!("drawer.common.cancel")))
                                    .style(themes::styles::ghost_button_style)
                                    .padding([6, 10])
                                    .on_press(Message::RenameCancel),
                                button(text(t!("drawer.common.confirm")))
                                    .style(themes::styles::ghost_button_style)
                                    .padding([6, 10])
                                    .on_press(Message::RenameConfirm),
                            ]
                            .spacing(10)
                            .align_y(iced::Alignment::Center),
                        ]
                        .spacing(12),
                    )
                    .width(Length::Fill)
                    .into()
                } else {
                    container(
                        column![
                            text(t!("drawer.rename.title")).size(16),
                            rule::horizontal(1),
                            text(t!("drawer.rename.empty_hint")).size(12),
                        ]
                        .spacing(10),
                    )
                    .width(Length::Fill)
                    .into()
                }
            }
            DrawerPanel::NewFolder => {
                if self.new_folder_open {
                    container(
                        column![
                            text(t!("drawer.new_folder.title")).size(16),
                            rule::horizontal(1),
                            text_input(
                                t!("drawer.new_folder.input_label").as_ref(),
                                &self.new_folder_name
                            )
                            .on_input(Message::NewFolderChanged)
                            .padding(8)
                            .width(Length::Fill),
                            row![
                                button(text(t!("drawer.common.cancel")))
                                    .style(themes::styles::ghost_button_style)
                                    .padding([6, 10])
                                    .on_press(Message::NewFolderCancel),
                                button(text(t!("drawer.new_folder.confirm")))
                                    .style(themes::styles::ghost_button_style)
                                    .padding([6, 10])
                                    .on_press(Message::NewFolderConfirm),
                            ]
                            .spacing(10)
                            .align_y(iced::Alignment::Center),
                        ]
                        .spacing(12),
                    )
                    .width(Length::Fill)
                    .into()
                } else {
                    container(
                        column![
                            text(t!("drawer.new_folder.title")).size(16),
                            rule::horizontal(1),
                            text(t!("drawer.new_folder.empty_hint")).size(12),
                        ]
                        .spacing(10),
                    )
                    .width(Length::Fill)
                    .into()
                }
            }
            DrawerPanel::DeleteConfirm => {
                if let Some(target) = self.delete_target.clone() {
                    let name = target
                        .file_name()
                        .map(|s| s.to_string_lossy().to_string())
                        .unwrap_or_else(|| target.display().to_string());

                    container(
                        column![
                            text(t!("drawer.delete.confirm_title")).size(16),
                            rule::horizontal(1),
                            text(t!("drawer.delete.target", name = name)).size(12),
                            row![
                                button(text(t!("drawer.common.cancel")))
                                    .style(themes::styles::ghost_button_style)
                                    .padding([6, 10])
                                    .on_press(Message::DeleteCancel),
                                button(text(t!("drawer.delete.confirm")))
                                    .style(themes::styles::ghost_button_style)
                                    .padding([6, 10])
                                    .on_press(Message::DeleteConfirm),
                            ]
                            .spacing(10)
                            .align_y(iced::Alignment::Center),
                        ]
                        .spacing(12),
                    )
                    .width(Length::Fill)
                    .into()
                } else {
                    container(
                        column![
                            text(t!("drawer.delete.title")).size(16),
                            rule::horizontal(1),
                            text(t!("drawer.delete.empty_hint")).size(12),
                        ]
                        .spacing(10),
                    )
                    .width(Length::Fill)
                    .into()
                }
            }
            DrawerPanel::Task => {
                let queue_count = self.task_queue.len();
                let queue_preview: Vec<String> = self
                    .task_queue
                    .iter()
                    .take(3)
                    .map(|task| task.label())
                    .collect();
                let queue_preview_len = queue_preview.len();

                let title = self
                    .active_task_title
                    .clone()
                    .unwrap_or_else(|| t!("drawer.panel.task").to_string());

                let subtitle = if self.busy {
                    if self.active_task_cancelled {
                        t!("task.state.cancelling").to_string()
                    } else {
                        t!("task.state.running").to_string()
                    }
                } else if self.active_task_finished {
                    if self.active_task_error.is_some() {
                        t!("task.state.failed").to_string()
                    } else if self.active_task_cancelled {
                        t!("task.state.cancelled").to_string()
                    } else {
                        t!("task.state.completed").to_string()
                    }
                } else {
                    t!("task.state.ready").to_string()
                };

                let current = self
                    .active_task_current_entry
                    .as_ref()
                    .map(|s| s.as_str())
                    .unwrap_or("—");

                let progress_text = self
                    .active_task_progress
                    .map(|p| format!("{:.0}%", (p * 100.0).clamp(0.0, 100.0)))
                    .unwrap_or_else(|| "—".into());

                let cancel_btn = if self.busy && !self.active_task_cancelled {
                    button(text(t!("drawer.common.cancel")))
                        .style(themes::styles::ghost_button_style)
                        .padding([6, 10])
                        .on_press(Message::TaskCancelRequested)
                } else {
                    button(text(t!("drawer.common.cancel")))
                        .style(themes::styles::ghost_button_style)
                        .padding([6, 10])
                };

                let error_block: Element<'_, Message> =
                    if let Some(err) = self.active_task_error.as_ref() {
                        container(
                            column![text(t!("task.label.error")).size(14), text(err).size(12),]
                                .spacing(6),
                        )
                        .width(Length::Fill)
                        .style(themes::styles::panel_style)
                        .into()
                    } else {
                        container(text("")).width(Length::Fill).into()
                    };

                let queue_block: Element<'_, Message> = if queue_count == 0 {
                    container(
                        column![
                            text(t!("task.queue.title")).size(14),
                            text(t!("task.queue.empty")).size(12),
                        ]
                        .spacing(6),
                    )
                    .width(Length::Fill)
                    .into()
                } else {
                    let mut list = column![text(t!("task.queue.title")).size(14)].spacing(6);
                    list = list.push(text(t!("task.queue.count", count = queue_count)).size(12));
                    for item in queue_preview.into_iter() {
                        list = list.push(text(item).size(12));
                    }
                    if queue_count > queue_preview_len {
                        list = list.push(text("…").size(12));
                    }
                    container(list).width(Length::Fill).into()
                };

                container(
                    column![
                        text(title).size(16),
                        rule::horizontal(1),
                        row![
                            text(t!("task.label.status")).size(12),
                            container(text(subtitle).size(12)).width(Length::Fill),
                        ]
                        .spacing(10)
                        .align_y(Alignment::Center),
                        row![
                            text(t!("task.label.current")).size(12),
                            container(text(current).size(12)).width(Length::Fill),
                        ]
                        .spacing(10)
                        .align_y(Alignment::Center),
                        row![
                            text(t!("task.label.progress")).size(12),
                            container(text(progress_text).size(12)).width(Length::Fill),
                        ]
                        .spacing(10)
                        .align_y(Alignment::Center),
                        queue_block,
                        error_block,
                        row![cancel_btn].spacing(10).align_y(Alignment::Center),
                    ]
                    .spacing(12),
                )
                .width(Length::Fill)
                .into()
            }
        };

        let drawer_text = DrawerText {
            panel_task: t!("drawer.panel.task").to_string(),
            panel_extract: t!("drawer.panel.extract").to_string(),
            panel_rename: t!("drawer.panel.rename").to_string(),
            panel_new_folder: t!("drawer.panel.new_folder").to_string(),
            panel_delete_confirm: t!("drawer.panel.delete_confirm").to_string(),
        };

        let drawer = components::drawer(
            self.drawer_open,
            self.drawer_panel,
            self.drawer_width_px,
            self.drawer_resizing,
            drawer_panel_content,
            drawer_text,
            drawer_style,
            drawer_actions,
        );

        let main_row = components::main_split(left_panel, main_content, drawer);

        let item_count = match self.view_mode {
            ViewMode::FileSystem | ViewMode::FileSystemTree => self.entries.len(),
            ViewMode::Archive | ViewMode::ArchiveTree => self.archive_entries.len(),
        };

        let items_label = t!("status.items", count = item_count).to_string();
        let status_text = if self.status == items_label {
            String::new()
        } else {
            self.status.clone()
        };
        let status_bar = components::status_bar(
            items_label,
            status_text,
            self.busy,
            self.spinner_index,
            status_bar_style,
        );

        let app_shell = components::app_shell(top, main_row, status_bar, layout_style);

        let mut overlays: Vec<Element<'_, Message>> = vec![app_shell];

        if self.properties_open {
            let content = self
                .properties_content
                .clone()
                .unwrap_or_else(|| "—".to_string());

            let modal = container(
                column![
                    row![
                        text(t!("files.context.properties")).size(16),
                        button(text(t!("app.close")))
                            .style(themes::styles::ghost_button_style)
                            .padding([4, 10])
                            .on_press(Message::PropertiesClose),
                    ]
                    .spacing(10)
                    .align_y(Alignment::Center),
                    rule::horizontal(1),
                    text(content).size(12),
                ]
                .spacing(12),
            )
            .padding(12)
            .width(Length::Fixed(420.0))
            .style(themes::styles::panel_style);

            let overlay: Element<'_, Message> = Stack::with_children(vec![
                button(
                    container(text(""))
                        .center_x(Length::Fill)
                        .center_y(Length::Fill)
                        .style(themes::styles::modal_backdrop_style),
                )
                .padding(0)
                .style(themes::styles::ghost_button_style)
                .on_press(Message::PropertiesClose)
                .into(),
                container(modal)
                    .center_x(Length::Fill)
                    .center_y(Length::Fill)
                    .padding(20)
                    .into(),
            ])
            .into();

            overlays.push(overlay);
        }

        if self.unmount_confirm_open {
            let device = self
                .unmount_confirm_device
                .clone()
                .unwrap_or_else(|| "—".to_string());
            let overlay = confirm_dialog(
                t!("dialog.unmount.title").to_string(),
                t!("dialog.unmount.body", device = device).to_string(),
                t!("dialog.unmount.confirm").to_string(),
                t!("app.cancel").to_string(),
                Message::UnmountConfirmAccept,
                Message::UnmountConfirmCancel,
                DialogStyle {
                    panel_style: themes::styles::panel_style,
                    backdrop_style: themes::styles::modal_backdrop_style,
                    button_style: themes::styles::ghost_button_style,
                },
            );
            overlays.push(overlay);
        }

        if overlays.len() == 1 {
            return overlays.pop().unwrap_or_else(|| container(text("")).into());
        }

        Stack::with_children(overlays).into()
    }
}
