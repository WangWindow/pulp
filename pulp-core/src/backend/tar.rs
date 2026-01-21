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
use bzip2::{Compression as BzCompression, read::BzDecoder, write::BzEncoder};
use flate2::{Compression as GzCompression, read::GzDecoder, write::GzEncoder};
use std::{
    fs::File,
    io::{Read, Write},
    path::{Path, PathBuf},
    time::UNIX_EPOCH,
};
use tar::{Archive, Builder};
use xz2::{read::XzDecoder, write::XzEncoder};

/// TAR 系列后端（tar / tar.gz / tar.bz2 / tar.xz）。
#[derive(Debug, Default, Clone)]
pub struct TarBackend;

impl TarBackend {
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

    fn open_reader(path: &Path, format: ArchiveFormat) -> Result<Box<dyn Read>> {
        let file = File::open(path)?;
        let reader: Box<dyn Read> = match format {
            ArchiveFormat::Tar => Box::new(file),
            ArchiveFormat::TarGz => Box::new(GzDecoder::new(file)),
            ArchiveFormat::TarBz2 => Box::new(BzDecoder::new(file)),
            ArchiveFormat::TarXz => Box::new(XzDecoder::new(file)),
            _ => {
                return Err(PulpError::Unsupported(
                    "Not a tar family format".to_string(),
                ));
            }
        };
        Ok(reader)
    }

    fn create_writer(
        path: &Path,
        format: ArchiveFormat,
        level: Option<u8>,
    ) -> Result<Box<dyn Write>> {
        let file = File::create(path)?;
        let writer: Box<dyn Write> = match format {
            ArchiveFormat::Tar => Box::new(file),
            ArchiveFormat::TarGz => {
                let lvl = level.unwrap_or(6) as u32;
                Box::new(GzEncoder::new(file, GzCompression::new(lvl)))
            }
            ArchiveFormat::TarBz2 => {
                let lvl = level.unwrap_or(6) as u32;
                Box::new(BzEncoder::new(file, BzCompression::new(lvl)))
            }
            ArchiveFormat::TarXz => {
                let lvl = level.unwrap_or(6) as u32;
                Box::new(XzEncoder::new(file, lvl))
            }
            _ => {
                return Err(PulpError::Unsupported(
                    "Not a tar family format".to_string(),
                ));
            }
        };
        Ok(writer)
    }
}

#[async_trait]
impl crate::backend::ArchiveBackend for TarBackend {
    fn name(&self) -> &'static str {
        "tar"
    }

    fn supported_formats(&self) -> &'static [ArchiveFormat] {
        &[
            ArchiveFormat::Tar,
            ArchiveFormat::TarGz,
            ArchiveFormat::TarBz2,
            ArchiveFormat::TarXz,
        ]
    }

    async fn list(
        &self,
        source: &ArchiveSource,
        _options: &ListOptions,
        _ctx: &TaskContext<'_>,
    ) -> Result<Vec<ArchiveEntry>> {
        let format = source
            .format_hint
            .or_else(|| ArchiveFormat::from_path(&source.path))
            .ok_or_else(|| PulpError::Unsupported("Unable to infer tar format".to_string()))?;

        let reader = Self::open_reader(&source.path, format)?;
        let mut archive = Archive::new(reader);
        let mut entries = Vec::new();

        for item in archive.entries()? {
            let entry = item?;
            let path = entry.path()?.to_string_lossy().to_string();
            let header = entry.header();
            let is_dir = header.entry_type().is_dir();
            let size = if is_dir { None } else { Some(header.size()?) };
            let modified = header
                .mtime()
                .ok()
                .map(|t| UNIX_EPOCH + std::time::Duration::from_secs(t));

            entries.push(ArchiveEntry {
                path,
                is_dir,
                size,
                compressed_size: None,
                modified,
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
        let format = source
            .format_hint
            .or_else(|| ArchiveFormat::from_path(&source.path))
            .ok_or_else(|| PulpError::Unsupported("Unable to infer tar format".to_string()))?;

        let reader = Self::open_reader(&source.path, format)?;
        let mut archive = Archive::new(reader);
        let safe_opts = ExtractPathOptions {
            preserve_paths: options.preserve_paths,
            strip_components: options.strip_components,
        };

        for item in archive.entries()? {
            ctx.cancel.throw_if_cancelled()?;
            let mut entry = item?;
            let path = entry.path()?.to_string_lossy().to_string();
            let is_dir = entry.header().entry_type().is_dir();

            let out_path = match build_output_path(dest_dir, &path, &safe_opts) {
                Ok(Some(p)) => p,
                Ok(None) => {
                    ctx.warn(format!("Skipped entry (empty or stripped path): {path}"));
                    continue;
                }
                Err(e) => {
                    ctx.warn(format!("Skipped unsafe path {path}: {e:?}"));
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

            let total = entry.header().size().ok();
            let mut progress = Self::entry_progress(&out_path, &path, total, 0, false);
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
        format: ArchiveFormat,
        options: &CompressOptions,
        ctx: &TaskContext<'_>,
    ) -> Result<()> {
        let writer = Self::create_writer(dest_archive, format, options.level)?;
        let mut builder = Builder::new(writer);

        for input in inputs {
            ctx.cancel.throw_if_cancelled()?;
            let root_name = input
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "root".to_string());

            if input.is_file() {
                let mut f = File::open(input)?;
                let total = f.metadata().ok().map(|m| m.len());
                let mut progress = Self::entry_progress(input, &root_name, total, 0, false);
                ctx.progress.report(ProgressEvent::EntryStarted {
                    task_id: ctx.task_id,
                    entry: progress.clone(),
                });

                builder.append_file(root_name.clone(), &mut f)?;
                progress.processed_bytes = total.unwrap_or(progress.processed_bytes);

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
                let rel_name = if rel.as_os_str().is_empty() {
                    root_name.clone()
                } else {
                    format!("{}/{}", root_name, rel.to_string_lossy())
                };

                if entry.file_type().is_dir() {
                    builder.append_dir(rel_name, path)?;
                    continue;
                }

                let mut f = File::open(path)?;
                let total = f.metadata().ok().map(|m| m.len());
                let mut progress = Self::entry_progress(path, &rel_name, total, 0, false);
                ctx.progress.report(ProgressEvent::EntryStarted {
                    task_id: ctx.task_id,
                    entry: progress.clone(),
                });

                builder.append_file(rel_name, &mut f)?;
                progress.processed_bytes = total.unwrap_or(progress.processed_bytes);

                ctx.progress.report(ProgressEvent::EntryFinished {
                    task_id: ctx.task_id,
                    entry: progress,
                });
            }
        }

        builder.finish()?;
        Ok(())
    }
}
