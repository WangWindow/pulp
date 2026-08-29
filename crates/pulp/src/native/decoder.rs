//! Format7zF engine implementation for listing, extraction and verification.

use std::io::{self, Seek, SeekFrom, Write};
use std::sync::Mutex;

use crate::{
    ArchiveEntry, ArchiveError, ArchiveFormatId, CreateOptions, DetectionHint, DetectionMethod,
    DetectionResult, EngineInfo, EntryId, EntryKind, EntrySink, EntrySinkDecision, EntrySource,
    EntryVisitor, ExtractOptions, FormatCapability, FormatDescriptor, FormatDetector,
    IgnoreEntryVisitor, LicenseNotice, OperationContext, OperationKind, OperationReport, ReadSeek,
    TestOptions, UpdateOptions, WriteSeek,
};

use super::ArchiveEngine;
use super::callbacks::{self, ExtractState, ListState};
use super::encoder;
use super::error::Format7zError;
use super::ffi;
use super::loader::{self, NativeRuntime};
use super::metadata::{self, RuntimeMetadata};
use super::streams::{self, InputCallbacks};
use super::temp::TemporaryArchive;

impl ArchiveEngine {
    /// Loads the provider embedded in this crate.
    pub fn load() -> Result<Self, Format7zError> {
        let runtime = loader::open()?;
        let metadata = metadata::collect(&runtime)?;
        Self::from_runtime(runtime, metadata)
    }

    fn from_runtime(
        runtime: NativeRuntime,
        metadata: RuntimeMetadata,
    ) -> Result<Self, Format7zError> {
        if metadata.formats.is_empty() {
            return Err(Format7zError::Metadata(
                "Format7zF did not expose any archive handlers".to_owned(),
            ));
        }
        let info = EngineInfo {
            name: "Format7zF".to_owned(),
            version: None,
            library_path: None,
            license: LicenseNotice::new(
                "7zip-lgpl",
                "7-Zip components are distributed under the LGPL-2.1-or-later license.",
                "https://www.7-zip.org/license.txt",
            ),
            diagnostics: vec![
                "handler and method capabilities were queried from the embedded provider"
                    .to_owned(),
                "the native provider is statically linked through the Pulp C ABI bridge".to_owned(),
            ],
        };
        Ok(Self {
            runtime: Mutex::new(runtime),
            info,
            formats: metadata.formats,
            methods: metadata.methods,
        })
    }

    /// Returns all compression methods reported by the loaded provider.
    #[must_use]
    pub fn methods(&self) -> &[crate::CompressionMethod] {
        &self.methods
    }

    /// Finds a handler by its provider identifier.
    #[must_use]
    pub fn format(&self, id: &ArchiveFormatId) -> Option<&FormatDescriptor> {
        self.formats.iter().find(|format| &format.id == id)
    }

    /// Lists an archive with an explicitly selected starting handler.
    pub fn list_format(
        &self,
        format: &ArchiveFormatId,
        input: &mut dyn ReadSeek,
        visitor: &mut dyn EntryVisitor,
        context: &OperationContext,
    ) -> crate::ArchiveResult<OperationReport> {
        let report = self.list_chain_from(format.clone(), input, context, visitor, 0)?;
        self.finish_report(report, context)
    }

    /// Extracts an archive with an explicitly selected starting handler.
    pub fn extract_format(
        &self,
        format: &ArchiveFormatId,
        input: &mut dyn ReadSeek,
        sink: &mut dyn EntrySink,
        options: &ExtractOptions,
        context: &OperationContext,
    ) -> crate::ArchiveResult<OperationReport> {
        let report = self.extract_chain_from(format.clone(), input, context, sink, options, 0)?;
        self.finish_report(report, context)
    }

    /// Tests an archive with an explicitly selected starting handler.
    pub fn test_format(
        &self,
        format: &ArchiveFormatId,
        input: &mut dyn ReadSeek,
        options: &TestOptions,
        context: &OperationContext,
    ) -> crate::ArchiveResult<OperationReport> {
        let report = self.test_chain_from(format.clone(), input, context, options, 0)?;
        self.finish_report(report, context)
    }

    /// Returns engine identity and native build diagnostics.
    #[must_use]
    pub fn info(&self) -> &EngineInfo {
        &self.info
    }

    /// Returns every archive handler exposed by the embedded provider.
    #[must_use]
    pub fn formats(&self) -> &[FormatDescriptor] {
        &self.formats
    }

    /// Confirms an archive format using signatures, the optional hint, and a
    /// real Format7zF open probe.
    pub fn detect(
        &self,
        input: &mut dyn ReadSeek,
        hint: Option<&DetectionHint>,
        context: &OperationContext,
    ) -> crate::ArchiveResult<DetectionResult> {
        self.detect_provider(input, hint, context)
    }

    /// Lists an archive after detecting its handler.
    pub fn list(
        &self,
        input: &mut dyn ReadSeek,
        context: &OperationContext,
        visitor: &mut dyn EntryVisitor,
    ) -> crate::ArchiveResult<OperationReport> {
        let report = self.list_chain(input, context, visitor, 0)?;
        self.finish_report(report, context)
    }

    /// Extracts an archive after detecting its handler.
    pub fn extract(
        &self,
        input: &mut dyn ReadSeek,
        context: &OperationContext,
        sink: &mut dyn EntrySink,
        options: &ExtractOptions,
    ) -> crate::ArchiveResult<OperationReport> {
        let report = self.extract_chain(input, context, sink, options, 0)?;
        self.finish_report(report, context)
    }

    /// Tests an archive after detecting its handler.
    pub fn test(
        &self,
        input: &mut dyn ReadSeek,
        context: &OperationContext,
        options: &TestOptions,
    ) -> crate::ArchiveResult<OperationReport> {
        let report = self.test_chain(input, context, options, 0)?;
        self.finish_report(report, context)
    }

    /// Creates a new archive using the selected writable handler.
    pub fn create(
        &self,
        format: &ArchiveFormatId,
        source: &mut dyn EntrySource,
        output: &mut dyn WriteSeek,
        options: &CreateOptions,
        context: &OperationContext,
    ) -> crate::ArchiveResult<OperationReport> {
        let descriptor = self.require_format(format, OperationKind::Create)?;
        encoder::create(self, descriptor, source, output, options, context)
    }

    /// Updates an archive using the selected writable handler.
    pub fn update(
        &self,
        format: &ArchiveFormatId,
        source: &mut dyn EntrySource,
        output: &mut dyn WriteSeek,
        options: &UpdateOptions,
        context: &OperationContext,
    ) -> crate::ArchiveResult<OperationReport> {
        let descriptor = self.require_format(format, OperationKind::Update)?;
        encoder::update(self, descriptor, source, output, options, context)
    }

    fn list_chain(
        &self,
        input: &mut dyn ReadSeek,
        context: &OperationContext,
        visitor: &mut dyn EntryVisitor,
        depth: usize,
    ) -> crate::ArchiveResult<OperationReport> {
        ensure_depth(context, depth)?;
        let format = self.current_format(input, context)?;
        self.list_chain_from(format, input, context, visitor, depth)
    }

    fn list_chain_from(
        &self,
        format: ArchiveFormatId,
        input: &mut dyn ReadSeek,
        context: &OperationContext,
        visitor: &mut dyn EntryVisitor,
        depth: usize,
    ) -> crate::ArchiveResult<OperationReport> {
        ensure_depth(context, depth)?;
        context.check_cancelled()?;
        let position = input.stream_position()?;
        let internal_context = context.without_progress();
        let (entries, report) = self.list_layer(&format, input, &internal_context)?;

        if let Some((inner, mut child)) =
            self.open_inner_archive(&format, input, position, &entries, context)?
        {
            return self.list_chain_from(inner, child.file_mut(), context, visitor, depth + 1);
        }

        for entry in &entries {
            context.report(crate::ProgressEvent::EntryStarted {
                id: entry.id,
                name: entry.name.clone(),
                size: entry.size,
            });
            visitor.visit(entry)?;
        }
        Ok(report)
    }

    fn extract_chain(
        &self,
        input: &mut dyn ReadSeek,
        context: &OperationContext,
        sink: &mut dyn EntrySink,
        options: &ExtractOptions,
        depth: usize,
    ) -> crate::ArchiveResult<OperationReport> {
        ensure_depth(context, depth)?;
        let format = self.current_format(input, context)?;
        self.extract_chain_from(format, input, context, sink, options, depth)
    }

    fn extract_chain_from(
        &self,
        format: ArchiveFormatId,
        input: &mut dyn ReadSeek,
        context: &OperationContext,
        sink: &mut dyn EntrySink,
        options: &ExtractOptions,
        depth: usize,
    ) -> crate::ArchiveResult<OperationReport> {
        ensure_depth(context, depth)?;
        context.check_cancelled()?;
        let position = input.stream_position()?;
        let descriptor = self.require_format(&format, OperationKind::Extract)?;
        if descriptor.supports(FormatCapability::Transparent) {
            let internal_context = context.without_progress();
            input.seek(SeekFrom::Start(position))?;
            let (entries, _) = self.list_layer(&format, input, &internal_context)?;
            if let Some((inner, mut child)) =
                self.open_inner_archive(&format, input, position, &entries, context)?
            {
                return self.extract_chain_from(
                    inner,
                    child.file_mut(),
                    context,
                    sink,
                    options,
                    depth + 1,
                );
            }
        }

        input.seek(SeekFrom::Start(position))?;
        let report = self.native_extract(descriptor, input, sink, options, false, context)?;
        Ok(report.with_format(descriptor.id.clone()))
    }

    fn test_chain(
        &self,
        input: &mut dyn ReadSeek,
        context: &OperationContext,
        options: &TestOptions,
        depth: usize,
    ) -> crate::ArchiveResult<OperationReport> {
        ensure_depth(context, depth)?;
        let format = self.current_format(input, context)?;
        self.test_chain_from(format, input, context, options, depth)
    }

    fn test_chain_from(
        &self,
        format: ArchiveFormatId,
        input: &mut dyn ReadSeek,
        context: &OperationContext,
        options: &TestOptions,
        depth: usize,
    ) -> crate::ArchiveResult<OperationReport> {
        ensure_depth(context, depth)?;
        context.check_cancelled()?;
        let position = input.stream_position()?;
        let descriptor = self.require_format(&format, OperationKind::Test)?;
        if descriptor.supports(FormatCapability::Transparent) {
            let internal_context = context.without_progress();
            input.seek(SeekFrom::Start(position))?;
            let (entries, _) = self.list_layer(&format, input, &internal_context)?;
            if let Some((inner, mut child)) =
                self.open_inner_archive(&format, input, position, &entries, context)?
            {
                return self.test_chain_from(inner, child.file_mut(), context, options, depth + 1);
            }
        }

        input.seek(SeekFrom::Start(position))?;
        let mut sink = crate::MetadataOnlySink;
        let report = self.native_extract(
            descriptor,
            input,
            &mut sink,
            &ExtractOptions::default(),
            options.verify_data,
            context,
        )?;
        Ok(report.with_format(descriptor.id.clone()))
    }

    fn current_format(
        &self,
        input: &mut dyn ReadSeek,
        context: &OperationContext,
    ) -> crate::ArchiveResult<ArchiveFormatId> {
        match self.detect_provider(input, None, context)? {
            DetectionResult::Detected(candidate) => Ok(candidate.descriptor.id),
            DetectionResult::Ambiguous(_) => Err(ArchiveError::UnsupportedFormat(
                "multiple Format7zF handlers opened the archive".to_owned(),
            )),
            DetectionResult::Unknown => Err(ArchiveError::UnsupportedFormat(
                "no Format7zF handler could open the archive".to_owned(),
            )),
        }
    }

    fn inner_format(
        &self,
        input: &mut dyn ReadSeek,
        context: &OperationContext,
    ) -> crate::ArchiveResult<Option<ArchiveFormatId>> {
        Ok(match self.detect_provider(input, None, context)? {
            DetectionResult::Detected(candidate) => Some(candidate.descriptor.id),
            DetectionResult::Ambiguous(_) | DetectionResult::Unknown => None,
        })
    }

    fn list_layer(
        &self,
        format: &ArchiveFormatId,
        input: &mut dyn ReadSeek,
        context: &OperationContext,
    ) -> crate::ArchiveResult<(Vec<ArchiveEntry>, OperationReport)> {
        let descriptor = self.require_format(format, OperationKind::List)?;
        let mut collector = LayerEntries::default();
        let report = self.native_list(descriptor, input, &mut collector, context)?;
        Ok((collector.entries, report.with_format(descriptor.id.clone())))
    }

    fn open_inner_archive(
        &self,
        format: &ArchiveFormatId,
        input: &mut dyn ReadSeek,
        position: u64,
        entries: &[ArchiveEntry],
        context: &OperationContext,
    ) -> crate::ArchiveResult<Option<(ArchiveFormatId, TemporaryArchive)>> {
        let descriptor = self.require_format(format, OperationKind::List)?;
        if !descriptor.supports(FormatCapability::Extract) {
            return Ok(None);
        }
        let Some(content) = transparent_content(Some(descriptor), entries) else {
            return Ok(None);
        };

        let internal_context = context.without_progress();
        input.seek(SeekFrom::Start(position))?;
        let mut child = TemporaryArchive::create()?;
        self.copy_entry(
            format,
            input,
            content.id,
            child.file_mut(),
            &internal_context,
        )?;
        child.rewind()?;
        let Some(inner) = self.inner_format(child.file_mut(), &internal_context)? else {
            return Ok(None);
        };
        Ok(Some((inner, child)))
    }

    fn copy_entry(
        &self,
        format: &ArchiveFormatId,
        input: &mut dyn ReadSeek,
        entry: EntryId,
        output: &mut dyn WriteSeek,
        context: &OperationContext,
    ) -> crate::ArchiveResult<()> {
        let position = input.stream_position()?;
        let descriptor = self.require_format(format, OperationKind::Extract)?;
        let index = u32::try_from(entry.get()).map_err(|_| {
            ArchiveError::invalid_input("entry identifier exceeds native index range")
        })?;
        if self.copy_entry_stream(descriptor, input, index, output, context)? {
            return Ok(());
        }

        input.seek(SeekFrom::Start(position))?;
        let mut sink = SingleEntrySink::new(index, output);
        self.native_extract(
            descriptor,
            input,
            &mut sink,
            &ExtractOptions {
                selected: Some(vec![entry]),
                ..ExtractOptions::default()
            },
            false,
            context,
        )?;
        if !sink.completed {
            return Err(ArchiveError::DataError(
                "Format7z did not produce the requested entry stream".to_owned(),
            ));
        }
        Ok(())
    }

    fn copy_entry_stream(
        &self,
        descriptor: &FormatDescriptor,
        input: &mut dyn ReadSeek,
        index: u32,
        output: &mut dyn WriteSeek,
        context: &OperationContext,
    ) -> crate::ArchiveResult<bool> {
        let class_id = descriptor.class_id.ok_or_else(|| {
            ArchiveError::Internal(format!(
                "handler '{}' has no class identifier",
                descriptor.name
            ))
        })?;
        context.check_cancelled()?;
        let mut input_callbacks = InputCallbacks::new(input);
        let limit = context
            .limits()
            .max_entry_bytes
            .min(context.limits().max_total_bytes);
        let mut limited_output = LimitedOutput::new(output, limit);
        let mut output_callbacks = streams::OutputCallbacks::new(&mut limited_output);
        let mut ignored = IgnoreEntryVisitor;
        let mut state = ListState::new(&mut ignored, context);
        let open_callbacks = state.open_callbacks();
        let mut native_error = ffi::Pulp7zError::default();
        let status = {
            let runtime = self.runtime.lock().map_err(|_| {
                ArchiveError::Internal("native provider mutex is poisoned".to_owned())
            })?;
            unsafe {
                ffi::pulp7z_bridge_copy_entry(
                    runtime.bridge(),
                    class_id.as_ptr(),
                    input_callbacks.raw(),
                    &open_callbacks,
                    index,
                    output_callbacks.raw(),
                    &mut native_error,
                )
            }
        };
        let input_error = input_callbacks.take_error();
        let output_error = output_callbacks.take_error();
        drop(output_callbacks);
        let output_limit_hit = limited_output.exceeded;
        let state_result = state.finish();

        if let Some(error) = input_error {
            return Err(error);
        }
        if output_limit_hit {
            return Err(ArchiveError::ResourceLimit {
                kind: crate::ResourceLimitKind::EntryBytes,
                message: format!(
                    "nested archive stream exceeds the configured limit of {limit} bytes"
                ),
            });
        }
        if let Some(error) = output_error {
            return Err(error);
        }
        state_result?;
        if status == ffi::PULP7Z_STREAM_UNAVAILABLE {
            return Ok(false);
        }
        if status != ffi::PULP7Z_OK {
            return Err(loader::native_status_error(
                status,
                &native_error,
                "Format7z entry stream copy failed",
            ));
        }
        Ok(true)
    }

    fn require_format(
        &self,
        format: &ArchiveFormatId,
        operation: OperationKind,
    ) -> crate::ArchiveResult<&FormatDescriptor> {
        let Some(descriptor) = self.format(format) else {
            return Err(ArchiveError::UnsupportedFormat(format.to_string()));
        };
        if !descriptor.supports(capability_for(operation)) {
            return Err(ArchiveError::UnsupportedOperation {
                operation,
                format: Some(descriptor.name.clone()),
            });
        }
        Ok(descriptor)
    }

    fn detect_provider(
        &self,
        input: &mut dyn crate::ReadSeek,
        hint: Option<&DetectionHint>,
        context: &OperationContext,
    ) -> crate::ArchiveResult<DetectionResult> {
        let position = input.stream_position()?;
        let detector = FormatDetector::new(&self.formats);

        // Use the provider's own bounded signatures to prioritize the probe
        // order. This avoids opening every handler for the common case while
        // retaining the complete fallback loop for formats with weak or
        // missing signatures.
        let signature_result = detector.detect(input, None);
        let restore = input.seek(SeekFrom::Start(position));
        let signature_result = signature_result?;
        restore?;

        let signature_ids = detection_ids(signature_result);
        let hint_ids = hint
            .map(|value| detection_ids(detector.detect_hint(value)))
            .unwrap_or_default();
        let mut tested = vec![false; self.formats.len()];
        let mut opened = Vec::new();

        for ids in [&signature_ids, &hint_ids] {
            let indices = untested_indices(&self.formats, ids, &mut tested);
            if indices.is_empty() {
                continue;
            }
            let result = self.probe_handlers(input, position, &indices, context);
            let restore = input.seek(SeekFrom::Start(position));
            let result = result?;
            restore?;
            if !result.is_empty() {
                opened = result;
                break;
            }
        }

        if opened.is_empty() {
            let indices = self
                .formats
                .iter()
                .enumerate()
                .filter_map(|(index, _)| (!tested[index]).then_some(index))
                .collect::<Vec<_>>();
            let result = self.probe_handlers(input, position, &indices, context);
            let restore = input.seek(SeekFrom::Start(position));
            opened = result?;
            restore?;
        }

        let result = match opened.len() {
            0 => DetectionResult::Unknown,
            1 => DetectionResult::Detected(Box::new(opened.remove(0))),
            _ => DetectionResult::Ambiguous(opened),
        };
        if let (Some(filename_extension), DetectionResult::Detected(candidate)) = (
            hint.and_then(|value| value.filename_extension.as_deref()),
            &result,
        ) && hint
            .and_then(|value| value.extension.as_deref())
            .is_some_and(|extension| !extension.eq_ignore_ascii_case(filename_extension))
            && !candidate.descriptor.matches_extension(filename_extension)
        {
            context.report(crate::ProgressEvent::Warning(format!(
                "filename extension '.{filename_extension}' does not match the detected {} handler",
                candidate.descriptor.name
            )));
        }
        Ok(result)
    }

    fn probe_handlers(
        &self,
        input: &mut dyn crate::ReadSeek,
        position: u64,
        indices: &[usize],
        context: &OperationContext,
    ) -> crate::ArchiveResult<Vec<crate::DetectionCandidate>> {
        let mut opened = Vec::new();
        for &index in indices {
            context.check_cancelled()?;
            let descriptor = &self.formats[index];
            let Some(class_id) = descriptor.class_id else {
                continue;
            };
            input.seek(SeekFrom::Start(position))?;
            let mut callbacks = streams::InputCallbacks::new(input);
            let mut ignored = IgnoreEntryVisitor;
            let mut state = ListState::new(&mut ignored, context);
            let open_callbacks = state.volume_only_callbacks();
            let mut native_error = ffi::Pulp7zError::default();
            let status = {
                let runtime = self.runtime.lock().map_err(|_| {
                    ArchiveError::Internal("native provider mutex is poisoned".to_owned())
                })?;
                unsafe {
                    ffi::pulp7z_bridge_probe(
                        runtime.bridge(),
                        class_id.as_ptr(),
                        callbacks.raw(),
                        &open_callbacks,
                        &mut native_error,
                    )
                }
            };
            let state_result = state.finish();
            if let Some(error) = callbacks.take_error() {
                return Err(error);
            }
            if matches!(status, ffi::PULP7Z_OK | ffi::PULP7Z_PASSWORD_DECLINED)
                && state_result.is_ok()
            {
                opened.push(crate::DetectionCandidate {
                    descriptor: descriptor.clone(),
                    method: DetectionMethod::Provider,
                });
            }
        }
        Ok(opened)
    }

    fn native_list(
        &self,
        descriptor: &FormatDescriptor,
        input: &mut dyn crate::ReadSeek,
        visitor: &mut dyn EntryVisitor,
        context: &OperationContext,
    ) -> crate::ArchiveResult<OperationReport> {
        let class_id = descriptor.class_id.ok_or_else(|| {
            ArchiveError::Internal(format!(
                "handler '{}' has no class identifier",
                descriptor.name
            ))
        })?;
        context.check_cancelled()?;
        context.report(crate::ProgressEvent::Started {
            operation: OperationKind::List,
            total_bytes: None,
        });
        let mut input_callbacks = InputCallbacks::new(input);
        let mut state = ListState::new(visitor, context);
        let open_callbacks = state.open_callbacks();
        let mut native_error = ffi::Pulp7zError::default();
        let status = {
            let runtime = self.runtime.lock().map_err(|_| {
                ArchiveError::Internal("native provider mutex is poisoned".to_owned())
            })?;
            unsafe {
                ffi::pulp7z_bridge_list(
                    runtime.bridge(),
                    class_id.as_ptr(),
                    input_callbacks.raw(),
                    &open_callbacks,
                    callbacks::list_entry_callback(),
                    (&mut state as *mut ListState<'_>).cast(),
                    &mut native_error,
                )
            }
        };
        let input_error = input_callbacks.take_error();
        if let Some(error) = input_error {
            return Err(error);
        }
        let state_report = state.finish()?;
        if status != ffi::PULP7Z_OK {
            return Err(loader::native_status_error(
                status,
                &native_error,
                "Format7zF listing failed",
            ));
        }
        Ok(state_report)
    }

    fn native_extract(
        &self,
        descriptor: &FormatDescriptor,
        input: &mut dyn crate::ReadSeek,
        sink: &mut dyn EntrySink,
        options: &ExtractOptions,
        test_mode: bool,
        context: &OperationContext,
    ) -> crate::ArchiveResult<OperationReport> {
        let class_id = descriptor.class_id.ok_or_else(|| {
            ArchiveError::Internal(format!(
                "handler '{}' has no class identifier",
                descriptor.name
            ))
        })?;
        context.check_cancelled()?;
        context.report(crate::ProgressEvent::Started {
            operation: if test_mode {
                OperationKind::Test
            } else {
                OperationKind::Extract
            },
            total_bytes: None,
        });
        let indices = selected_indices(options)?;
        let index_count = u32::try_from(indices.len())
            .map_err(|_| ArchiveError::invalid_input("too many selected entries"))?;
        let mut input_callbacks = InputCallbacks::new(input);
        let mut state = ExtractState::new(sink, context, test_mode);
        let callbacks = state.callbacks();
        let mut native_error = ffi::Pulp7zError::default();
        let status = {
            let runtime = self.runtime.lock().map_err(|_| {
                ArchiveError::Internal("native provider mutex is poisoned".to_owned())
            })?;
            unsafe {
                ffi::pulp7z_bridge_extract(
                    runtime.bridge(),
                    class_id.as_ptr(),
                    input_callbacks.raw(),
                    if indices.is_empty() {
                        std::ptr::null()
                    } else {
                        indices.as_ptr()
                    },
                    index_count,
                    i32::from(test_mode),
                    &callbacks,
                    &mut native_error,
                )
            }
        };
        let input_error = input_callbacks.take_error();
        if let Some(error) = input_error {
            return Err(error);
        }
        let state_report = state.finish()?;
        if status != ffi::PULP7Z_OK {
            return Err(loader::native_status_error(
                status,
                &native_error,
                "Format7zF extraction failed",
            ));
        }
        Ok(state_report)
    }

    fn finish_report(
        &self,
        report: OperationReport,
        context: &OperationContext,
    ) -> crate::ArchiveResult<OperationReport> {
        context.report(crate::ProgressEvent::Finished(report.clone()));
        Ok(report)
    }
}

fn ensure_depth(context: &OperationContext, depth: usize) -> crate::ArchiveResult<()> {
    if depth >= context.limits().max_depth {
        return Err(ArchiveError::ResourceLimit {
            kind: crate::ResourceLimitKind::Depth,
            message: format!(
                "archive nesting exceeds the configured limit of {} layers",
                context.limits().max_depth
            ),
        });
    }
    Ok(())
}

fn transparent_content<'a>(
    descriptor: Option<&FormatDescriptor>,
    entries: &'a [ArchiveEntry],
) -> Option<&'a ArchiveEntry> {
    let descriptor = descriptor?;
    if !descriptor.supports(FormatCapability::Transparent) || entries.len() != 1 {
        return None;
    }
    let entry = entries.first()?;
    (entry.kind == EntryKind::File).then_some(entry)
}

#[derive(Default)]
struct LayerEntries {
    entries: Vec<ArchiveEntry>,
}

impl EntryVisitor for LayerEntries {
    fn visit(&mut self, entry: &ArchiveEntry) -> crate::ArchiveResult<()> {
        self.entries.push(entry.clone());
        Ok(())
    }
}

struct SingleEntrySink<'a> {
    index: u32,
    output: Option<&'a mut dyn WriteSeek>,
    completed: bool,
}

impl<'a> SingleEntrySink<'a> {
    fn new(index: u32, output: &'a mut dyn WriteSeek) -> Self {
        Self {
            index,
            output: Some(output),
            completed: false,
        }
    }
}

impl EntrySink for SingleEntrySink<'_> {
    fn begin<'a>(
        &'a mut self,
        entry: &ArchiveEntry,
        _context: &OperationContext,
    ) -> crate::ArchiveResult<EntrySinkDecision<'a>> {
        if entry.id.get() != u64::from(self.index) || entry.kind != EntryKind::File {
            return Ok(EntrySinkDecision::Skip);
        }
        let output = self.output.take().ok_or_else(|| {
            ArchiveError::Internal("nested archive entry stream was opened twice".to_owned())
        })?;
        Ok(EntrySinkDecision::Write(Box::new(output)))
    }

    fn finish(
        &mut self,
        entry: &ArchiveEntry,
        outcome: crate::EntryOutcome,
        _context: &OperationContext,
    ) -> crate::ArchiveResult<()> {
        if entry.id.get() == u64::from(self.index)
            && matches!(outcome, crate::EntryOutcome::Written { .. })
        {
            self.completed = true;
        }
        Ok(())
    }
}

struct LimitedOutput<'a> {
    writer: &'a mut dyn WriteSeek,
    limit: u64,
    written: u64,
    exceeded: bool,
}

impl<'a> LimitedOutput<'a> {
    fn new(writer: &'a mut dyn WriteSeek, limit: u64) -> Self {
        Self {
            writer,
            limit,
            written: 0,
            exceeded: false,
        }
    }
}

impl Write for LimitedOutput<'_> {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        let remaining = self.limit.saturating_sub(self.written);
        if buffer.len() as u64 > remaining {
            self.exceeded = true;
            return Err(io::Error::new(
                io::ErrorKind::WriteZero,
                "nested archive stream exceeds the configured limit",
            ));
        }
        let written = self.writer.write(buffer)?;
        if written as u64 > remaining {
            self.exceeded = true;
            return Err(io::Error::new(
                io::ErrorKind::WriteZero,
                "nested archive stream writer exceeded the configured limit",
            ));
        }
        self.written = self.written.saturating_add(written as u64);
        Ok(written)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.writer.flush()
    }
}

impl Seek for LimitedOutput<'_> {
    fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
        self.writer.seek(position)
    }
}

fn selected_indices(options: &ExtractOptions) -> crate::ArchiveResult<Vec<u32>> {
    let Some(selected) = options.selected.as_ref() else {
        return Ok(Vec::new());
    };
    let mut indices = selected
        .iter()
        .map(|id| {
            u32::try_from(id.get()).map_err(|_| {
                ArchiveError::invalid_input("selected entry identifier exceeds native index range")
            })
        })
        .collect::<crate::ArchiveResult<Vec<_>>>()?;
    indices.sort_unstable();
    indices.dedup();
    Ok(indices)
}

fn detection_ids(result: DetectionResult) -> Vec<ArchiveFormatId> {
    match result {
        DetectionResult::Detected(candidate) => vec![candidate.descriptor.id],
        DetectionResult::Ambiguous(candidates) => candidates
            .into_iter()
            .map(|candidate| candidate.descriptor.id)
            .collect(),
        DetectionResult::Unknown => Vec::new(),
    }
}

fn untested_indices(
    formats: &[FormatDescriptor],
    ids: &[ArchiveFormatId],
    tested: &mut [bool],
) -> Vec<usize> {
    let mut indices = Vec::with_capacity(ids.len());
    for id in ids {
        let Some(index) = formats.iter().position(|format| &format.id == id) else {
            continue;
        };
        if !tested[index] {
            tested[index] = true;
            indices.push(index);
        }
    }
    indices
}

fn capability_for(operation: OperationKind) -> FormatCapability {
    match operation {
        OperationKind::List => FormatCapability::List,
        OperationKind::Extract => FormatCapability::Extract,
        OperationKind::Test => FormatCapability::Test,
        OperationKind::Create => FormatCapability::Create,
        OperationKind::Update => FormatCapability::Update,
        OperationKind::Detect => FormatCapability::List,
    }
}

#[cfg(test)]
mod tests {
    use super::{FormatCapability, transparent_content};
    use crate::{ArchiveEntry, ArchiveFormatId, EntryId, EntryKind, EntryName, FormatDescriptor};

    fn transparent_descriptor() -> FormatDescriptor {
        FormatDescriptor::new(
            ArchiveFormatId::new("wrapper"),
            "wrapper",
            ["wrap"],
            [
                FormatCapability::List,
                FormatCapability::Extract,
                FormatCapability::Transparent,
            ],
        )
    }

    fn entry(name: &str, kind: EntryKind) -> ArchiveEntry {
        ArchiveEntry::new(
            EntryId::new(0),
            EntryName::parse(name).expect("test entry name should be valid"),
            kind,
        )
    }

    #[test]
    fn transparent_layers_accept_named_single_streams() {
        let descriptor = transparent_descriptor();
        let entries = [entry("payload.tar", EntryKind::File)];

        assert_eq!(
            transparent_content(Some(&descriptor), &entries).map(|value| value.name.as_str()),
            Some("payload.tar")
        );
    }

    #[test]
    fn transparent_layers_reject_directories_and_multiple_entries() {
        let descriptor = transparent_descriptor();
        let directory = [entry("payload", EntryKind::Directory)];
        let multiple = [
            entry("first", EntryKind::File),
            entry("second", EntryKind::File),
        ];

        assert!(transparent_content(Some(&descriptor), &directory).is_none());
        assert!(transparent_content(Some(&descriptor), &multiple).is_none());
    }
}
