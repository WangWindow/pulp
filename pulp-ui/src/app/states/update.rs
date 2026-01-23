//! iced 更新逻辑：消息分发与状态转换。

use super::App;
use super::app::SidebarSwipeTarget;
use super::helpers::{
    build_archive_children_index, build_rows_from_fs, build_tree_render_rows, parse_virtual_path,
};
use super::tasks::UiTask;
use crate::app::config;
use crate::domain::{
    ContextAction, DOUBLE_CLICK_GAP_MS, DRAWER_MAX_WIDTH_RATIO, DRAWER_MIN_WIDTH_PX, DrawerPanel,
    EntryRow, EntrySource, GlobalEvent, Message, Page, ViewMode, handle_global_event,
};
use crate::utils;
use crate::utils::fs::{apply_filter, load_directory, open_path};
use iced::Task;
use pulp_core::ArchiveFormat;
use rust_i18n::t;
use std::path::PathBuf;
use std::time::{Duration, Instant};

impl App {
    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Noop => Task::none(),
            // -------------------------------
            // 视图切换：平铺 <-> 树状列表
            // -------------------------------
            Message::ToggleFileViewMode => {
                self.view_mode = match self.view_mode {
                    ViewMode::FileSystem => ViewMode::FileSystemTree,
                    ViewMode::FileSystemTree => ViewMode::FileSystem,
                    ViewMode::Archive => ViewMode::ArchiveTree,
                    ViewMode::ArchiveTree => ViewMode::Archive,
                };

                if self.view_mode == ViewMode::FileSystemTree {
                    self.tree_expanded.clear();
                    self.fs_children_cache.clear();
                    self.fs_tree_loading.clear();

                    let roots = self.rendered_rows.clone();
                    self.tree_render_rows = build_tree_render_rows(
                        &roots,
                        &self.tree_expanded,
                        &self.fs_children_cache,
                    );
                } else if self.view_mode == ViewMode::ArchiveTree {
                    self.tree_expanded.clear();

                    if let Some(archive_path) = self.active_archive.clone() {
                        let archive_name = archive_path
                            .file_name()
                            .map(|s| s.to_string_lossy().to_string())
                            .unwrap_or_else(|| "archive".to_string());

                        self.archive_children_index = build_archive_children_index(
                            &archive_name,
                            &archive_path,
                            &self.archive_entries,
                        );

                        let root_key = PathBuf::from(format!("{archive_name}::/"));
                        let roots = self
                            .archive_children_index
                            .get(&root_key)
                            .cloned()
                            .unwrap_or_default();

                        self.tree_render_rows = build_tree_render_rows(
                            &roots,
                            &self.tree_expanded,
                            &self.archive_children_index,
                        );
                    } else {
                        self.archive_children_index.clear();
                        self.tree_render_rows.clear();
                    }
                }

                Task::none()
            }
            // -------------------------------
            // 树状列表：展开/折叠与按需加载
            // -------------------------------
            Message::TreeToggle(dir_key, expanded) => {
                if expanded {
                    self.tree_expanded.insert(dir_key.clone());
                } else {
                    self.tree_expanded.remove(&dir_key);
                }

                if self.view_mode == ViewMode::FileSystemTree {
                    if !expanded {
                        let roots = self.rendered_rows.clone();
                        self.tree_render_rows = build_tree_render_rows(
                            &roots,
                            &self.tree_expanded,
                            &self.fs_children_cache,
                        );
                        return Task::none();
                    }

                    let dir_path_str = dir_key.display().to_string();
                    let real_dir = PathBuf::from(dir_path_str.trim_end_matches('/'));

                    if !self.fs_children_cache.contains_key(&dir_key)
                        && !self.fs_tree_loading.contains(&dir_key)
                    {
                        self.fs_tree_loading.insert(dir_key.clone());

                        return Task::perform(load_directory(real_dir), |(path, children)| {
                            Message::TreeChildrenLoaded(path, children)
                        });
                    }

                    let roots = self.rendered_rows.clone();
                    self.tree_render_rows = build_tree_render_rows(
                        &roots,
                        &self.tree_expanded,
                        &self.fs_children_cache,
                    );
                    return Task::none();
                }

                if self.view_mode == ViewMode::ArchiveTree {
                    if let Some(archive_path) = self.active_archive.clone() {
                        let archive_name = archive_path
                            .file_name()
                            .map(|s| s.to_string_lossy().to_string())
                            .unwrap_or_else(|| "archive".to_string());

                        let root_key = PathBuf::from(format!("{archive_name}::/"));
                        let roots = self
                            .archive_children_index
                            .get(&root_key)
                            .cloned()
                            .unwrap_or_default();

                        self.tree_render_rows = build_tree_render_rows(
                            &roots,
                            &self.tree_expanded,
                            &self.archive_children_index,
                        );
                    }
                    return Task::none();
                }

                Task::none()
            }
            Message::TreeChildrenLoaded(real_dir, children) => {
                if self.view_mode != ViewMode::FileSystemTree {
                    return Task::none();
                }

                let dir_key = PathBuf::from(format!("{}/", real_dir.display()));

                self.fs_tree_loading.remove(&dir_key);

                let child_rows = build_rows_from_fs(&children);
                self.fs_children_cache.insert(dir_key.clone(), child_rows);

                if self.tree_expanded.contains(&dir_key) {
                    let roots = self.rendered_rows.clone();
                    self.tree_render_rows = build_tree_render_rows(
                        &roots,
                        &self.tree_expanded,
                        &self.fs_children_cache,
                    );
                }

                Task::none()
            }
            Message::TaskCancelRequested => {
                if let Some(token) = self.active_task_cancel.as_ref() {
                    token.cancel();
                    self.active_task_cancelled = true;
                    self.status = t!("status.cancelling").to_string();
                }
                Task::none()
            }
            Message::ListViewportChanged(viewport) => {
                self.list_viewport = Some(viewport);
                Task::none()
            }
            Message::AutoRefreshTick => {
                if self.busy
                    || self.page != Page::Browser
                    || !matches!(self.view_mode, ViewMode::FileSystem)
                {
                    return Task::none();
                }

                let path = self.selected_path.clone();
                Task::perform(load_directory(path), |(path, entries)| {
                    Message::DirLoaded(path, entries)
                })
            }
            Message::CloseDrawer => {
                self.drawer_open = false;
                self.drawer_resizing = false;
                Task::none()
            }
            Message::DrawerResizeStart => {
                self.drawer_open = true;
                self.drawer_resizing = true;
                self.drawer_resize_last_cursor_x = None;
                Task::none()
            }
            Message::ToggleTitleMenu => {
                self.title_menu_open = !self.title_menu_open;
                Task::none()
            }
            Message::DismissTitleMenu => {
                self.title_menu_open = false;
                Task::none()
            }
            Message::DismissContextMenu => {
                // 右键菜单的 overlay 关闭：应与标题菜单的 open 状态解耦，
                // 以避免“关闭右键菜单顺便关闭标题菜单”的副作用。
                Task::none()
            }
            Message::ToggleMenu => {
                self.menu_open = !self.menu_open;
                Task::none()
            }
            Message::ToggleSettings => {
                self.page = if self.page == Page::Settings {
                    Page::Browser
                } else {
                    Page::Settings
                };

                if self.page == Page::Settings {
                    let cfg = config::load().unwrap_or_default();
                    let preference = cfg.locale_preference();
                    let system_locale =
                        crate::i18n::normalize_system_locale(sys_locale::get_locale());
                    let locale_state = crate::i18n::LocaleState::resolve(
                        preference,
                        system_locale.as_deref(),
                        crate::i18n::AppLocale::En,
                    );
                    rust_i18n::set_locale(locale_state.effective_locale_str());
                    self.effective_locale = locale_state.effective;
                }

                Task::none()
            }
            Message::ToggleLocationEdit => {
                self.location_editing = !self.location_editing;
                if self.location_editing {
                    self.path = self.selected_path.display().to_string();
                }
                Task::none()
            }

            // -------------------------------
            // Settings / Preferences（语言）
            // -------------------------------
            Message::LocalePreferenceChanged(pref) => {
                let mut cfg = config::load().unwrap_or_default();
                cfg.set_locale_preference(pref);

                if let Err(e) = config::save(&cfg) {
                    self.status =
                        t!("status.save_settings_failed", error = e.to_string()).to_string();
                }

                let system_locale = crate::i18n::normalize_system_locale(sys_locale::get_locale());
                let locale_state = crate::i18n::LocaleState::resolve(
                    cfg.locale_preference(),
                    system_locale.as_deref(),
                    crate::i18n::AppLocale::En,
                );
                rust_i18n::set_locale(locale_state.effective_locale_str());
                self.effective_locale = locale_state.effective;

                Task::none()
            }
            Message::LocaleRollbackToSystem => {
                let mut cfg = config::load().unwrap_or_default();
                cfg.set_locale_preference(crate::i18n::LocalePreference::FollowSystem);

                if let Err(e) = config::save(&cfg) {
                    self.status =
                        t!("status.save_settings_failed", error = e.to_string()).to_string();
                }

                let system_locale = crate::i18n::normalize_system_locale(sys_locale::get_locale());
                let locale_state = crate::i18n::LocaleState::resolve(
                    crate::i18n::LocalePreference::FollowSystem,
                    system_locale.as_deref(),
                    crate::i18n::AppLocale::En,
                );
                rust_i18n::set_locale(locale_state.effective_locale_str());
                self.effective_locale = locale_state.effective;

                Task::none()
            }
            Message::ThemeModeChanged(mode) => {
                self.theme_mode = mode;
                self.title_menu_open = false;
                Task::none()
            }

            Message::NewFolderRequested => {
                self.title_menu_open = false;
                self.new_folder_open = true;
                self.new_folder_name = t!("status.default_new_folder_name").to_string();

                self.drawer_open = true;
                self.drawer_panel = DrawerPanel::NewFolder;

                Task::none()
            }
            Message::NewFolderChanged(value) => {
                self.new_folder_name = value;
                Task::none()
            }
            Message::NewFolderCancel => {
                self.new_folder_open = false;

                if self.drawer_open && self.drawer_panel == DrawerPanel::NewFolder {
                    self.drawer_panel = DrawerPanel::Task;
                }

                Task::none()
            }
            Message::NewFolderConfirm => {
                self.new_folder_open = false;

                self.drawer_open = true;
                self.drawer_panel = DrawerPanel::Task;

                let parent = self.selected_path.clone();
                let name = self.new_folder_name.clone();
                self.active_task_title = Some(t!("task.create_folder").to_string());
                self.active_task_current_entry = None;
                self.active_task_progress = None;
                self.active_task_finished = false;
                self.active_task_cancelled = false;
                self.active_task_error = None;
                self.active_task_cancel = None;

                return self.enqueue_task(UiTask::CreateFolder { parent, name });
            }
            Message::RenameRequested(target) => {
                self.selected_entry = Some(target.clone());
                self.rename_target = Some(target.clone());
                self.rename_name = target
                    .file_name()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_default();

                self.drawer_open = true;
                self.drawer_panel = DrawerPanel::Rename;

                Task::none()
            }
            Message::RenameChanged(value) => {
                self.rename_name = value;
                Task::none()
            }
            Message::RenameCancel => {
                self.rename_target = None;
                self.rename_name.clear();

                if self.drawer_open && self.drawer_panel == DrawerPanel::Rename {
                    self.drawer_panel = DrawerPanel::Task;
                }

                Task::none()
            }
            Message::RenameConfirm => {
                let Some(from) = self.rename_target.clone() else {
                    return Task::none();
                };
                let name = self.rename_name.trim();
                if name.is_empty() {
                    self.status = t!("status.rename_empty_name").to_string();
                    return Task::none();
                }
                let Some(parent) = from.parent().map(|p| p.to_path_buf()) else {
                    self.status = t!("status.rename_invalid_path").to_string();
                    return Task::none();
                };

                let to = parent.join(name);
                self.rename_target = None;
                self.rename_name.clear();

                self.drawer_open = true;
                self.drawer_panel = DrawerPanel::Task;

                self.active_task_title = Some(t!("task.rename").to_string());
                self.active_task_current_entry = None;
                self.active_task_progress = None;
                self.active_task_finished = false;
                self.active_task_cancelled = false;
                self.active_task_error = None;
                self.active_task_cancel = None;

                return self.enqueue_task(UiTask::Rename { from, to });
            }
            Message::DeleteRequested(target) => {
                self.selected_entry = Some(target.clone());
                self.delete_target = Some(target);

                self.drawer_open = true;
                self.drawer_panel = DrawerPanel::DeleteConfirm;

                Task::none()
            }
            Message::DeleteCancel => {
                self.delete_target = None;

                if self.drawer_open && self.drawer_panel == DrawerPanel::DeleteConfirm {
                    self.drawer_panel = DrawerPanel::Task;
                }

                Task::none()
            }
            Message::DeleteConfirm => {
                let Some(target) = self.delete_target.clone() else {
                    return Task::none();
                };
                self.delete_target = None;

                self.drawer_open = true;
                self.drawer_panel = DrawerPanel::Task;

                self.active_task_title = Some(t!("task.delete").to_string());
                self.active_task_current_entry = None;
                self.active_task_progress = None;
                self.active_task_finished = false;
                self.active_task_cancelled = false;
                self.active_task_error = None;
                self.active_task_cancel = None;

                return self.enqueue_task(UiTask::Delete { target });
            }
            Message::DeleteFinished(result) => {
                self.active_task_finished = true;
                self.busy = false;
                match result {
                    Ok(()) => {
                        self.status = t!("status.deleted").to_string();
                        self.active_task_error = None;
                        self.active_task_progress = None;
                        self.active_task_current_entry = None;
                        self.active_task_cancel = None;

                        let cwd = self.selected_path.clone();
                        let refresh =
                            Task::perform(load_directory(cwd), |(p, e)| Message::DirLoaded(p, e));
                        return Task::batch(vec![refresh, self.finish_task_and_next()]);
                    }
                    Err(message) => {
                        self.status = t!("status.failed", message = message.as_str()).to_string();
                        self.active_task_error = Some(message);
                    }
                }
                self.finish_task_and_next()
            }
            Message::OpenFinished(result) => {
                match result {
                    Ok(()) => {
                        self.status = t!("status.ready").to_string();
                    }
                    Err(err) => {
                        self.status =
                            t!("status.operation_failed", error = err.as_str()).to_string();
                    }
                }
                Task::none()
            }
            Message::MountRequested(device) => {
                self.status = t!("status.mounting").to_string();
                self.busy = true;
                self.active_task_error = None;
                Task::perform(
                    crate::utils::mounts::mount_device(device),
                    Message::MountFinished,
                )
            }
            Message::UnmountRequested(device) => {
                if crate::utils::mounts::is_system_device(&device) {
                    self.status = t!("status.unmount_system_blocked").to_string();
                    return Task::none();
                }
                self.status = t!("status.unmounting").to_string();
                self.busy = true;
                self.active_task_error = None;
                Task::perform(
                    crate::utils::mounts::unmount_device(device),
                    Message::UnmountFinished,
                )
            }
            Message::UnmountConfirmRequested(device) => {
                if crate::utils::mounts::is_system_device(&device) {
                    self.status = t!("status.unmount_system_blocked").to_string();
                    return Task::none();
                }
                self.unmount_confirm_open = true;
                self.unmount_confirm_device = Some(device);
                self.sidebar_swipe_open = None;
                Task::none()
            }
            Message::UnmountConfirmCancel => {
                self.unmount_confirm_open = false;
                self.unmount_confirm_device = None;
                Task::none()
            }
            Message::UnmountConfirmAccept => {
                let Some(device) = self.unmount_confirm_device.clone() else {
                    self.unmount_confirm_open = false;
                    return Task::none();
                };
                self.unmount_confirm_open = false;
                self.unmount_confirm_device = None;
                return self.update(Message::UnmountRequested(device));
            }
            Message::MountFinished(result) => {
                self.busy = false;
                crate::utils::mounts::invalidate_sidebar_cache();
                match result {
                    Ok(path) => {
                        if path.as_os_str().is_empty() {
                            self.status = t!("status.mount_done").to_string();
                            return Task::none();
                        }
                        self.status =
                            t!("status.mount_done.path", path = path.display().to_string())
                                .to_string();
                        if path.is_dir() {
                            return self.navigate_to(path, true);
                        }
                    }
                    Err(err) => {
                        self.status = t!("status.operation_failed", error = err).to_string();
                        self.active_task_error = Some(err);
                    }
                }
                Task::none()
            }
            Message::UnmountFinished(result) => {
                self.busy = false;
                crate::utils::mounts::invalidate_sidebar_cache();
                match result {
                    Ok(()) => {
                        self.status = t!("status.unmount_done").to_string();
                    }
                    Err(err) => {
                        self.status = t!("status.operation_failed", error = err).to_string();
                        self.active_task_error = Some(err);
                    }
                }
                Task::none()
            }
            Message::SidebarSwipeStart(device, path) => {
                self.sidebar_swipe_target = Some(SidebarSwipeTarget {
                    device,
                    path,
                    start_x: self.last_cursor_pos.map(|p| p.x),
                    dragged: false,
                });
                Task::none()
            }

            Message::PropertiesRequested(entry) => {
                let path = entry.path.clone();
                self.properties_target = Some(path.clone());
                self.properties_open = true;

                let mut lines = Vec::new();
                lines.push(format!(
                    "{}: {}",
                    t!("files.column.name"),
                    entry.display_name
                ));
                lines.push(format!(
                    "{}: {}",
                    t!("appbar.location_edit"),
                    path.display()
                ));
                lines.push(format!("{}: {}", t!("files.column.type"), entry.kind));

                let size = entry
                    .size
                    .map(utils::format_size)
                    .unwrap_or_else(|| "—".into());
                lines.push(format!("{}: {}", t!("files.column.size"), size));

                let modified = utils::format_time(entry.modified);
                lines.push(format!("{}: {}", t!("files.column.modified"), modified));

                self.properties_content = Some(lines.join("\n"));
                Task::none()
            }
            Message::PropertiesClose => {
                self.properties_target = None;
                self.properties_open = false;
                self.properties_content = None;
                Task::none()
            }

            Message::FolderCreated(result) => {
                self.busy = false;
                self.active_task_finished = true;
                match result {
                    Ok(path) => {
                        self.status =
                            t!("status.created", path = path.display().to_string()).to_string();
                        self.active_task_error = None;
                        let cwd = self.selected_path.clone();
                        let refresh =
                            Task::perform(load_directory(cwd), |(p, e)| Message::DirLoaded(p, e));
                        return Task::batch(vec![refresh, self.finish_task_and_next()]);
                    }
                    Err(err) => {
                        self.status = t!("status.create_failed", error = err).to_string();
                        self.active_task_error = Some(err);
                    }
                }
                self.finish_task_and_next()
            }

            Message::Renamed(result) => {
                self.busy = false;
                self.active_task_finished = true;
                match result {
                    Ok(path) => {
                        self.status =
                            t!("status.renamed", path = path.display().to_string()).to_string();
                        self.active_task_error = None;
                        let cwd = self.selected_path.clone();
                        let refresh =
                            Task::perform(load_directory(cwd), |(p, e)| Message::DirLoaded(p, e));
                        return Task::batch(vec![refresh, self.finish_task_and_next()]);
                    }
                    Err(err) => {
                        self.status = t!("status.rename_failed", error = err).to_string();
                        self.active_task_error = Some(err);
                    }
                }
                self.finish_task_and_next()
            }
            Message::NavigateBack => {
                if let Some(prev) = self.back_stack.pop() {
                    self.forward_stack.push(self.selected_path.clone());
                    return self.navigate_to(prev, false);
                }
                Task::none()
            }
            Message::NavigateForward => {
                if let Some(next) = self.forward_stack.pop() {
                    self.back_stack.push(self.selected_path.clone());
                    return self.navigate_to(next, false);
                }
                Task::none()
            }
            Message::NavigateUp => {
                if let Some(parent) = self.selected_path.parent().map(|p| p.to_path_buf()) {
                    return self.navigate_to(parent, true);
                }
                Task::none()
            }
            Message::NavigateHome => {
                let home = std::env::var("HOME").unwrap_or_else(|_| "/".to_string());
                return self.navigate_to(PathBuf::from(home), true);
            }
            Message::NavigateTo(path) => self.navigate_to(path, true),
            Message::PathChanged(value) => {
                self.path = value;
                Task::none()
            }
            Message::PathSubmitted => {
                let p = PathBuf::from(self.path.trim());
                if p.is_dir() {
                    self.location_editing = false;
                    return self.navigate_to(p, true);
                }
                self.status = t!("status.path_not_directory").to_string();
                Task::none()
            }
            Message::FilterChanged(value) => {
                self.filter = value;
                self.entries = apply_filter(self.all_entries.clone(), &self.filter);

                self.rendered_rows = build_rows_from_fs(&self.entries);

                if self.view_mode == ViewMode::FileSystemTree {
                    let roots = self.rendered_rows.clone();
                    self.tree_render_rows = build_tree_render_rows(
                        &roots,
                        &self.tree_expanded,
                        &self.fs_children_cache,
                    );
                }

                Task::none()
            }
            Message::RowClicked(row) => {
                let path = row.path.clone();
                self.selected_entry = Some(path.clone());

                let now = Instant::now();
                let gap = Duration::from_millis(DOUBLE_CLICK_GAP_MS);
                if let Some((last_path, last_time)) = self.last_click.take() {
                    if last_path == path && now.duration_since(last_time) <= gap {
                        return self.open_row(row);
                    }
                }

                self.last_click = Some((path, now));
                Task::none()
            }
            Message::DirLoaded(path, entries) => {
                if path == self.selected_path {
                    self.all_entries = entries;
                    self.entries = apply_filter(self.all_entries.clone(), &self.filter);
                    self.status = format!("{} 项", self.entries.len());

                    self.rendered_rows = build_rows_from_fs(&self.entries);

                    if matches!(
                        self.view_mode,
                        ViewMode::FileSystemTree | ViewMode::ArchiveTree
                    ) {
                        self.tree_expanded.clear();
                        self.fs_children_cache.clear();
                        self.fs_tree_loading.clear();
                        self.archive_children_index.clear();

                        let roots = self.rendered_rows.clone();
                        self.tree_render_rows = build_tree_render_rows(
                            &roots,
                            &self.tree_expanded,
                            &self.fs_children_cache,
                        );
                    } else {
                        self.tree_render_rows.clear();
                    }
                }

                Task::none()
            }
            Message::ArchiveLoaded(result) => {
                self.busy = false;
                self.active_task_finished = true;
                let (path, entries) = match result {
                    Ok(v) => v,
                    Err(err) => {
                        self.status = t!("status.archive_read_failed", error = err).to_string();
                        self.active_task_error = Some(err);
                        self.active_task_cancel = None;
                        return self.finish_task_and_next();
                    }
                };

                self.active_archive = Some(path.clone());
                self.archive_entries = entries.clone();
                self.status =
                    t!("status.archive_entries", count = self.archive_entries.len()).to_string();

                let archive_name = path
                    .file_name()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_else(|| "archive".to_string());

                self.archive_rendered_rows = self
                    .archive_entries
                    .iter()
                    .map(|e| {
                        let inner = if e.path.starts_with('/') {
                            e.path.clone()
                        } else {
                            format!("/{}", e.path)
                        };
                        let virtual_path = format!("{archive_name}::{inner}");

                        EntryRow {
                            display_name: e
                                .path
                                .split('/')
                                .filter(|s| !s.is_empty())
                                .last()
                                .unwrap_or(&e.path)
                                .to_string(),
                            path: PathBuf::from(virtual_path),
                            is_dir: e.is_dir,
                            depth: 0,
                            kind: if e.is_dir {
                                t!("fs.kind.folder").to_string()
                            } else {
                                t!("fs.kind.file").to_string()
                            },
                            size: e.size,
                            modified: e.modified,
                            source: EntrySource::Archive {
                                archive_path: path.clone(),
                            },
                        }
                    })
                    .collect();

                if self.pending_extract {
                    self.pending_extract = false;
                    let dest = path
                        .parent()
                        .unwrap_or_else(|| std::path::Path::new("."))
                        .join(utils::archive_stem(&path));
                    return Task::batch(vec![
                        self.finish_task_and_next(),
                        self.enqueue_task(UiTask::Extract {
                            archive: path,
                            dest,
                        }),
                    ]);
                }
                self.active_task_cancel = None;
                return self.finish_task_and_next();
            }
            Message::ExtractToRequested(target) => {
                self.title_menu_open = false;
                self.selected_entry = Some(target.clone());
                self.extract_to_target = Some(target.clone());

                let default_dest = target
                    .parent()
                    .unwrap_or_else(|| std::path::Path::new("."))
                    .join(utils::archive_stem(&target));
                self.extract_to_path = default_dest.display().to_string();

                self.drawer_open = true;
                self.drawer_panel = DrawerPanel::Extract;
                self.status = t!("status.choose_extract_dir").to_string();
                self.active_task_error = None;
                Task::none()
            }
            Message::ExtractToChanged(value) => {
                self.extract_to_path = value;
                Task::none()
            }
            Message::ExtractToCancel => {
                self.extract_to_target = None;
                self.extract_to_path.clear();

                if self.drawer_open && self.drawer_panel == DrawerPanel::Extract {
                    self.drawer_panel = DrawerPanel::Task;
                }

                Task::none()
            }
            Message::ExtractToConfirm => {
                let Some(archive) = self.extract_to_target.clone() else {
                    return Task::none();
                };

                if ArchiveFormat::from_path(&archive).is_none() {
                    self.status = t!("status.select_archive").to_string();
                    return Task::none();
                }

                let dest = PathBuf::from(self.extract_to_path.trim());
                if dest.as_os_str().is_empty() {
                    self.status = t!("status.enter_extract_dir").to_string();
                    return Task::none();
                }

                self.drawer_open = true;
                self.drawer_panel = DrawerPanel::Task;

                self.extract_to_target = None;
                self.extract_to_path.clear();

                return self.enqueue_task(UiTask::Extract { archive, dest });
            }
            Message::CompressSelected => {
                self.title_menu_open = false;

                let mut inputs: Vec<PathBuf> = self
                    .entries
                    .iter()
                    .filter(|e| e.checked)
                    .map(|e| e.path.clone())
                    .collect();

                if inputs.is_empty() {
                    if let Some(sel) = self.selected_entry.clone() {
                        inputs.push(sel);
                    }
                }

                if inputs.is_empty() {
                    self.status = t!("status.select_inputs").to_string();
                    return Task::none();
                }

                let dest_dir = self.selected_path.clone();
                let first_name = inputs
                    .first()
                    .and_then(|p| p.file_name())
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_else(|| "archive".to_string());

                let dest_archive = match utils::suggest_archive_path(&first_name, &dest_dir, "zip")
                {
                    Ok(p) => p,
                    Err(err) => {
                        self.status = t!("status.archive_path_failed", error = err).to_string();
                        return Task::none();
                    }
                };

                return self.enqueue_task(UiTask::Compress {
                    inputs,
                    dest: dest_archive,
                });
            }
            Message::CompressFinished(result) => {
                self.busy = false;
                self.active_task_finished = true;
                self.active_task_cancel = None;
                match result {
                    Ok(path) => {
                        self.status = t!("status.compress_done", path = path.display().to_string())
                            .to_string()
                    }
                    Err(err) => {
                        self.status = t!("status.compress_failed", error = err).to_string();
                        self.active_task_error = Some(err);
                    }
                }
                return self.finish_task_and_next();
            }
            Message::ExtractFinished(result) => {
                self.busy = false;
                self.active_task_finished = true;
                self.active_task_cancel = None;
                match result {
                    Ok(dest) => {
                        self.status =
                            t!("status.extract_done", path = dest.display().to_string()).to_string()
                    }
                    Err(err) => {
                        self.status = t!("status.operation_failed", error = err).to_string();
                        self.active_task_error = Some(err);
                    }
                }
                return self.finish_task_and_next();
            }
            Message::ContextActionFor(action, row) => {
                let target = row.path.clone();
                self.selected_entry = Some(target.clone());

                match action {
                    ContextAction::Open => {
                        self.status = t!("status.opening").to_string();
                        if matches!(row.source, EntrySource::FileSystem)
                            && !row.is_dir
                            && ArchiveFormat::from_path(&target).is_none()
                        {
                            return Task::perform(open_path(target), Message::OpenFinished);
                        }
                        return self.open_row(row);
                    }
                    ContextAction::SmartExtract => {
                        self.status = t!("status.smart_extract_preparing").to_string();
                        self.drawer_open = true;
                        self.drawer_panel = DrawerPanel::Task;

                        if matches!(row.source, EntrySource::FileSystem)
                            && ArchiveFormat::from_path(&target).is_some()
                        {
                            self.pending_extract = true;
                            return self.enqueue_task(UiTask::ListArchive(target));
                        }

                        self.status = t!("status.select_archive").to_string();
                        Task::none()
                    }
                    ContextAction::ExtractTo => {
                        self.status = t!("status.extract_to").to_string();
                        self.drawer_open = true;
                        self.drawer_panel = DrawerPanel::Extract;

                        if matches!(row.source, EntrySource::FileSystem)
                            && ArchiveFormat::from_path(&target).is_some()
                        {
                            return self.update(Message::ExtractToRequested(target));
                        }

                        self.status = t!("status.select_archive").to_string();
                        Task::none()
                    }
                    ContextAction::CompressZip => {
                        if !matches!(row.source, EntrySource::FileSystem) {
                            self.status =
                                t!("status.operation_failed", error = "not supported").to_string();
                            return Task::none();
                        }
                        self.status = t!("status.creating_archive").to_string();
                        self.drawer_open = true;
                        self.drawer_panel = DrawerPanel::Task;

                        return self.update(Message::CompressSelected);
                    }
                    ContextAction::Rename => {
                        if !matches!(row.source, EntrySource::FileSystem) {
                            self.status =
                                t!("status.operation_failed", error = "not supported").to_string();
                            return Task::none();
                        }
                        self.status = t!("status.renaming").to_string();
                        self.drawer_open = true;
                        self.drawer_panel = DrawerPanel::Rename;

                        return self.update(Message::RenameRequested(target));
                    }
                    ContextAction::Delete => {
                        if !matches!(row.source, EntrySource::FileSystem) {
                            self.status =
                                t!("status.operation_failed", error = "not supported").to_string();
                            return Task::none();
                        }
                        self.status = t!("status.deleting").to_string();
                        self.drawer_open = true;
                        self.drawer_panel = DrawerPanel::DeleteConfirm;

                        return self.update(Message::DeleteRequested(target));
                    }
                    ContextAction::Properties => {
                        return self.update(Message::PropertiesRequested(row));
                    }
                }
            }
            Message::Tick => {
                if self.busy {
                    self.spinner_index = (self.spinner_index + 1) % utils::SPINNER.len();
                }
                Task::none()
            }
            Message::Event(event) => {
                if self.drawer_resizing {
                    match &event {
                        iced::Event::Mouse(iced::mouse::Event::CursorMoved { position }) => {
                            let x = position.x;
                            if let Some(last) = self.drawer_resize_last_cursor_x {
                                let dx = x - last;
                                let delta = -dx;

                                let window_w = 1100.0_f32;
                                let max_w = window_w * DRAWER_MAX_WIDTH_RATIO;

                                self.drawer_width_px = (self.drawer_width_px + delta)
                                    .clamp(DRAWER_MIN_WIDTH_PX, max_w);
                            }
                            self.drawer_resize_last_cursor_x = Some(x);
                        }
                        iced::Event::Mouse(iced::mouse::Event::ButtonReleased(
                            iced::mouse::Button::Left,
                        )) => {
                            self.drawer_resizing = false;
                            self.drawer_resize_last_cursor_x = None;
                        }
                        _ => {}
                    }
                }

                match &event {
                    iced::Event::Mouse(iced::mouse::Event::CursorMoved { position }) => {
                        self.last_cursor_pos = Some(*position);

                        if let Some(target) = self.sidebar_swipe_target.as_mut() {
                            let start_x = target.start_x.get_or_insert(position.x);
                            let dx = position.x - *start_x;
                            if dx.abs() > 6.0 {
                                target.dragged = true;
                            }
                            if dx <= -24.0 {
                                self.sidebar_swipe_open = Some(target.device.clone());
                            } else if dx >= 12.0 {
                                if self.sidebar_swipe_open.as_deref()
                                    == Some(target.device.as_str())
                                {
                                    self.sidebar_swipe_open = None;
                                }
                            }
                        }
                    }
                    iced::Event::Mouse(iced::mouse::Event::ButtonReleased(
                        iced::mouse::Button::Left,
                    )) => {
                        if let Some(target) = self.sidebar_swipe_target.take() {
                            if !target.dragged {
                                return self.navigate_to(target.path, true);
                            }
                        }
                    }
                    _ => {}
                }

                match handle_global_event(&event) {
                    GlobalEvent::None => {}
                    GlobalEvent::FileDropped(path) => {
                        if ArchiveFormat::from_path(&path).is_some() {
                            self.pending_extract = true;
                            return self.enqueue_task(UiTask::ListArchive(path));
                        }
                    }
                }
                Task::none()
            }
        }
    }

    fn open_entry(&mut self, path: PathBuf) -> Task<Message> {
        if let Some(entry) = self.entries.iter().find(|entry| entry.path == path) {
            if entry.is_dir {
                return self.navigate_to(path, true);
            }
            if entry.is_archive {
                return self.enqueue_task(UiTask::ListArchive(path));
            }
            self.status = t!("status.open_unsupported").to_string();
            return Task::none();
        }

        // 回退：树视图/过滤列表可能不在 entries 中，直接访问文件系统判断。
        if let Ok(md) = std::fs::metadata(&path) {
            if md.is_dir() {
                return self.navigate_to(path, true);
            }
            if ArchiveFormat::from_path(&path).is_some() {
                return self.enqueue_task(UiTask::ListArchive(path));
            }
            self.status = t!("status.open_unsupported").to_string();
        }
        Task::none()
    }

    /// 统一的“打开/进入”分派：根据 `EntryRow.source` 处理双击行为。
    fn open_row(&mut self, row: EntryRow) -> Task<Message> {
        match row.source {
            EntrySource::FileSystem => self.open_entry(row.path),
            EntrySource::Archive { archive_path } => {
                let Some((archive_name, inner)) = parse_virtual_path(&row.path) else {
                    self.status = t!("status.virtual_path_parse_failed").to_string();
                    return Task::none();
                };

                if row.is_dir {
                    let dir_key =
                        PathBuf::from(format!("{archive_name}::{inner}/").replace("//", "/"));
                    let roots = self
                        .archive_children_index
                        .get(&dir_key)
                        .cloned()
                        .unwrap_or_default();

                    self.active_archive = Some(archive_path.clone());
                    self.archive_rendered_rows = roots.clone();

                    if self.view_mode == ViewMode::ArchiveTree {
                        self.tree_expanded.clear();
                        self.tree_render_rows = build_tree_render_rows(
                            &roots,
                            &self.tree_expanded,
                            &self.archive_children_index,
                        );
                    }

                    self.status = t!(
                        "status.enter_archive_dir",
                        name = archive_name,
                        inner = inner
                    )
                    .to_string();
                    return Task::none();
                }

                self.status =
                    t!("status.preview_pending", name = archive_name, inner = inner).to_string();
                Task::none()
            }
        }
    }

    fn navigate_to(&mut self, path: PathBuf, record_history: bool) -> Task<Message> {
        if record_history && path != self.selected_path {
            self.back_stack.push(self.selected_path.clone());
            self.forward_stack.clear();
        }

        self.sidebar_swipe_target = None;
        self.sidebar_swipe_open = None;

        self.page = Page::Browser;
        self.selected_path = path.clone();
        self.path = path.display().to_string();
        self.view_mode = match self.view_mode {
            ViewMode::FileSystem | ViewMode::FileSystemTree => self.view_mode,
            ViewMode::Archive | ViewMode::ArchiveTree => ViewMode::FileSystem,
        };
        self.active_archive = None;
        self.archive_entries.clear();

        Task::perform(load_directory(path), |(path, entries)| {
            Message::DirLoaded(path, entries)
        })
    }
}
