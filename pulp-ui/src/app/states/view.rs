//! 视图拼装：将状态转换为 iced 组件树。

use super::App;
use super::helpers::tree_view_rows;
use crate::app::{components, config, themes};
use crate::domain::{DrawerPanel, Message, Page, THEME_MODES, ViewMode};
use iced::widget::{button, column, container, pick_list, row, rule, scrollable, text, text_input};
use iced::{Alignment, Element, Length};
use rust_i18n::t;

impl App {
    pub fn view(&self) -> Element<'_, Message> {
        let top = components::app_bar(
            self.menu_open,
            !self.back_stack.is_empty(),
            !self.forward_stack.is_empty(),
            &self.selected_path,
            &self.path,
            &self.filter,
            self.title_menu_open,
            self.location_editing,
        );

        let left_panel: Element<'_, Message> = if self.menu_open {
            components::sidebar(&self.selected_path)
        } else {
            container(column![]).width(Length::Fixed(0.0)).into()
        };

        let main_content: Element<'_, Message> = if self.page == Page::Settings {
            let theme_pick = pick_list(
                THEME_MODES,
                Some(self.theme_mode),
                Message::ThemeModeChanged,
            )
            .placeholder(rust_i18n::t!("menu.settings.theme").to_string())
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
            let header = components::table_header();
            let rows = match self.view_mode {
                ViewMode::FileSystem => components::file_entries(
                    &self.rendered_rows,
                    self.selected_entry.as_ref(),
                    self.list_viewport,
                ),
                ViewMode::FileSystemTree | ViewMode::ArchiveTree => tree_view_rows(
                    &self.tree_render_rows,
                    &self.tree_expanded,
                    self.selected_entry.as_ref(),
                    self.list_viewport,
                ),
                ViewMode::Archive => components::file_entries(
                    &self.archive_rendered_rows,
                    self.selected_entry.as_ref(),
                    self.list_viewport,
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

        let drawer = components::drawer(
            self.drawer_open,
            self.drawer_panel,
            self.drawer_width_px,
            self.drawer_resizing,
            drawer_panel_content,
        );

        let main_row = components::main_split(left_panel, main_content, drawer);

        let item_count = match self.view_mode {
            ViewMode::FileSystem | ViewMode::FileSystemTree => self.entries.len(),
            ViewMode::Archive | ViewMode::ArchiveTree => self.archive_entries.len(),
        };

        let status_bar =
            components::status_bar(item_count, &self.status, self.busy, self.spinner_index);

        components::app_shell(top, main_row, status_bar)
    }
}
