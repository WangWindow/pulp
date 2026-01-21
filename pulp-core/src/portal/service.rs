//! ArchiveService：pulp-core 对外统一门面。

use crate::{
    backend::{BackendRegistry, TaskContext},
    domain::{
        ArchiveEntry, ArchiveFormat, ArchiveSource, CompressOptions, ExtractOptions, JobRequest,
        JobResult, ListOptions,
    },
    portal::{
        cancel::CancellationToken,
        error::{PulpError, Result},
        progress::{ProgressEvent, ProgressReporter, TaskId, TaskKind},
    },
};
use std::{future::Future, pin::Pin, sync::Arc};

/// 返回 Result 的异步 Future 类型别名。
pub type ResultFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T>> + Send + 'a>>;

/// ArchiveService：对外统一门面。
pub trait ArchiveService: Send + Sync {
    fn supported_formats(&self) -> Vec<ArchiveFormat>;

    fn execute<'a>(
        &'a self,
        req: JobRequest,
        progress: &'a dyn ProgressReporter,
        cancel: &'a CancellationToken,
    ) -> ResultFuture<'a, JobResult>;

    fn list<'a>(
        &'a self,
        task_id: TaskId,
        title: String,
        source: ArchiveSource,
        options: ListOptions,
        progress: &'a dyn ProgressReporter,
        cancel: &'a CancellationToken,
    ) -> ResultFuture<'a, Vec<ArchiveEntry>>;

    fn extract<'a>(
        &'a self,
        task_id: TaskId,
        title: String,
        source: ArchiveSource,
        dest_dir: std::path::PathBuf,
        options: ExtractOptions,
        progress: &'a dyn ProgressReporter,
        cancel: &'a CancellationToken,
    ) -> ResultFuture<'a, ()>;

    fn compress<'a>(
        &'a self,
        task_id: TaskId,
        title: String,
        inputs: Vec<std::path::PathBuf>,
        dest_archive: std::path::PathBuf,
        format: Option<ArchiveFormat>,
        options: CompressOptions,
        progress: &'a dyn ProgressReporter,
        cancel: &'a CancellationToken,
    ) -> ResultFuture<'a, ()>;
}

/// 默认实现：基于 BackendRegistry 的 service。
#[derive(Clone)]
pub struct DefaultArchiveService {
    registry: Arc<BackendRegistry>,
}

impl DefaultArchiveService {
    pub fn new(registry: BackendRegistry) -> Self {
        Self {
            registry: Arc::new(registry),
        }
    }

    pub fn registry(&self) -> &BackendRegistry {
        &self.registry
    }

    fn announce_start(ctx: &TaskContext<'_>, kind: TaskKind, title: &str) {
        ctx.progress.report(ProgressEvent::TaskStarted {
            task_id: ctx.task_id,
            kind,
            title: title.to_string(),
        });
    }

    fn announce_finish(ctx: &TaskContext<'_>) {
        ctx.progress.report(ProgressEvent::TaskFinished {
            task_id: ctx.task_id,
        });
    }

    fn announce_cancel(ctx: &TaskContext<'_>) {
        ctx.progress.report(ProgressEvent::TaskCancelled {
            task_id: ctx.task_id,
        });
    }

    fn announce_fail(ctx: &TaskContext<'_>, err: &PulpError) {
        ctx.progress.report(ProgressEvent::TaskFailed {
            task_id: ctx.task_id,
            message: err.to_string(),
        });
    }
}

impl ArchiveService for DefaultArchiveService {
    fn supported_formats(&self) -> Vec<ArchiveFormat> {
        self.registry.registered_formats()
    }

    fn execute<'a>(
        &'a self,
        req: JobRequest,
        progress: &'a dyn ProgressReporter,
        cancel: &'a CancellationToken,
    ) -> ResultFuture<'a, JobResult> {
        Box::pin(async move {
            match req {
                JobRequest::List {
                    task_id,
                    source,
                    options,
                    title,
                } => {
                    let ctx = TaskContext::new(task_id, progress, cancel);
                    Self::announce_start(&ctx, TaskKind::List, &title);

                    let result: Result<Vec<ArchiveEntry>> =
                        self.registry.list(&source, &options, &ctx).await;

                    match result {
                        Ok(entries) => {
                            Self::announce_finish(&ctx);
                            Ok(JobResult::List { entries })
                        }
                        Err(err) if err.is_cancelled() => {
                            Self::announce_cancel(&ctx);
                            Err(err)
                        }
                        Err(err) => {
                            Self::announce_fail(&ctx, &err);
                            Err(err)
                        }
                    }
                }
                JobRequest::Extract {
                    task_id,
                    source,
                    dest_dir,
                    options,
                    title,
                } => {
                    let ctx = TaskContext::new(task_id, progress, cancel);
                    Self::announce_start(&ctx, TaskKind::Extract, &title);

                    let result = self
                        .registry
                        .extract(&source, &dest_dir, &options, &ctx)
                        .await;

                    match result {
                        Ok(()) => {
                            Self::announce_finish(&ctx);
                            Ok(JobResult::Extract { dest_dir })
                        }
                        Err(err) if err.is_cancelled() => {
                            Self::announce_cancel(&ctx);
                            Err(err)
                        }
                        Err(err) => {
                            Self::announce_fail(&ctx, &err);
                            Err(err)
                        }
                    }
                }
                JobRequest::Compress {
                    task_id,
                    inputs,
                    dest_archive,
                    format,
                    options,
                    title,
                } => {
                    let ctx = TaskContext::new(task_id, progress, cancel);
                    Self::announce_start(&ctx, TaskKind::Compress, &title);

                    let fmt = format.unwrap_or_else(|| {
                        ArchiveFormat::from_path(&dest_archive).unwrap_or(ArchiveFormat::Zip)
                    });

                    let result = self
                        .registry
                        .compress(&inputs, &dest_archive, fmt, &options, &ctx)
                        .await;

                    match result {
                        Ok(()) => {
                            Self::announce_finish(&ctx);
                            Ok(JobResult::Compress { dest_archive })
                        }
                        Err(err) if err.is_cancelled() => {
                            Self::announce_cancel(&ctx);
                            Err(err)
                        }
                        Err(err) => {
                            Self::announce_fail(&ctx, &err);
                            Err(err)
                        }
                    }
                }
            }
        })
    }

    fn list<'a>(
        &'a self,
        task_id: TaskId,
        title: String,
        source: ArchiveSource,
        options: ListOptions,
        progress: &'a dyn ProgressReporter,
        cancel: &'a CancellationToken,
    ) -> ResultFuture<'a, Vec<ArchiveEntry>> {
        Box::pin(async move {
            let result = self
                .execute(
                    JobRequest::List {
                        task_id,
                        source,
                        options,
                        title,
                    },
                    progress,
                    cancel,
                )
                .await?;

            match result {
                JobResult::List { entries } => Ok(entries),
                _ => Err(PulpError::Unsupported(
                    "List job returned unexpected result type".to_string(),
                )),
            }
        })
    }

    fn extract<'a>(
        &'a self,
        task_id: TaskId,
        title: String,
        source: ArchiveSource,
        dest_dir: std::path::PathBuf,
        options: ExtractOptions,
        progress: &'a dyn ProgressReporter,
        cancel: &'a CancellationToken,
    ) -> ResultFuture<'a, ()> {
        Box::pin(async move {
            let result = self
                .execute(
                    JobRequest::Extract {
                        task_id,
                        source,
                        dest_dir: dest_dir.clone(),
                        options,
                        title,
                    },
                    progress,
                    cancel,
                )
                .await?;

            match result {
                JobResult::Extract { .. } => Ok(()),
                _ => Err(PulpError::Unsupported(
                    "Extract job returned unexpected result type".to_string(),
                )),
            }
        })
    }

    fn compress<'a>(
        &'a self,
        task_id: TaskId,
        title: String,
        inputs: Vec<std::path::PathBuf>,
        dest_archive: std::path::PathBuf,
        format: Option<ArchiveFormat>,
        options: CompressOptions,
        progress: &'a dyn ProgressReporter,
        cancel: &'a CancellationToken,
    ) -> ResultFuture<'a, ()> {
        Box::pin(async move {
            let result = self
                .execute(
                    JobRequest::Compress {
                        task_id,
                        inputs,
                        dest_archive,
                        format,
                        options,
                        title,
                    },
                    progress,
                    cancel,
                )
                .await?;

            match result {
                JobResult::Compress { .. } => Ok(()),
                _ => Err(PulpError::Unsupported(
                    "Compress job returned unexpected result type".to_string(),
                )),
            }
        })
    }
}
