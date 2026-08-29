//! Application state and commands for the desktop archive workspace.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{
    Arc,
    mpsc::{self, TryRecvError},
};
use std::time::Duration;

use gpui::{
    AppContext as _, Context, Entity, ExternalPaths, IntoElement, Modifiers, MouseButton, Render,
    Window, WindowControlArea, div, prelude::*,
};
use gpui_component::button::{Button, ButtonVariants};
use gpui_component::dialog::DialogButtonProps;
use gpui_component::input::{Input, InputState};
use gpui_component::menu::{DropdownMenu, PopupMenu, PopupMenuItem};
use gpui_component::{
    ActiveTheme, Disableable, InteractiveElementExt, Root, Sizable, WindowExt as _, h_flex, v_flex,
};
use lucide_icons::Icon as LucideIcon;
use pulp::{
    ArchiveFormatId, CancellationToken, CreateOptions, OperationKind, OperationReport, Password,
    PasswordProvider, PasswordReason, ProgressEvent,
};

use crate::archive::{self, FormatOption, LoadedArchive};
use crate::i18n::{I18n, Locale, MessageKey};
use crate::password::{PasswordBroker, PasswordPrompt, PasswordResponder};
use crate::settings::{LanguagePreference, SettingsFile, save_settings_atomic};
use crate::views;

/// Top-level state for the archive browser and its operations.
pub struct Workspace {
    i18n: I18n,
    settings: SettingsFile,
    settings_path: Option<PathBuf>,
    archive: Option<LoadedArchive>,
    current_prefix: String,
    expanded_folders: HashSet<String>,
    selected: HashSet<String>,
    selection_anchor: Option<String>,
    selection_dragging: bool,
    formats: Vec<FormatOption>,
    status: String,
    operation: Option<OperationProgress>,
    password_dialog: Option<PasswordResponder>,
    settings_open: bool,
}

struct OperationProgress {
    kind: OperationKind,
    processed: u64,
    total: Option<u64>,
    current: Option<String>,
    cancellation: CancellationToken,
}

struct TitleBarState {
    should_move: bool,
}

impl Render for TitleBarState {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}

enum JobOutput {
    Archive(LoadedArchive),
    Report(OperationReport),
}

#[derive(Clone, Copy)]
enum MenuAction {
    OpenArchive,
    NewArchive,
    ExtractTo,
    TestArchive,
    CloseArchive,
    CancelOperation,
    SelectAll,
    OpenSettings,
    RestoreDefaults,
    ToggleFolderPane,
    ToggleArchivePath,
    AboutPulp,
}

impl Workspace {
    /// Creates a workspace from the validated persisted preferences.
    #[must_use]
    pub fn new(settings: SettingsFile, i18n: I18n, settings_path: Option<PathBuf>) -> Self {
        let status = i18n.text(MessageKey::Ready);
        Self {
            i18n,
            settings,
            settings_path,
            archive: None,
            current_prefix: String::new(),
            expanded_folders: HashSet::new(),
            selected: HashSet::new(),
            selection_anchor: None,
            selection_dragging: false,
            formats: Vec::new(),
            status,
            operation: None,
            password_dialog: None,
            settings_open: false,
        }
    }

    /// Returns localized text for a view.
    #[must_use]
    pub fn text(&self, key: MessageKey) -> String {
        self.i18n.text(key)
    }

    /// Returns the active localization catalog.
    #[must_use]
    pub const fn i18n(&self) -> &I18n {
        &self.i18n
    }

    /// Returns the current preferences.
    #[must_use]
    pub const fn settings(&self) -> &SettingsFile {
        &self.settings
    }

    /// Returns the archive currently displayed by the browser.
    #[must_use]
    pub const fn archive(&self) -> Option<&LoadedArchive> {
        self.archive.as_ref()
    }

    /// Returns provider formats discovered by the last metadata query.
    #[must_use]
    pub fn formats(&self) -> &[FormatOption] {
        &self.formats
    }

    /// Loads writable provider formats without blocking the UI thread.
    pub fn load_formats(&mut self, cx: &mut Context<Self>) {
        if !self.formats.is_empty() {
            return;
        }
        let entity = cx.entity();
        cx.spawn(async move |_, cx| {
            let result = cx
                .background_spawn(async { archive::creation_formats() })
                .await;
            let _ = entity.update(cx, |workspace, cx| match result {
                Ok(formats) => {
                    workspace.formats = formats;
                    cx.notify();
                }
                Err(error) => workspace.set_error(error.to_string(), cx),
            });
        })
        .detach();
    }

    /// Returns the path prefix displayed in the archive browser.
    #[must_use]
    pub fn current_prefix(&self) -> &str {
        &self.current_prefix
    }

    /// Returns expanded archive folders.
    #[must_use]
    pub const fn expanded_folders(&self) -> &HashSet<String> {
        &self.expanded_folders
    }

    /// Returns whether the folder pane is visible.
    #[must_use]
    pub const fn show_folder_pane(&self) -> bool {
        self.settings.ui.show_folder_pane
    }

    /// Returns whether the host archive path is shown in the breadcrumb.
    #[must_use]
    pub const fn show_archive_path(&self) -> bool {
        self.settings.ui.show_archive_path
    }

    /// Returns the selected archive-relative name.
    #[must_use]
    pub fn is_selected(&self, name: &str) -> bool {
        self.selected.contains(name)
    }

    /// Returns the configured list presentation.
    #[must_use]
    pub const fn list_mode(&self) -> crate::settings::ListMode {
        self.settings.ui.list_mode
    }

    /// Creates a button that dispatches one workspace command.
    pub fn action_button(
        &self,
        id: &'static str,
        button_icon: LucideIcon,
        label: MessageKey,
        enabled: bool,
        cx: &mut Context<Self>,
    ) -> Button {
        let Some(action) = menu_action_for_label(label) else {
            return Button::new(id).ghost().disabled(true).child(
                h_flex()
                    .gap_2()
                    .child(icon(button_icon, 15.))
                    .child(self.text(label)),
            );
        };
        let entity = cx.entity();
        Button::new(id)
            .ghost()
            .disabled(!enabled)
            .child(
                h_flex()
                    .gap_2()
                    .child(icon(button_icon, 15.))
                    .child(self.text(label)),
            )
            .on_click(move |_, window, app| {
                entity.update(app, |workspace, cx| {
                    workspace.run_menu(action, window, cx);
                });
            })
    }

    /// Opens the native file picker for an archive.
    pub fn open_archive(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.is_busy() {
            return;
        }
        let receiver = cx.prompt_for_paths(gpui::PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some(self.text(MessageKey::OpenArchive).into()),
        });
        let entity = cx.entity();
        cx.spawn_in(window, async move |_, cx| match receiver.await {
            Ok(Ok(Some(paths))) => {
                if let Some(path) = paths.into_iter().find(|path| path.is_file()) {
                    let _ = cx.update(|window, app| {
                        entity.update(app, |workspace, cx| {
                            workspace.start_load(path, window, cx);
                        });
                    });
                } else {
                    let _ = entity.update(cx, |workspace, cx| {
                        workspace.status = workspace.text(MessageKey::NoArchiveOpen);
                        cx.notify();
                    });
                }
            }
            Ok(Ok(None)) => {}
            Ok(Err(error)) => {
                let _ = entity.update(cx, |workspace, cx| {
                    workspace.set_error(error.to_string(), cx);
                });
            }
            Err(error) => {
                let _ = entity.update(cx, |workspace, cx| {
                    workspace.set_error(error.to_string(), cx);
                });
            }
        })
        .detach();
    }

    /// Opens the native picker for source paths, followed by an output picker.
    pub fn new_archive(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.is_busy() {
            return;
        }
        let receiver = cx.prompt_for_paths(gpui::PathPromptOptions {
            files: true,
            directories: true,
            multiple: true,
            prompt: Some(self.text(MessageKey::SourcePaths).into()),
        });
        let entity = cx.entity();
        let default_name = self.default_archive_name();
        cx.spawn_in(window, async move |_, cx| {
            let sources = match receiver.await {
                Ok(Ok(Some(paths))) if !paths.is_empty() => paths,
                Ok(Ok(_)) => return,
                Ok(Err(error)) => {
                    let _ = entity.update(cx, |workspace, cx| {
                        workspace.set_error(error.to_string(), cx);
                    });
                    return;
                }
                Err(error) => {
                    let _ = entity.update(cx, |workspace, cx| {
                        workspace.set_error(error.to_string(), cx);
                    });
                    return;
                }
            };
            let directory = sources
                .first()
                .and_then(|path| path.parent())
                .map(Path::to_owned)
                .unwrap_or_else(|| PathBuf::from("."));
            let output = match cx
                .update(|_, app| app.prompt_for_new_path(&directory, Some(&default_name)))
            {
                Ok(receiver) => match receiver.await {
                    Ok(Ok(Some(path))) => path,
                    Ok(Ok(None)) => return,
                    Ok(Err(error)) => {
                        let _ = entity.update(cx, |workspace, cx| {
                            workspace.set_error(error.to_string(), cx);
                        });
                        return;
                    }
                    Err(error) => {
                        let _ = entity.update(cx, |workspace, cx| {
                            workspace.set_error(error.to_string(), cx);
                        });
                        return;
                    }
                },
                Err(error) => {
                    let _ = entity.update(cx, |workspace, cx| {
                        workspace.set_error(error.to_string(), cx);
                    });
                    return;
                }
            };
            let _ = cx.update(|window, app| {
                entity.update(app, |workspace, cx| {
                    workspace.start_creation(sources, output, window, cx);
                });
            });
        })
        .detach();
    }

    /// Opens the native directory picker and extracts the current archive.
    pub fn extract_to(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.is_busy() {
            return;
        }
        let Some(archive) = self.archive.clone() else {
            self.status = self.text(MessageKey::NoArchiveOpen);
            cx.notify();
            return;
        };
        let receiver = cx.prompt_for_paths(gpui::PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some(self.text(MessageKey::SelectDestination).into()),
        });
        let selected = self.selected.clone();
        let entity = cx.entity();
        cx.spawn_in(window, async move |_, cx| match receiver.await {
            Ok(Ok(Some(paths))) => {
                if let Some(destination) = paths.into_iter().find(|path| path.is_dir()) {
                    let _ = cx.update(|window, app| {
                        entity.update(app, |workspace, cx| {
                            workspace.start_extraction(archive, destination, selected, window, cx);
                        });
                    });
                }
            }
            Ok(Ok(None)) => {}
            Ok(Err(error)) => {
                let _ = entity.update(cx, |workspace, cx| {
                    workspace.set_error(error.to_string(), cx);
                });
            }
            Err(error) => {
                let _ = entity.update(cx, |workspace, cx| {
                    workspace.set_error(error.to_string(), cx);
                });
            }
        })
        .detach();
    }

    /// Tests every compressed stream in the current archive.
    pub fn test_archive(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.is_busy() {
            return;
        }
        let Some(archive) = self.archive.clone() else {
            self.status = self.text(MessageKey::NoArchiveOpen);
            cx.notify();
            return;
        };
        let settings = self.settings.clone();
        self.start_job(
            OperationKind::Test,
            window,
            cx,
            move |cancellation, progress, password| {
                archive::test_archive(
                    &archive,
                    archive::ArchiveJob::new(settings, progress, cancellation, password),
                )
                .map(JobOutput::Report)
                .map_err(|error| error.to_string())
            },
        );
    }

    /// Closes the current archive without closing the application window.
    pub fn close_archive(&mut self, cx: &mut Context<Self>) {
        if self.is_busy() {
            return;
        }
        self.archive = None;
        self.current_prefix.clear();
        self.expanded_folders.clear();
        self.selected.clear();
        self.selection_anchor = None;
        self.selection_dragging = false;
        self.status = self.text(MessageKey::Ready);
        cx.notify();
    }

    /// Cancels the current worker operation.
    pub fn cancel_operation(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(operation) = &self.operation {
            operation.cancellation.cancel();
            self.dismiss_password_prompt(window, cx);
            self.status = self.text(MessageKey::Processing);
            cx.notify();
        }
    }

    fn show_password_prompt(
        &mut self,
        prompt: PasswordPrompt,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.dismiss_password_prompt(window, cx);
        let retry = prompt.request.reason == PasswordReason::Retry;
        let title = self.text(MessageKey::PasswordRequired);
        let message = if retry {
            self.text(MessageKey::WrongPassword)
        } else {
            self.text(MessageKey::PasswordPrompt)
        };
        let placeholder = self.text(MessageKey::PasswordPrompt);
        let input = cx.new(|cx| {
            InputState::new(window, cx)
                .masked(true)
                .placeholder(placeholder)
        });
        let response = prompt.into_responder();
        self.password_dialog = Some(response.clone());
        window.defer(cx, move |window, cx| {
            window.open_dialog(cx, move |dialog, _, _| {
                let ok_response = response.clone();
                let cancel_response = response.clone();
                dialog
                    .title(div().child(title.clone()))
                    .button_props(
                        DialogButtonProps::default()
                            .ok_text("OK")
                            .cancel_text("Cancel"),
                    )
                    .confirm()
                    .child(
                        v_flex()
                            .gap_2()
                            .child(div().text_sm().child(message.clone()))
                            .child(Input::new(&input).mask_toggle()),
                    )
                    .on_ok({
                        let input = input.clone();
                        move |_, _, app| {
                            let value = input.read(app).value().to_string();
                            if value.is_empty() {
                                return false;
                            }
                            ok_response.send(Some(Password::new(value)));
                            true
                        }
                    })
                    .on_cancel(move |_, _, _| {
                        cancel_response.send(None);
                        true
                    })
            });
        });
    }

    fn dismiss_password_prompt(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(response) = self.password_dialog.take() {
            response.send(None);
            window.defer(cx, |window, cx| {
                if window.has_active_dialog(cx) {
                    window.close_dialog(cx);
                }
            });
        }
    }

    /// Selects all immediate entries in the displayed archive directory.
    pub fn select_all(&mut self, cx: &mut Context<Self>) {
        let Some(archive) = &self.archive else {
            return;
        };
        let prefix = self.current_prefix.clone();
        let prefix_with_separator = (!prefix.is_empty()).then(|| format!("{prefix}/"));
        let mut names = HashSet::new();
        for entry in &archive.entries {
            let Some(remainder) = prefix_with_separator
                .as_deref()
                .map_or(Some(entry.name.as_str()), |prefix| {
                    entry.name.as_str().strip_prefix(prefix)
                })
            else {
                continue;
            };
            if remainder.is_empty() {
                continue;
            }
            let component = remainder.split('/').next().unwrap_or_default();
            names.insert(if prefix.is_empty() {
                component.to_owned()
            } else {
                format!("{prefix}/{component}")
            });
        }
        if !names.is_empty() && names.iter().all(|name| self.selected.contains(name)) {
            self.selected.clear();
        } else {
            self.selected.extend(names);
        }
        self.selection_anchor = None;
        self.selection_dragging = false;
        cx.notify();
    }

    /// Changes the current archive directory.
    pub fn navigate_to(&mut self, prefix: String, cx: &mut Context<Self>) {
        self.current_prefix = prefix;
        self.selected.clear();
        self.selection_anchor = None;
        self.selection_dragging = false;
        cx.notify();
    }

    /// Toggles a folder tree node.
    pub fn toggle_folder(&mut self, prefix: String, cx: &mut Context<Self>) {
        if prefix.is_empty() {
            return;
        }
        if !self.expanded_folders.insert(prefix.clone()) {
            self.expanded_folders.remove(&prefix);
        }
        cx.notify();
    }

    /// Applies standard file-manager selection semantics to one visible row.
    pub fn begin_selection(
        &mut self,
        name: String,
        visible_names: &[String],
        modifiers: Modifiers,
        cx: &mut Context<Self>,
    ) {
        let range = modifiers.shift;
        let additive = modifiers.control || modifiers.platform;
        if range {
            let anchor = self
                .selection_anchor
                .clone()
                .unwrap_or_else(|| name.clone());
            self.selected.clear();
            self.selected
                .extend(names_between(visible_names, &anchor, &name));
            self.selection_anchor = Some(anchor);
        } else if additive {
            if !self.selected.insert(name.clone()) {
                self.selected.remove(&name);
            }
            self.selection_anchor = Some(name);
        } else {
            self.selected.clear();
            self.selected.insert(name.clone());
            self.selection_anchor = Some(name);
        }
        self.selection_dragging = true;
        cx.notify();
    }

    /// Extends the active mouse selection to the row under the pointer.
    pub fn extend_selection(
        &mut self,
        name: &str,
        visible_names: &[String],
        cx: &mut Context<Self>,
    ) {
        if !self.selection_dragging {
            return;
        }
        let Some(anchor) = self.selection_anchor.as_deref() else {
            return;
        };
        self.selected = names_between(visible_names, anchor, name);
        cx.notify();
    }

    /// Ends a pointer selection gesture.
    pub fn end_selection(&mut self, _cx: &mut Context<Self>) {
        self.selection_dragging = false;
    }

    /// Updates and immediately persists a preference.
    pub fn update_settings<F>(&mut self, edit: F, cx: &mut Context<Self>)
    where
        F: FnOnce(&mut SettingsFile),
    {
        let previous = self.settings.clone();
        edit(&mut self.settings);
        if let Err(error) = self.settings.validate() {
            self.settings = previous;
            self.set_error(error.to_string(), cx);
            return;
        }
        if let Some(path) = &self.settings_path
            && let Err(error) = save_settings_atomic(path, &self.settings)
        {
            self.settings = previous;
            self.set_error(error.to_string(), cx);
            return;
        }
        self.apply_preferences(cx);
        self.status = self.text(MessageKey::SettingsSaved);
        cx.notify();
    }

    /// Restores all preferences to their documented defaults.
    pub fn restore_defaults(&mut self, cx: &mut Context<Self>) {
        self.update_settings(|settings| *settings = SettingsFile::default(), cx);
    }

    /// Opens the settings page in the current window.
    pub fn open_settings(&mut self, cx: &mut Context<Self>) {
        self.settings_open = true;
        cx.notify();
    }

    /// Returns to the archive page.
    pub fn close_settings(&mut self, cx: &mut Context<Self>) {
        self.settings_open = false;
        self.status = self.text(MessageKey::Ready);
        cx.notify();
    }

    fn handle_drop(&mut self, paths: &[PathBuf], window: &mut Window, cx: &mut Context<Self>) {
        if self.is_busy() {
            return;
        }
        match paths.iter().find(|path| path.is_file()).cloned() {
            Some(path) => self.start_load(path, window, cx),
            None => {
                self.status = self.text(MessageKey::NoArchiveOpen);
                cx.notify();
            }
        }
    }

    fn start_load(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        if self.is_busy() {
            return;
        }
        let settings = self.settings.clone();
        self.start_job(
            OperationKind::List,
            window,
            cx,
            move |cancellation, progress, password| {
                archive::load_archive(
                    path,
                    archive::ArchiveJob::new(settings, progress, cancellation, password),
                )
                .map(JobOutput::Archive)
                .map_err(|error| error.to_string())
            },
        );
    }

    fn start_extraction(
        &mut self,
        archive: LoadedArchive,
        base: PathBuf,
        selected: HashSet<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let destination = if self.settings.extraction.smart {
            match archive::smart_destination(&archive, &base, pulp::CollisionPolicy::AutoRename) {
                Ok(path) => path,
                Err(error) => {
                    self.set_error(error.to_string(), cx);
                    return;
                }
            }
        } else {
            base
        };
        let settings = self.settings.clone();
        self.start_job(
            OperationKind::Extract,
            window,
            cx,
            move |cancellation, progress, password| {
                archive::extract_archive(
                    &archive,
                    destination,
                    selected,
                    archive::ArchiveJob::new(settings, progress, cancellation, password),
                )
                .map_err(|error| error.to_string())
                .map(JobOutput::Report)
            },
        );
    }

    /// Extracts to the archive's parent using the configured smart destination.
    fn quick_extract(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.is_busy() {
            return;
        }
        let Some(archive) = self.archive.clone() else {
            return;
        };
        let base = archive
            .path
            .parent()
            .filter(|path| !path.as_os_str().is_empty())
            .map(Path::to_owned)
            .unwrap_or_else(|| PathBuf::from("."));
        self.start_extraction(archive, base, self.selected.clone(), window, cx);
    }

    fn start_creation(
        &mut self,
        sources: Vec<PathBuf>,
        output: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let format = self.format_for_output(&output);
        let options = CreateOptions {
            compression_method: self.settings.archive.compression_method.clone(),
            compression_level: Some(u32::from(self.settings.archive.compression_level)),
            ..CreateOptions::default()
        };
        let settings = self.settings.clone();
        self.start_job(
            OperationKind::Create,
            window,
            cx,
            move |cancellation, progress, password| {
                archive::create_archive(
                    sources,
                    output,
                    format,
                    options,
                    archive::ArchiveJob::new(settings, progress, cancellation, password),
                )
                .map_err(|error| error.to_string())
                .map(JobOutput::Report)
            },
        );
    }

    fn format_for_output(&self, output: &Path) -> ArchiveFormatId {
        let extension = output
            .extension()
            .and_then(|extension| extension.to_str())
            .map(str::to_ascii_lowercase);
        extension
            .and_then(|extension| {
                self.formats
                    .iter()
                    .find(|format| format.extension.eq_ignore_ascii_case(&extension))
                    .map(|format| format.id.clone())
            })
            .unwrap_or_else(|| ArchiveFormatId::new(self.settings.archive.default_format.clone()))
    }

    fn default_archive_name(&self) -> String {
        let format = self
            .formats
            .iter()
            .find(|format| {
                format
                    .id
                    .as_str()
                    .eq_ignore_ascii_case(&self.settings.archive.default_format)
            })
            .or_else(|| {
                self.formats
                    .iter()
                    .find(|format| format.id.as_str() == "zip")
            });
        let extension = format
            .map(|format| format.extension.as_str())
            .filter(|extension| !extension.is_empty())
            .unwrap_or(self.settings.archive.default_format.as_str());
        format!("archive.{extension}")
    }

    fn start_job<F>(
        &mut self,
        kind: OperationKind,
        window: &mut Window,
        cx: &mut Context<Self>,
        job: F,
    ) where
        F: FnOnce(
                CancellationToken,
                mpsc::Sender<ProgressEvent>,
                Arc<dyn PasswordProvider>,
            ) -> Result<JobOutput, String>
            + Send
            + 'static,
    {
        let cancellation = CancellationToken::new();
        let worker_cancellation = cancellation.clone();
        let (password_provider, password_receiver) = PasswordBroker::channel(cancellation.clone());
        let password_provider: Arc<dyn PasswordProvider> = password_provider;
        let (progress_sender, progress_receiver) = mpsc::channel();
        let (result_sender, result_receiver) = mpsc::channel();
        self.operation = Some(OperationProgress {
            kind,
            processed: 0,
            total: None,
            current: None,
            cancellation,
        });
        self.status = self.operation_label(kind);
        cx.notify();

        cx.background_spawn(async move {
            let _ =
                result_sender.send(job(worker_cancellation, progress_sender, password_provider));
        })
        .detach();

        cx.spawn_in(window, async move |entity, cx| {
            loop {
                while let Ok(prompt) = password_receiver.try_recv() {
                    let fallback = prompt.responder();
                    if let Err(error) = entity.update_in(cx, |workspace, window, app| {
                        workspace.show_password_prompt(prompt, window, app);
                    }) {
                        fallback.send(None);
                        let message = format!("password dialog unavailable: {error}");
                        let _ = entity.update(cx, |workspace, cx| {
                            workspace.set_error(message, cx);
                        });
                    }
                }
                while let Ok(event) = progress_receiver.try_recv() {
                    let _ = entity.update(cx, |workspace, cx| workspace.apply_progress(event, cx));
                }
                match result_receiver.try_recv() {
                    Ok(result) => {
                        while let Ok(event) = progress_receiver.try_recv() {
                            let _ = entity.update(cx, |workspace, cx| {
                                workspace.apply_progress(event, cx);
                            });
                        }
                        if let Err(error) = entity.update_in(cx, |workspace, window, app| {
                            workspace.finish_job(result, window, app);
                        }) {
                            let message = format!("operation UI update failed: {error}");
                            let _ = entity.update(cx, |workspace, cx| {
                                workspace.set_error(message, cx);
                            });
                        }
                        break;
                    }
                    Err(TryRecvError::Disconnected) => {
                        if let Err(error) = entity.update_in(cx, |workspace, window, app| {
                            workspace.finish_job(
                                Err("operation worker disconnected".to_owned()),
                                window,
                                app,
                            );
                        }) {
                            let message = format!("operation UI update failed: {error}");
                            let _ = entity.update(cx, |workspace, cx| {
                                workspace.set_error(message, cx);
                            });
                        }
                        break;
                    }
                    Err(TryRecvError::Empty) => {
                        cx.background_executor()
                            .timer(Duration::from_millis(60))
                            .await;
                    }
                }
            }
        })
        .detach();
    }

    fn apply_progress(&mut self, event: ProgressEvent, cx: &mut Context<Self>) {
        let Some(operation) = &mut self.operation else {
            return;
        };
        match event {
            ProgressEvent::Started {
                operation: kind,
                total_bytes,
            } => {
                operation.kind = kind;
                operation.processed = 0;
                operation.total = total_bytes;
            }
            ProgressEvent::EntryStarted { name, .. } => {
                operation.current = Some(name.to_string());
            }
            ProgressEvent::Bytes {
                processed, total, ..
            } => {
                operation.processed = processed;
                operation.total = total.or(operation.total);
            }
            ProgressEvent::Warning(message) => {
                self.status = format!("{}: {message}", self.text(MessageKey::Warning));
            }
            ProgressEvent::Finished(report) => {
                operation.processed = report.bytes_read.max(report.bytes_written);
            }
        }
        cx.notify();
    }

    fn finish_job(
        &mut self,
        result: Result<JobOutput, String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.dismiss_password_prompt(window, cx);
        let cancelled = self
            .operation
            .as_ref()
            .is_some_and(|operation| operation.cancellation.is_cancelled());
        self.operation = None;
        match result {
            Ok(JobOutput::Archive(archive)) => {
                let count = archive.entries.len();
                self.archive = Some(archive);
                self.current_prefix.clear();
                self.expanded_folders.clear();
                self.selected.clear();
                self.status = format!(
                    "{} · {}",
                    self.text(MessageKey::ArchiveOpened),
                    self.text_count(count)
                );
            }
            Ok(JobOutput::Report(report)) => {
                let operation = report.operation.unwrap_or(OperationKind::List);
                let completed = match operation {
                    OperationKind::Extract => self.text(MessageKey::ExtractionCompleted),
                    OperationKind::Create | OperationKind::Update => {
                        self.text(MessageKey::CreationCompleted)
                    }
                    OperationKind::Test => self.text(MessageKey::TestCompleted),
                    OperationKind::List | OperationKind::Detect => {
                        self.text(MessageKey::ArchiveOpened)
                    }
                };
                self.status = format!(
                    "{completed} · {}",
                    self.text_count(report.entries_completed as usize),
                );
            }
            Err(error) if cancelled || error == "operation cancelled" => {
                self.status = self.text(MessageKey::OperationCancelled);
            }
            Err(error) => self.set_error(error, cx),
        }
        cx.notify();
    }

    fn operation_label(&self, kind: OperationKind) -> String {
        self.text(match kind {
            OperationKind::List | OperationKind::Detect => MessageKey::OpeningArchive,
            OperationKind::Extract => MessageKey::Extracting,
            OperationKind::Create | OperationKind::Update => MessageKey::Creating,
            OperationKind::Test => MessageKey::Testing,
        })
    }

    fn text_count(&self, count: usize) -> String {
        format!("{count} {}", self.text(MessageKey::Objects))
    }

    fn set_error(&mut self, error: String, cx: &mut Context<Self>) {
        self.status = format!("{}: {error}", self.text(MessageKey::Error));
        self.operation = None;
        cx.notify();
    }

    fn is_busy(&self) -> bool {
        self.operation.is_some()
    }

    fn apply_preferences(&mut self, cx: &mut Context<Self>) {
        let locale = match self.settings.ui.language {
            LanguagePreference::System => Locale::from_system(),
            LanguagePreference::English => Locale::En,
            LanguagePreference::ZhCn => Locale::ZhCn,
        };
        self.i18n = I18n::new(locale);
        crate::apply_theme_preference(self.settings.ui.theme, None, cx);
    }

    fn run_menu(&mut self, action: MenuAction, window: &mut Window, cx: &mut Context<Self>) {
        match action {
            MenuAction::OpenArchive => self.open_archive(window, cx),
            MenuAction::NewArchive => self.new_archive(window, cx),
            MenuAction::ExtractTo => self.extract_to(window, cx),
            MenuAction::TestArchive => self.test_archive(window, cx),
            MenuAction::CloseArchive => self.close_archive(cx),
            MenuAction::CancelOperation => self.cancel_operation(window, cx),
            MenuAction::SelectAll => self.select_all(cx),
            MenuAction::OpenSettings => self.open_settings(cx),
            MenuAction::RestoreDefaults => self.restore_defaults(cx),
            MenuAction::ToggleFolderPane => self.update_settings(
                |settings| settings.ui.show_folder_pane = !settings.ui.show_folder_pane,
                cx,
            ),
            MenuAction::ToggleArchivePath => self.update_settings(
                |settings| settings.ui.show_archive_path = !settings.ui.show_archive_path,
                cx,
            ),
            MenuAction::AboutPulp => {
                self.status = self.text(MessageKey::AboutPulp);
                cx.notify();
            }
        }
    }

    fn render_menu_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let entity = cx.entity();
        let state = MenuState {
            has_archive: self.archive.is_some(),
            busy: self.is_busy(),
        };
        let i18n = self.i18n.clone();
        h_flex()
            .gap_1()
            .child(top_menu_button(
                "pulp-file-menu",
                i18n.text(MessageKey::File),
                entity.clone(),
                i18n.clone(),
                state,
                build_file_menu,
            ))
            .child(top_menu_button(
                "pulp-edit-menu",
                i18n.text(MessageKey::Edit),
                entity.clone(),
                i18n.clone(),
                state,
                build_edit_menu,
            ))
            .child(top_menu_button(
                "pulp-view-menu",
                i18n.text(MessageKey::View),
                entity.clone(),
                i18n.clone(),
                state,
                build_view_menu,
            ))
            .child(top_menu_button(
                "pulp-help-menu",
                i18n.text(MessageKey::Help),
                entity,
                i18n,
                state,
                build_help_menu,
            ))
    }

    fn render_title_bar(&self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let archive_label = if self.settings_open {
            None
        } else {
            self.archive
                .as_ref()
                .and_then(|archive| archive.path.file_name())
                .map(|name| name.to_string_lossy().into_owned())
        };
        let quick_extract = (!self.settings_open && self.archive.is_some()).then(|| {
            let entity = cx.entity();
            let busy = self.is_busy();
            let button_icon = if busy {
                LucideIcon::Square
            } else {
                LucideIcon::Play
            };
            let tooltip = if busy {
                MessageKey::StopOperation
            } else {
                MessageKey::QuickExtract
            };
            Button::new("pulp-quick-extract")
                .success()
                .small()
                .compact()
                .when(busy, |button| button.danger())
                .tooltip(self.text(tooltip))
                .child(icon(button_icon, 15.))
                .on_click(move |_, window, app| {
                    entity.update(app, |workspace, cx| {
                        if busy {
                            workspace.cancel_operation(window, cx);
                        } else {
                            workspace.quick_extract(window, cx);
                        }
                    });
                })
        });
        let state = window.use_state(cx, |_, _| TitleBarState { should_move: false });
        let drag_area = div()
            .id("pulp-title-bar-drag-area")
            .h_full()
            .min_w_0()
            .flex_1()
            .flex()
            .items_center()
            .px_2()
            .window_control_area(WindowControlArea::Drag)
            .on_mouse_down(
                MouseButton::Left,
                window.listener_for(&state, |state, _, _, _| {
                    state.should_move = true;
                }),
            )
            .on_mouse_up(
                MouseButton::Left,
                window.listener_for(&state, |state, _, _, _| {
                    state.should_move = false;
                }),
            )
            .on_mouse_move(window.listener_for(&state, |state, _, window, _| {
                if state.should_move {
                    state.should_move = false;
                    window.start_window_move();
                }
            }))
            .on_mouse_down_out(window.listener_for(&state, |state, _, _, _| {
                state.should_move = false;
            }))
            .on_double_click(|_, window, _| window.zoom_window())
            .when_some(archive_label, |this, archive_label| {
                this.child(
                    div()
                        .min_w_0()
                        .flex_1()
                        .truncate()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child(archive_label),
                )
            });
        h_flex()
            .id("pulp-title-bar")
            .w_full()
            .h(gpui::px(34.))
            .flex_shrink_0()
            .items_center()
            .border_b_1()
            .border_color(cx.theme().title_bar_border)
            .bg(cx.theme().title_bar)
            .child(self.render_menu_bar(cx))
            .child(drag_area)
            .when_some(quick_extract, |this, quick_extract| {
                this.child(div().h_full().flex().items_center().child(quick_extract))
            })
            .child(window_control_buttons(window, cx))
    }

    fn render_status_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let progress = self.operation.as_ref().and_then(|operation| {
            operation
                .total
                .filter(|total| *total > 0)
                .map(|total| (operation.processed.min(total) as f32 / total as f32).clamp(0., 1.))
        });
        let current = self
            .operation
            .as_ref()
            .and_then(|operation| operation.current.as_deref())
            .map(|current| format!(" · {current}"))
            .unwrap_or_default();
        h_flex()
            .w_full()
            .h(gpui::px(28.))
            .flex_shrink_0()
            .relative()
            .px_2()
            .gap_2()
            .border_t_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().background)
            .text_sm()
            .text_color(cx.theme().muted_foreground)
            .child(icon(LucideIcon::Info, 14.))
            .child(
                div()
                    .min_w_0()
                    .flex_1()
                    .truncate()
                    .child(format!("{}{}", self.status, current)),
            )
            .when_some(progress, |this, progress| {
                this.child(
                    div()
                        .w(gpui::px(150.))
                        .h(gpui::px(4.))
                        .rounded(gpui::px(2.))
                        .bg(cx.theme().progress_bar)
                        .child(
                            div()
                                .h_full()
                                .w(gpui::px(150. * progress))
                                .rounded(gpui::px(2.))
                                .bg(cx.theme().accent),
                        ),
                )
            })
    }
}

fn names_between(names: &[String], first: &str, last: &str) -> HashSet<String> {
    let Some(first_index) = names.iter().position(|name| name == first) else {
        return HashSet::from([last.to_owned()]);
    };
    let Some(last_index) = names.iter().position(|name| name == last) else {
        return HashSet::from([first.to_owned()]);
    };
    let (start, end) = if first_index <= last_index {
        (first_index, last_index)
    } else {
        (last_index, first_index)
    };
    names[start..=end].iter().cloned().collect()
}

impl Render for Workspace {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let entity = cx.entity();
        div()
            .id("pulp-workspace")
            .size_full()
            .flex()
            .flex_col()
            .on_drop(move |paths: &ExternalPaths, window, app| {
                let paths = paths.paths().to_vec();
                entity.update(app, |workspace, cx| {
                    workspace.handle_drop(&paths, window, cx)
                });
            })
            .child(self.render_title_bar(window, cx))
            .child(div().flex_1().min_h_0().min_w_0().overflow_hidden().child(
                if self.settings_open {
                    views::render_settings(self, window, cx).into_any_element()
                } else {
                    views::render_archive(self, window, cx).into_any_element()
                },
            ))
            .child(self.render_status_bar(cx))
            // `Root` owns dialogs, but gpui-component leaves the layer to the app view.
            .children(Root::render_dialog_layer(window, cx))
    }
}

#[derive(Clone, Copy)]
struct MenuState {
    has_archive: bool,
    busy: bool,
}

fn top_menu_button(
    id: &'static str,
    label: String,
    entity: Entity<Workspace>,
    i18n: I18n,
    state: MenuState,
    builder: fn(PopupMenu, Entity<Workspace>, I18n, MenuState) -> PopupMenu,
) -> impl IntoElement {
    Button::new(id)
        .ghost()
        .small()
        .child(label)
        .dropdown_menu(move |menu, _, _| builder(menu, entity.clone(), i18n.clone(), state))
}

fn build_file_menu(
    menu: PopupMenu,
    entity: Entity<Workspace>,
    i18n: I18n,
    state: MenuState,
) -> PopupMenu {
    menu.min_w(gpui::px(300.))
        .item(menu_item(
            i18n.text(MessageKey::OpenArchive),
            false,
            entity.clone(),
            MenuAction::OpenArchive,
        ))
        .item(menu_item(
            i18n.text(MessageKey::NewArchive),
            false,
            entity.clone(),
            MenuAction::NewArchive,
        ))
        .item(PopupMenuItem::separator())
        .item(menu_item(
            i18n.text(MessageKey::ExtractTo),
            !state.has_archive || state.busy,
            entity.clone(),
            MenuAction::ExtractTo,
        ))
        .item(menu_item(
            i18n.text(MessageKey::Test),
            !state.has_archive || state.busy,
            entity.clone(),
            MenuAction::TestArchive,
        ))
        .item(PopupMenuItem::separator())
        .item(menu_item(
            i18n.text(MessageKey::CloseArchive),
            !state.has_archive || state.busy,
            entity.clone(),
            MenuAction::CloseArchive,
        ))
        .item(menu_item(
            i18n.text(MessageKey::CancelOperation),
            !state.busy,
            entity,
            MenuAction::CancelOperation,
        ))
}

fn build_edit_menu(
    menu: PopupMenu,
    entity: Entity<Workspace>,
    i18n: I18n,
    state: MenuState,
) -> PopupMenu {
    menu.min_w(gpui::px(300.))
        .item(menu_item(
            i18n.text(MessageKey::SelectAll),
            !state.has_archive,
            entity.clone(),
            MenuAction::SelectAll,
        ))
        .item(PopupMenuItem::separator())
        .item(menu_item(
            i18n.text(MessageKey::OpenSettings),
            false,
            entity,
            MenuAction::OpenSettings,
        ))
}

fn build_view_menu(
    menu: PopupMenu,
    entity: Entity<Workspace>,
    i18n: I18n,
    _state: MenuState,
) -> PopupMenu {
    menu.min_w(gpui::px(300.))
        .item(menu_item(
            i18n.text(MessageKey::ShowFolderPane),
            false,
            entity.clone(),
            MenuAction::ToggleFolderPane,
        ))
        .item(menu_item(
            i18n.text(MessageKey::ShowArchivePath),
            false,
            entity.clone(),
            MenuAction::ToggleArchivePath,
        ))
}

fn build_help_menu(
    menu: PopupMenu,
    entity: Entity<Workspace>,
    i18n: I18n,
    _: MenuState,
) -> PopupMenu {
    menu.min_w(gpui::px(300.)).item(menu_item(
        i18n.text(MessageKey::AboutPulp),
        false,
        entity,
        MenuAction::AboutPulp,
    ))
}

fn menu_item(
    label: String,
    disabled: bool,
    entity: Entity<Workspace>,
    action: MenuAction,
) -> PopupMenuItem {
    PopupMenuItem::element(move |_, _| h_flex().w_full().child(label.clone()))
        .disabled(disabled)
        .on_click(move |_, window, app| {
            entity.update(app, |workspace, cx| {
                workspace.run_menu(action, window, cx);
            });
        })
}

fn menu_action_for_label(label: MessageKey) -> Option<MenuAction> {
    Some(match label {
        MessageKey::OpenArchive => MenuAction::OpenArchive,
        MessageKey::NewArchive => MenuAction::NewArchive,
        MessageKey::ExtractTo => MenuAction::ExtractTo,
        MessageKey::RestoreDefaults => MenuAction::RestoreDefaults,
        _ => return None,
    })
}

#[derive(Clone, Copy)]
enum WindowAction {
    Minimize,
    ToggleMaximize,
    Close,
}

fn window_control_buttons(window: &Window, cx: &mut Context<Workspace>) -> impl IntoElement {
    let maximize_icon = if window.is_maximized() {
        LucideIcon::Copy
    } else {
        LucideIcon::Square
    };

    h_flex()
        .id("pulp-window-controls")
        .h_full()
        .flex_shrink_0()
        .items_center()
        .when(!cfg!(target_os = "macos"), |this| {
            this.child(window_control_button(
                "pulp-window-minimize",
                LucideIcon::Minus,
                WindowControlArea::Min,
                WindowAction::Minimize,
                cx,
            ))
            .child(window_control_button(
                "pulp-window-maximize",
                maximize_icon,
                WindowControlArea::Max,
                WindowAction::ToggleMaximize,
                cx,
            ))
            .child(window_control_button(
                "pulp-window-close",
                LucideIcon::X,
                WindowControlArea::Close,
                WindowAction::Close,
                cx,
            ))
        })
}

fn window_control_button(
    id: &'static str,
    button_icon: LucideIcon,
    area: WindowControlArea,
    action: WindowAction,
    cx: &mut Context<Workspace>,
) -> impl IntoElement {
    let close = matches!(action, WindowAction::Close);
    let hover_foreground = if close {
        cx.theme().danger_foreground
    } else {
        cx.theme().secondary_foreground
    };
    let hover_background = if close {
        cx.theme().danger
    } else {
        cx.theme().secondary_hover
    };
    let active_background = if close {
        cx.theme().danger_active
    } else {
        cx.theme().secondary_active
    };

    div()
        .id(id)
        .w(gpui::px(34.))
        .h_full()
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_center()
        .text_color(cx.theme().foreground)
        .hover(|style| style.bg(hover_background).text_color(hover_foreground))
        .active(|style| style.bg(active_background).text_color(hover_foreground))
        .when(cfg!(target_os = "windows"), |this| {
            this.window_control_area(area)
        })
        .when(
            !cfg!(target_os = "windows") && !cfg!(target_os = "macos"),
            |this| {
                this.on_mouse_down(MouseButton::Left, |_, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                })
                .on_click(move |_, window, cx| {
                    cx.stop_propagation();
                    match action {
                        WindowAction::Minimize => window.minimize_window(),
                        WindowAction::ToggleMaximize => window.zoom_window(),
                        WindowAction::Close => window.remove_window(),
                    }
                })
            },
        )
        .child(icon(button_icon, 15.))
}

fn icon(icon: LucideIcon, size: f32) -> gpui::Div {
    div()
        .w(gpui::px(size))
        .h(gpui::px(size))
        .flex()
        .items_center()
        .justify_center()
        .font_family("lucide")
        .text_size(gpui::px(size))
        .child(icon.unicode().to_string())
}

#[cfg(test)]
mod tests {
    use super::names_between;

    #[test]
    fn range_selection_is_inclusive_in_both_directions() {
        let names = ["a", "b", "c", "d"]
            .into_iter()
            .map(str::to_owned)
            .collect::<Vec<_>>();

        assert_eq!(
            names_between(&names, "a", "c"),
            ["a", "b", "c"].into_iter().map(str::to_owned).collect()
        );
        assert_eq!(
            names_between(&names, "d", "b"),
            ["b", "c", "d"].into_iter().map(str::to_owned).collect()
        );
    }
}
