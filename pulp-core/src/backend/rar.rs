use crate::{
    backend::TaskContext,
    domain::{
        ArchiveEntry, ArchiveFormat, ArchiveSource, CompressOptions, ExtractOptions, ListOptions,
    },
    portal::{
        error::{PulpError, Result},
        progress::{EntryProgress, ProgressEvent},
    },
    utils::security::pathlib::{ExtractPathOptions, build_output_path},
};
use async_trait::async_trait;
use std::path::{Path, PathBuf};
use unrar::{Archive, error::Code};

/// RAR 后端（基于 unrar 库，仅支持 list/extract）。
///
/// 说明：
/// - 该后端依赖系统级 unrar 库；
/// - RAR 创建暂不支持，compress 会返回 Unsupported。
#[derive(Debug, Default, Clone)]
pub struct RarBackend;

impl RarBackend {
    fn map_unrar_err(err: unrar::error::UnrarError) -> PulpError {
        match err.code {
            Code::MissingPassword | Code::BadPassword => PulpError::PasswordRequired,
            Code::BadArchive | Code::BadData => PulpError::InvalidArchive(err.to_string()),
            Code::UnknownFormat => PulpError::Unsupported("Unsupported RAR format".to_string()),
            _ => PulpError::backend("rar", err.to_string()),
        }
    }

    fn entry_progress(
        path: &Path,
        name: &str,
        total: Option<u64>,
        processed: u64,
        is_dir: bool,
    ) -> EntryProgress {
        EntryProgress {
            path: path.to_path_buf(),
            name: name.to_string(),
            total_bytes: total,
            processed_bytes: processed,
            is_dir,
        }
    }

    // 使用内联 open_for_listing/open_for_processing，避免暴露 unrar 的泛型类型。
}

#[async_trait]
impl crate::backend::ArchiveBackend for RarBackend {
    fn name(&self) -> &'static str {
        "rar"
    }

    fn supported_formats(&self) -> &'static [ArchiveFormat] {
        &[ArchiveFormat::Rar]
    }

    async fn list(
        &self,
        source: &ArchiveSource,
        options: &ListOptions,
        _ctx: &TaskContext<'_>,
    ) -> Result<Vec<ArchiveEntry>> {
        let archive = if let Some(password) = options.password.as_deref() {
            Archive::with_password(&source.path, password)
                .open_for_listing()
                .map_err(Self::map_unrar_err)?
        } else {
            Archive::new(&source.path)
                .open_for_listing()
                .map_err(Self::map_unrar_err)?
        };

        let mut entries = Vec::new();
        for item in archive {
            let entry = item.map_err(Self::map_unrar_err)?;
            let is_dir = entry.is_directory();
            let name = entry.filename.to_string_lossy().to_string();

            entries.push(ArchiveEntry {
                path: name,
                is_dir,
                size: if is_dir {
                    None
                } else {
                    Some(entry.unpacked_size)
                },
                compressed_size: None,
                modified: None,
            });
        }

        Ok(entries)
    }

    async fn extract(
        &self,
        source: &ArchiveSource,
        dest_dir: &Path,
        options: &ExtractOptions,
        ctx: &TaskContext<'_>,
    ) -> Result<()> {
        let mut archive = if let Some(password) = options.password.as_deref() {
            Archive::with_password(&source.path, password)
                .open_for_processing()
                .map_err(Self::map_unrar_err)?
        } else {
            Archive::new(&source.path)
                .open_for_processing()
                .map_err(Self::map_unrar_err)?
        };
        let safe_opts = ExtractPathOptions {
            preserve_paths: options.preserve_paths,
            strip_components: options.strip_components,
        };

        while let Some(header) = archive.read_header().map_err(Self::map_unrar_err)? {
            ctx.cancel.throw_if_cancelled()?;

            let entry = header.entry();
            let name = entry.filename.to_string_lossy().to_string();
            let is_dir = entry.is_directory();

            let out_path = match build_output_path(dest_dir, &name, &safe_opts) {
                Ok(Some(p)) => p,
                Ok(None) => {
                    ctx.warn(format!("Skipped entry (empty or stripped path): {name}"));
                    archive = header.skip().map_err(Self::map_unrar_err)?;
                    continue;
                }
                Err(e) => {
                    ctx.warn(format!("Skipped unsafe path {name}: {e:?}"));
                    archive = header.skip().map_err(Self::map_unrar_err)?;
                    continue;
                }
            };

            if is_dir {
                std::fs::create_dir_all(&out_path)?;
                archive = header.skip().map_err(Self::map_unrar_err)?;
                continue;
            }

            if out_path.exists() && !options.overwrite {
                ctx.warn(format!("Target exists, skipped: {}", out_path.display()));
                archive = header.skip().map_err(Self::map_unrar_err)?;
                continue;
            }

            if let Some(parent) = out_path.parent() {
                std::fs::create_dir_all(parent)?;
            }

            let total = Some(entry.unpacked_size);
            let mut progress = Self::entry_progress(&out_path, &name, total, 0, false);
            ctx.progress.report(ProgressEvent::EntryStarted {
                task_id: ctx.task_id,
                entry: progress.clone(),
            });

            archive = header.extract_to(&out_path).map_err(Self::map_unrar_err)?;
            progress.processed_bytes = total.unwrap_or(0);
            ctx.progress.report(ProgressEvent::EntryFinished {
                task_id: ctx.task_id,
                entry: progress,
            });
        }

        if ctx.cancel.is_cancelled() {
            return Err(PulpError::Cancelled);
        }

        Ok(())
    }

    async fn compress(
        &self,
        _inputs: &[PathBuf],
        _dest_archive: &Path,
        _format: ArchiveFormat,
        _options: &CompressOptions,
        _ctx: &TaskContext<'_>,
    ) -> Result<()> {
        Err(PulpError::Unsupported(
            "RAR creation is not supported (extract/list only)".to_string(),
        ))
    }
}
