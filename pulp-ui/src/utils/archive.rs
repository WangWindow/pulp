use pulp_core::{
    ArchiveFormat, ArchiveService, ArchiveSource, CancellationToken, CompressOptions,
    DefaultArchiveService, ExtractOptions, ListOptions, ProgressEvent, ProgressReporter, TaskId,
    create_default_service,
};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

static TASK_SEQ: AtomicU64 = AtomicU64::new(1);

fn next_task_id() -> TaskId {
    TASK_SEQ.fetch_add(1, Ordering::Relaxed)
}

#[derive(Debug, Default, Clone)]
struct UiProgressReporter;

impl ProgressReporter for UiProgressReporter {
    fn report(&self, _event: ProgressEvent) {}
}

fn service() -> DefaultArchiveService {
    create_default_service()
}

/// 列出压缩包条目。
pub async fn list_archive(
    path: PathBuf,
) -> Result<(PathBuf, Vec<pulp_core::ArchiveEntry>), String> {
    let service = service();
    let reporter = UiProgressReporter;
    let cancel = CancellationToken::new();
    let task_id = next_task_id();
    let title = format!("list: {}", path.display());

    let entries = service
        .list(
            task_id,
            title,
            ArchiveSource::new(path.clone()),
            ListOptions::default(),
            &reporter,
            &cancel,
        )
        .await
        .map_err(|e| e.to_string())?;

    Ok((path, entries))
}

/// 解压压缩包到目标目录。
pub async fn extract_archive(
    archive: PathBuf,
    dest_dir: PathBuf,
    options: ExtractOptions,
    cancel: CancellationToken,
) -> Result<PathBuf, String> {
    let service = service();
    let reporter = UiProgressReporter;
    let task_id = next_task_id();
    let title = format!("extract: {}", archive.display());

    service
        .extract(
            task_id,
            title,
            ArchiveSource::new(archive),
            dest_dir.clone(),
            options,
            &reporter,
            &cancel,
        )
        .await
        .map_err(|e| e.to_string())?;

    Ok(dest_dir)
}

/// 压缩文件/目录到目标压缩包。
pub async fn compress_archive(
    inputs: Vec<PathBuf>,
    dest_archive: PathBuf,
    format: ArchiveFormat,
    options: CompressOptions,
    cancel: CancellationToken,
) -> Result<PathBuf, String> {
    let service = service();
    let reporter = UiProgressReporter;
    let task_id = next_task_id();
    let title = format!("compress: {}", dest_archive.display());

    service
        .compress(
            task_id,
            title,
            inputs,
            dest_archive.clone(),
            Some(format),
            options,
            &reporter,
            &cancel,
        )
        .await
        .map_err(|e| e.to_string())?;

    Ok(dest_archive)
}
