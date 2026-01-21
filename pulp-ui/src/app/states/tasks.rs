//! 与 core 交互的异步任务入口。

use super::App;
use crate::domain::Message;
use crate::utils::{archive, fs};
use iced::Task;
use pulp_core::{ArchiveFormat, CancellationToken};
use rust_i18n::t;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub(super) enum UiTask {
    ListArchive(PathBuf),
    Extract { archive: PathBuf, dest: PathBuf },
    Compress { inputs: Vec<PathBuf>, dest: PathBuf },
    CreateFolder { parent: PathBuf, name: String },
    Rename { from: PathBuf, to: PathBuf },
    Delete { target: PathBuf },
}

impl UiTask {
    pub(super) fn label(&self) -> String {
        match self {
            UiTask::ListArchive(path) => {
                format!("{} · {}", t!("task.list_archive"), file_label(path))
            }
            UiTask::Extract { archive, .. } => {
                format!("{} · {}", t!("task.extract"), file_label(archive))
            }
            UiTask::Compress { inputs, .. } => {
                let head = inputs
                    .first()
                    .map(file_label)
                    .unwrap_or_else(|| t!("task.compress").to_string());
                if inputs.len() > 1 {
                    format!("{} · {} (+{})", t!("task.compress"), head, inputs.len() - 1)
                } else {
                    format!("{} · {}", t!("task.compress"), head)
                }
            }
            UiTask::CreateFolder { name, .. } => {
                format!("{} · {}", t!("task.create_folder"), name)
            }
            UiTask::Rename { to, .. } => {
                format!("{} · {}", t!("task.rename"), file_label(to))
            }
            UiTask::Delete { target } => {
                format!("{} · {}", t!("task.delete"), file_label(target))
            }
        }
    }
}

fn file_label(path: &PathBuf) -> String {
    path.file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| path.display().to_string())
}

impl App {
    pub(super) fn enqueue_task(&mut self, task: UiTask) -> Task<Message> {
        self.task_queue.push_back(task);
        if self.task_running {
            Task::none()
        } else {
            self.start_next_task()
        }
    }

    pub(super) fn finish_task_and_next(&mut self) -> Task<Message> {
        self.task_running = false;
        self.start_next_task()
    }

    fn start_next_task(&mut self) -> Task<Message> {
        let Some(task) = self.task_queue.pop_front() else {
            self.task_running = false;
            return Task::none();
        };

        self.task_running = true;

        match task {
            UiTask::ListArchive(path) => self.start_open_archive(path),
            UiTask::Extract { archive, dest } => self.start_extract_task(archive, dest),
            UiTask::Compress { inputs, dest } => self.start_compress_task(inputs, dest),
            UiTask::CreateFolder { parent, name } => {
                self.status = t!("status.creating_folder").to_string();
                self.busy = true;
                Task::perform(fs::create_folder(parent, name), Message::FolderCreated)
            }
            UiTask::Rename { from, to } => {
                self.status = t!("status.renaming").to_string();
                self.busy = true;
                Task::perform(fs::rename_path(from, to), Message::Renamed)
            }
            UiTask::Delete { target } => {
                self.status = t!("status.deleting").to_string();
                self.busy = true;
                Task::perform(fs::delete_path(target), Message::DeleteFinished)
            }
        }
    }

    pub(super) fn start_open_archive(&mut self, path: PathBuf) -> Task<Message> {
        self.active_archive = None;
        self.busy = true;
        self.status = t!("status.reading_archive").to_string();
        self.active_task_error = None;
        self.active_task_title = Some(t!("task.list_archive").to_string());
        self.active_task_current_entry = None;
        self.active_task_progress = None;
        self.active_task_finished = false;
        self.active_task_cancelled = false;
        self.active_task_cancel = None;

        Task::perform(archive::list_archive(path), Message::ArchiveLoaded)
    }

    pub(super) fn start_extract_task(&mut self, archive: PathBuf, dest: PathBuf) -> Task<Message> {
        self.status = t!("status.extracting").to_string();
        self.busy = true;

        self.active_task_title = Some(t!("task.extract").to_string());
        self.active_task_current_entry = None;
        self.active_task_progress = None;
        self.active_task_finished = false;
        self.active_task_cancelled = false;
        self.active_task_error = None;

        let cancel = CancellationToken::new();
        self.active_task_cancel = Some(cancel.clone());

        let options = pulp_core::ExtractOptions::default();
        Task::perform(
            archive::extract_archive(archive, dest, options, cancel),
            Message::ExtractFinished,
        )
    }

    pub(super) fn start_compress_task(
        &mut self,
        inputs: Vec<PathBuf>,
        dest: PathBuf,
    ) -> Task<Message> {
        self.status = t!("status.compressing").to_string();
        self.busy = true;

        self.active_task_title = Some(t!("task.compress").to_string());
        self.active_task_current_entry = None;
        self.active_task_progress = None;
        self.active_task_finished = false;
        self.active_task_cancelled = false;
        self.active_task_error = None;

        let cancel = CancellationToken::new();
        self.active_task_cancel = Some(cancel.clone());

        let options = pulp_core::CompressOptions::default();
        Task::perform(
            archive::compress_archive(inputs, dest, ArchiveFormat::Zip, options, cancel),
            Message::CompressFinished,
        )
    }
}
