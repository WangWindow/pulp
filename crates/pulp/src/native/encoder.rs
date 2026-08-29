//! Format7zF update callbacks used to create and rewrite archives.

use super::ArchiveEngine;
use super::callbacks::SourceState;
use super::ffi;
use super::loader;
use super::streams::OutputCallbacks;
use crate::{
    ArchiveError, ArchiveResult, CreateOptions, EntryKind, EntrySource, FormatDescriptor,
    OperationContext, OperationKind, OperationReport, UpdateOptions, WriteSeek,
};

pub fn create(
    engine: &ArchiveEngine,
    descriptor: &FormatDescriptor,
    source: &mut dyn EntrySource,
    output: &mut dyn WriteSeek,
    options: &CreateOptions,
    context: &OperationContext,
) -> ArchiveResult<OperationReport> {
    run_update(
        engine,
        descriptor,
        source,
        output,
        options,
        OperationKind::Create,
        context,
    )
}

pub fn update(
    engine: &ArchiveEngine,
    descriptor: &FormatDescriptor,
    source: &mut dyn EntrySource,
    output: &mut dyn WriteSeek,
    options: &UpdateOptions,
    context: &OperationContext,
) -> ArchiveResult<OperationReport> {
    if options.keep_unmentioned {
        return Err(ArchiveError::unsupported(
            "preserving unmentioned archive entries requires an existing-archive update adapter",
        ));
    }
    run_update(
        engine,
        descriptor,
        source,
        output,
        &options.create,
        OperationKind::Update,
        context,
    )
}

fn run_update(
    engine: &ArchiveEngine,
    descriptor: &FormatDescriptor,
    source: &mut dyn EntrySource,
    output: &mut dyn crate::WriteSeek,
    options: &CreateOptions,
    operation: OperationKind,
    context: &OperationContext,
) -> ArchiveResult<OperationReport> {
    context.check_cancelled()?;
    if options.volume_size.is_some() {
        return Err(ArchiveError::unsupported(
            "multi-volume output is not implemented by the Format7zF Rust bridge",
        ));
    }
    let entries = collect_entries(source, context)?;
    let count = u32::try_from(entries.len())
        .map_err(|_| ArchiveError::invalid_input("source contains too many entries"))?;
    let class_id = descriptor.class_id.ok_or_else(|| {
        ArchiveError::Internal(format!(
            "handler '{}' has no class identifier",
            descriptor.name
        ))
    })?;
    let mut output_callbacks = OutputCallbacks::new(output);
    let mut source_state = SourceState::new(
        source,
        entries,
        context,
        operation,
        options.password.clone(),
    );
    let source_callbacks = source_state.callbacks();
    debug_assert_eq!(source_callbacks.count, count);
    let method = options
        .compression_method
        .as_deref()
        .map(str::as_bytes)
        .unwrap_or_default();
    let method_len = u32::try_from(method.len())
        .map_err(|_| ArchiveError::invalid_input("compression method name is too long"))?;
    let level = options
        .compression_level
        .map(|value| i32::try_from(value).unwrap_or(i32::MAX))
        .unwrap_or(-1);
    let solid = options.solid.map(i32::from).unwrap_or(-1);
    let native_options = ffi::Pulp7zUpdateOptions {
        method: if method.is_empty() {
            std::ptr::null()
        } else {
            method.as_ptr().cast()
        },
        method_len,
        level,
        solid,
        header_encryption: if options.header_encryption { 1 } else { -1 },
    };
    context.report(crate::ProgressEvent::Started {
        operation,
        total_bytes: None,
    });
    let mut native_error = ffi::Pulp7zError::default();
    let status = {
        let runtime = engine
            .runtime
            .lock()
            .map_err(|_| ArchiveError::Internal("native provider mutex is poisoned".to_owned()))?;
        unsafe {
            ffi::pulp7z_bridge_update(
                runtime.bridge(),
                class_id.as_ptr(),
                output_callbacks.raw(),
                &source_callbacks,
                &native_options,
                &mut native_error,
            )
        }
    };
    let output_error = output_callbacks.take_error();
    let output_bytes = output_callbacks.bytes();
    let source_result = source_state.finish();
    if let Some(error) = output_error {
        return Err(error);
    }
    let mut report = source_result?;
    if status != ffi::PULP7Z_OK {
        return Err(loader::native_status_error(
            status,
            &native_error,
            "Format7zF archive creation failed",
        ));
    }
    output.flush()?;
    report.format = Some(descriptor.id.clone());
    report.bytes_written = output_bytes;
    context.report(crate::ProgressEvent::Finished(report.clone()));
    Ok(report)
}

fn collect_entries(
    source: &mut dyn EntrySource,
    context: &OperationContext,
) -> ArchiveResult<Vec<crate::ArchiveEntry>> {
    let mut entries = Vec::new();
    let mut total_bytes = 0_u64;
    loop {
        context.check_cancelled()?;
        let Some(entry) = source.next(context)? else {
            break;
        };
        if matches!(
            entry.kind,
            EntryKind::Symlink | EntryKind::Hardlink | EntryKind::Special
        ) {
            return Err(ArchiveError::invalid_input(format!(
                "Format7zF creation does not yet encode link or special entry '{}'",
                entry.name
            )));
        }
        if entries.len() as u64 >= context.limits().max_entries {
            return Err(ArchiveError::ResourceLimit {
                kind: crate::ResourceLimitKind::Entries,
                message: "source entry count exceeds configured limit".to_owned(),
            });
        }
        context.validate_entry(&entry)?;
        if entry.kind == EntryKind::File && entry.size.is_none() {
            return Err(ArchiveError::invalid_input(format!(
                "regular file entry '{}' must provide its size",
                entry.name
            )));
        }
        total_bytes = total_bytes
            .checked_add(entry.size.unwrap_or(0))
            .ok_or_else(|| ArchiveError::ResourceLimit {
                kind: crate::ResourceLimitKind::TotalBytes,
                message: "source byte count overflowed".to_owned(),
            })?;
        if total_bytes > context.limits().max_total_bytes {
            return Err(ArchiveError::ResourceLimit {
                kind: crate::ResourceLimitKind::TotalBytes,
                message: "source byte count exceeds configured limit".to_owned(),
            });
        }
        if entries.len() >= u32::MAX as usize {
            return Err(ArchiveError::invalid_input(
                "source contains too many entries",
            ));
        }
        entries.push(entry);
    }
    Ok(entries)
}
