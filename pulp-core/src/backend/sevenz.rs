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
use sevenz_rust2::{ArchiveReader, ArchiveWriter, Password};
use std::{
    fs::File,
    io::Write,
    path::{Path, PathBuf},
};

/// 7z 后端（纯 Rust）。
#[derive(Debug, Default, Clone)]
pub struct SevenzBackend;

impl SevenzBackend {
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

    fn password_from_option(pw: &Option<String>) -> Password {
        match pw {
            Some(v) if !v.is_empty() => Password::from(v.as_str()),
            _ => Password::empty(),
        }
    }
}

#[async_trait]
impl crate::backend::ArchiveBackend for SevenzBackend {
    fn name(&self) -> &'static str {
        "sevenz"
    }

    fn supported_formats(&self) -> &'static [ArchiveFormat] {
        &[ArchiveFormat::SevenZ]
    }

    async fn list(
        &self,
        source: &ArchiveSource,
        options: &ListOptions,
        _ctx: &TaskContext<'_>,
    ) -> Result<Vec<ArchiveEntry>> {
        let password = Self::password_from_option(&options.password);
        let reader = ArchiveReader::open(&source.path, password)
            .map_err(|e| PulpError::backend("7z", e.to_string()))?;

        let archive = reader.archive();
        let mut entries = Vec::with_capacity(archive.files.len());
        for f in &archive.files {
            entries.push(ArchiveEntry {
                path: f.name().to_string(),
                is_dir: f.is_directory,
                size: if f.is_directory { None } else { Some(f.size) },
                compressed_size: if f.is_directory {
                    None
                } else {
                    Some(f.compressed_size)
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
        let password = Self::password_from_option(&options.password);
        let mut reader = ArchiveReader::open(&source.path, password)
            .map_err(|e| PulpError::backend("7z", e.to_string()))?;

        if let Some(threads) = options.threads {
            reader.set_thread_count(threads as u32);
        }

        let safe_opts = ExtractPathOptions {
            preserve_paths: options.preserve_paths,
            strip_components: options.strip_components,
        };

        reader
            .for_each_entries(|entry, entry_reader| {
                if ctx.cancel.is_cancelled() {
                    return Ok(false);
                }
                let name = entry.name().to_string();
                let is_dir = entry.is_directory;

                let out_path = match build_output_path(dest_dir, &name, &safe_opts) {
                    Ok(Some(p)) => p,
                    Ok(None) => {
                        ctx.warn(format!("Skipped entry (empty or stripped path): {name}"));
                        return Ok(true);
                    }
                    Err(e) => {
                        ctx.warn(format!("Skipped unsafe path {name}: {e:?}"));
                        return Ok(true);
                    }
                };

                if is_dir {
                    std::fs::create_dir_all(&out_path)?;
                    return Ok(true);
                }

                if out_path.exists() && !options.overwrite {
                    ctx.warn(format!("Target exists, skipped: {}", out_path.display()));
                    return Ok(true);
                }

                if let Some(parent) = out_path.parent() {
                    std::fs::create_dir_all(parent)?;
                }

                let total = Some(entry.size);
                let mut progress = Self::entry_progress(&out_path, &name, total, 0, false);
                ctx.progress.report(ProgressEvent::EntryStarted {
                    task_id: ctx.task_id,
                    entry: progress.clone(),
                });

                let mut outfile = File::create(&out_path)?;
                let mut buf = [0u8; 64 * 1024];
                loop {
                    let n = entry_reader.read(&mut buf)?;
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

                Ok(true)
            })
            .map_err(|e| PulpError::backend("7z", e.to_string()))?;

        if ctx.cancel.is_cancelled() {
            return Err(PulpError::Cancelled);
        }

        Ok(())
    }

    async fn compress(
        &self,
        inputs: &[PathBuf],
        dest_archive: &Path,
        _format: ArchiveFormat,
        options: &CompressOptions,
        ctx: &TaskContext<'_>,
    ) -> Result<()> {
        if options.password.is_some() {
            return Err(PulpError::Unsupported(
                "7z encryption is not implemented yet".to_string(),
            ));
        }

        let mut writer = ArchiveWriter::create(dest_archive)
            .map_err(|e| PulpError::backend("7z", e.to_string()))?;

        for input in inputs {
            ctx.cancel.throw_if_cancelled()?;

            let name = input
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "root".to_string());

            let progress = Self::entry_progress(input, &name, None, 0, input.is_dir());
            ctx.progress.report(ProgressEvent::EntryStarted {
                task_id: ctx.task_id,
                entry: progress.clone(),
            });

            let res = match options.solid {
                Some(false) => writer.push_source_path_non_solid(input, |_| true),
                _ => writer.push_source_path(input, |_| true),
            };

            res.map_err(|e| PulpError::backend("7z", e.to_string()))?;

            ctx.progress.report(ProgressEvent::EntryFinished {
                task_id: ctx.task_id,
                entry: progress,
            });
        }

        writer
            .finish()
            .map_err(|e| PulpError::backend("7z", e.to_string()))?;
        Ok(())
    }
}
