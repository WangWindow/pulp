use super::{ArchiveBackend, TaskContext};
use crate::{
    domain::{
        ArchiveEntry, ArchiveFormat, ArchiveSource, CompressOptions, ExtractOptions, ListOptions,
    },
    portal::{error::Result, progress::ProgressEvent},
};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Arc,
};

/// 后端注册表：管理多个后端并负责路由选择。
#[derive(Default, Clone)]
pub struct BackendRegistry {
    by_format: HashMap<ArchiveFormat, Vec<Arc<dyn ArchiveBackend>>>,
}

impl BackendRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// 注册一个后端（按注册顺序作为优先级）。
    pub fn register(&mut self, backend: Arc<dyn ArchiveBackend>) {
        for fmt in backend.supported_formats() {
            self.by_format
                .entry(*fmt)
                .or_default()
                .push(backend.clone());
        }
    }

    /// 获取已注册格式列表。
    pub fn registered_formats(&self) -> Vec<ArchiveFormat> {
        let mut v: Vec<ArchiveFormat> = self.by_format.keys().copied().collect();
        v.sort_by_key(|f| f.to_string());
        v
    }

    /// 获取某格式下的后端名称列表。
    pub fn backends_for_format(&self, format: ArchiveFormat) -> Vec<&'static str> {
        self.by_format
            .get(&format)
            .map(|list| list.iter().map(|b| b.name()).collect::<Vec<_>>())
            .unwrap_or_default()
    }

    pub fn pick_for_source(&self, source: &ArchiveSource) -> Result<Arc<dyn ArchiveBackend>> {
        let format = source
            .format_hint
            .or_else(|| ArchiveFormat::from_path(&source.path))
            .ok_or_else(|| {
                crate::PulpError::Unsupported("Unable to infer archive format".to_string())
            })?;

        self.pick_for_format_and_path(format, &source.path)
    }

    pub fn pick_for_compress(
        &self,
        format: ArchiveFormat,
        dest_archive: &Path,
    ) -> Result<Arc<dyn ArchiveBackend>> {
        self.pick_for_format_and_path(format, dest_archive)
    }

    fn pick_for_format_and_path(
        &self,
        format: ArchiveFormat,
        archive_path: &Path,
    ) -> Result<Arc<dyn ArchiveBackend>> {
        let list: &Vec<Arc<dyn ArchiveBackend>> = self.by_format.get(&format).ok_or_else(|| {
            crate::PulpError::Unsupported(format!("No backend registered for format: {format}"))
        })?;

        for backend in list {
            if backend.can_handle_path(archive_path) {
                return Ok(backend.clone());
            }
        }

        list.first()
            .cloned()
            .ok_or_else(|| crate::PulpError::Unsupported("No backend registered".to_string()))
    }

    pub async fn list(
        &self,
        source: &ArchiveSource,
        options: &ListOptions,
        ctx: &TaskContext<'_>,
    ) -> Result<Vec<ArchiveEntry>> {
        let backend = self.pick_for_source(source)?;

        ctx.progress.report(ProgressEvent::BackendSelected {
            task_id: ctx.task_id,
            backend: backend.name().to_string(),
        });

        backend.list(source, options, ctx).await
    }

    pub async fn extract(
        &self,
        source: &ArchiveSource,
        dest_dir: &Path,
        options: &ExtractOptions,
        ctx: &TaskContext<'_>,
    ) -> Result<()> {
        let backend = self.pick_for_source(source)?;

        ctx.progress.report(ProgressEvent::BackendSelected {
            task_id: ctx.task_id,
            backend: backend.name().to_string(),
        });

        backend.extract(source, dest_dir, options, ctx).await
    }

    pub async fn compress(
        &self,
        inputs: &[PathBuf],
        dest_archive: &Path,
        format: ArchiveFormat,
        options: &CompressOptions,
        ctx: &TaskContext<'_>,
    ) -> Result<()> {
        let backend = self.pick_for_compress(format, dest_archive)?;

        ctx.progress.report(ProgressEvent::BackendSelected {
            task_id: ctx.task_id,
            backend: backend.name().to_string(),
        });

        backend
            .compress(inputs, dest_archive, format, options, ctx)
            .await
    }
}

/// 创建默认 registry（纯 Rust 优先）。
pub fn create_default_registry() -> BackendRegistry {
    let mut reg = BackendRegistry::new();

    reg.register(Arc::new(super::sevenz::SevenzBackend::default()));
    reg.register(Arc::new(super::zip::ZipBackend::default()));
    reg.register(Arc::new(super::tar::TarBackend::default()));
    reg.register(Arc::new(super::rar::RarBackend::default()));

    reg
}
