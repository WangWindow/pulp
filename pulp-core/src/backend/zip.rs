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
use std::{
    fs::File,
    io::{Read, Write},
    path::{Path, PathBuf},
};
use zip::{CompressionMethod, ZipArchive, ZipWriter, write::FileOptions};

/// ZIP 后端（纯 Rust）。
#[derive(Debug, Default, Clone)]
pub struct ZipBackend;

impl ZipBackend {
    fn map_zip_err(err: zip::result::ZipError) -> PulpError {
        PulpError::backend("zip", err.to_string())
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
}

#[async_trait]
impl crate::backend::ArchiveBackend for ZipBackend {
    fn name(&self) -> &'static str {
        "zip"
    }

    fn supported_formats(&self) -> &'static [ArchiveFormat] {
        &[ArchiveFormat::Zip]
    }

    async fn list(
        &self,
        source: &ArchiveSource,
        _options: &ListOptions,
        _ctx: &TaskContext<'_>,
    ) -> Result<Vec<ArchiveEntry>> {
        let file = File::open(&source.path)?;
        let mut archive = ZipArchive::new(file).map_err(Self::map_zip_err)?;

        let mut entries = Vec::with_capacity(archive.len());
        for i in 0..archive.len() {
            let entry = archive.by_index(i).map_err(Self::map_zip_err)?;
            let is_dir = entry.is_dir();
            let name = entry.name().to_string();
            entries.push(ArchiveEntry {
                path: name,
                is_dir,
                size: if is_dir { None } else { Some(entry.size()) },
                compressed_size: if is_dir {
                    None
                } else {
                    Some(entry.compressed_size())
                },
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
        let file = File::open(&source.path)?;
        let mut archive = ZipArchive::new(file).map_err(Self::map_zip_err)?;
        let safe_opts = ExtractPathOptions {
            preserve_paths: options.preserve_paths,
            strip_components: options.strip_components,
        };

        for i in 0..archive.len() {
            ctx.cancel.throw_if_cancelled()?;
            let mut entry = archive.by_index(i).map_err(Self::map_zip_err)?;
            let name = entry.name().to_string();
            let is_dir = entry.is_dir();

            let out_path = match build_output_path(dest_dir, &name, &safe_opts) {
                Ok(Some(p)) => p,
                Ok(None) => {
                    ctx.warn(format!("Skipped entry (empty or stripped path): {name}"));
                    continue;
                }
                Err(e) => {
                    ctx.warn(format!("Skipped unsafe path {name}: {e:?}"));
                    continue;
                }
            };

            if is_dir {
                std::fs::create_dir_all(&out_path)?;
                continue;
            }

            if out_path.exists() && !options.overwrite {
                ctx.warn(format!("Target exists, skipped: {}", out_path.display()));
                continue;
            }

            if let Some(parent) = out_path.parent() {
                std::fs::create_dir_all(parent)?;
            }

            let total = Some(entry.size());
            let mut progress = Self::entry_progress(&out_path, &name, total, 0, false);
            ctx.progress.report(ProgressEvent::EntryStarted {
                task_id: ctx.task_id,
                entry: progress.clone(),
            });

            let mut outfile = File::create(&out_path)?;
            let mut buf = [0u8; 64 * 1024];
            loop {
                let n = entry.read(&mut buf)?;
                if n == 0 {
                    break;
                }
                outfile.write_all(&buf[..n])?;
                progress.processed_bytes += n as u64;
                ctx.progress.report(ProgressEvent::EntryProgress {
                    task_id: ctx.task_id,
                    entry: progress.clone(),
                });
            }

            ctx.progress.report(ProgressEvent::EntryFinished {
                task_id: ctx.task_id,
                entry: progress,
            });
        }

        Ok(())
    }

    async fn compress(
        &self,
        inputs: &[PathBuf],
        dest_archive: &Path,
        _format: ArchiveFormat,
        _options: &CompressOptions,
        ctx: &TaskContext<'_>,
    ) -> Result<()> {
        let file = File::create(dest_archive)?;
        let mut writer = ZipWriter::new(file);
        let options: FileOptions<'_, ()> =
            FileOptions::default().compression_method(CompressionMethod::Deflated);

        for input in inputs {
            ctx.cancel.throw_if_cancelled()?;
            let root_name = input
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "root".to_string());

            if input.is_file() {
                let mut f = File::open(input)?;
                let mut progress = Self::entry_progress(input, &root_name, None, 0, false);
                ctx.progress.report(ProgressEvent::EntryStarted {
                    task_id: ctx.task_id,
                    entry: progress.clone(),
                });

                writer
                    .start_file(root_name.clone(), options)
                    .map_err(Self::map_zip_err)?;

                let mut buf = [0u8; 64 * 1024];
                loop {
                    let n = f.read(&mut buf)?;
                    if n == 0 {
                        break;
                    }
                    writer.write_all(&buf[..n])?;
                    progress.processed_bytes += n as u64;
                    ctx.progress.report(ProgressEvent::EntryProgress {
                        task_id: ctx.task_id,
                        entry: progress.clone(),
                    });
                }

                ctx.progress.report(ProgressEvent::EntryFinished {
                    task_id: ctx.task_id,
                    entry: progress,
                });
                continue;
            }

            for entry in walkdir::WalkDir::new(input)
                .into_iter()
                .filter_map(|e| e.ok())
            {
                ctx.cancel.throw_if_cancelled()?;
                let path = entry.path();
                let rel = path.strip_prefix(input).unwrap_or(path);
                let rel_str = if rel.as_os_str().is_empty() {
                    root_name.clone()
                } else {
                    format!("{}/{}", root_name, rel.to_string_lossy())
                };

                if entry.file_type().is_dir() {
                    let dir_name = if rel_str.ends_with('/') {
                        rel_str.clone()
                    } else {
                        format!("{rel_str}/")
                    };
                    writer
                        .add_directory(dir_name, options)
                        .map_err(Self::map_zip_err)?;
                    continue;
                }

                let mut f = File::open(path)?;
                let total = f.metadata().ok().map(|m| m.len());
                let mut progress = Self::entry_progress(path, &rel_str, total, 0, false);
                ctx.progress.report(ProgressEvent::EntryStarted {
                    task_id: ctx.task_id,
                    entry: progress.clone(),
                });

                writer
                    .start_file(rel_str, options)
                    .map_err(Self::map_zip_err)?;

                let mut buf = [0u8; 64 * 1024];
                loop {
                    let n = f.read(&mut buf)?;
                    if n == 0 {
                        break;
                    }
                    writer.write_all(&buf[..n])?;
                    progress.processed_bytes += n as u64;
                    ctx.progress.report(ProgressEvent::EntryProgress {
                        task_id: ctx.task_id,
                        entry: progress.clone(),
                    });
                }

                ctx.progress.report(ProgressEvent::EntryFinished {
                    task_id: ctx.task_id,
                    entry: progress,
                });
            }
        }

        writer.finish().map_err(Self::map_zip_err)?;
        Ok(())
    }
}
