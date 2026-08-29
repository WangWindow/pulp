//! Rust callback state used while the native provider is running.

use std::ffi::c_void;
use std::io::Read;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::{
    ArchiveEntry, ArchiveError, Checksum, EntryId, EntryKind, EntryName, EntryOutcome, EntrySink,
    EntrySinkDecision, EntrySource, EntryVisitor, OperationContext, OperationKind, OperationReport,
    PasswordReason, ProgressEvent,
};

use super::{ffi, streams};

const ASK_TEST: u32 = 1;
const PROGRESS_MIN_BYTES: u64 = 1024 * 1024;
const PROGRESS_MIN_INTERVAL: Duration = Duration::from_millis(50);

pub struct CallbackCommon<'a> {
    pub context: &'a OperationContext,
    pub report: OperationReport,
    pub error: Option<ArchiveError>,
    last_reported_completed: u64,
    progress_processed: u64,
    last_progress_at: Option<Instant>,
    declared_bytes: u64,
}

impl<'a> CallbackCommon<'a> {
    fn new(context: &'a OperationContext, operation: OperationKind) -> Self {
        Self {
            context,
            report: OperationReport::new(operation),
            error: None,
            last_reported_completed: 0,
            progress_processed: 0,
            last_progress_at: None,
            declared_bytes: 0,
        }
    }

    fn fail(&mut self, error: ArchiveError) -> i32 {
        if self.error.is_none() {
            self.error = Some(error);
        }
        ffi::PULP7Z_CALLBACK_ERROR
    }

    fn progress(&mut self, total: u64, completed: u64, _phase: u32) -> i32 {
        if let Err(error) = self.context.check_cancelled() {
            return self.fail(error);
        }
        let now = Instant::now();
        let delta = completed.saturating_sub(self.last_reported_completed);
        let processed = self.progress_processed.saturating_add(delta);
        let should_report = self.last_progress_at.is_none()
            || delta >= PROGRESS_MIN_BYTES
            || (total != 0 && completed >= total)
            || self
                .last_progress_at
                .is_some_and(|last| now.duration_since(last) >= PROGRESS_MIN_INTERVAL);
        if should_report {
            self.context.report(ProgressEvent::Bytes {
                delta,
                processed,
                // Format7zF reports totals for individual native streams.
                // They can reset while one archive operation is still active,
                // so exposing them as an operation total produces false UI
                // progress (especially for solid and multi-volume archives).
                total: None,
            });
            self.last_reported_completed = completed;
            self.progress_processed = processed;
            self.last_progress_at = Some(now);
        }
        ffi::PULP7Z_OK
    }

    fn finish(self) -> Result<OperationReport, ArchiveError> {
        match self.error {
            Some(error) => Err(error),
            None => Ok(self.report),
        }
    }

    fn accept_entry(&mut self, entry: &ArchiveEntry) -> Result<(), ArchiveError> {
        self.context.validate_entry(entry)?;
        if self.report.entries_seen >= self.context.limits().max_entries {
            return Err(ArchiveError::ResourceLimit {
                kind: crate::ResourceLimitKind::Entries,
                message: "entry count exceeds the configured limit".to_owned(),
            });
        }
        let entry_bytes = entry.size.unwrap_or(0);
        let total = self
            .declared_bytes
            .checked_add(entry_bytes)
            .ok_or_else(|| ArchiveError::ResourceLimit {
                kind: crate::ResourceLimitKind::TotalBytes,
                message: "declared byte count overflowed".to_owned(),
            })?;
        if total > self.context.limits().max_total_bytes {
            return Err(ArchiveError::ResourceLimit {
                kind: crate::ResourceLimitKind::TotalBytes,
                message: format!(
                    "entries declare {total} bytes, maximum is {}",
                    self.context.limits().max_total_bytes
                ),
            });
        }
        self.declared_bytes = total;
        Ok(())
    }
}

pub struct ListState<'a> {
    pub common: CallbackCommon<'a>,
    pub visitor: &'a mut dyn EntryVisitor,
    archive_name: Option<String>,
}

pub fn list_entry_callback() -> ffi::EntryCallback {
    list_entry
}

impl<'a> ListState<'a> {
    pub fn new(visitor: &'a mut dyn EntryVisitor, context: &'a OperationContext) -> Self {
        Self {
            common: CallbackCommon::new(context, OperationKind::List),
            visitor,
            archive_name: context.archive_name(),
        }
    }

    pub fn open_callbacks(&mut self) -> ffi::Pulp7zOpenCallbacks {
        ffi::Pulp7zOpenCallbacks {
            user: (self as *mut Self).cast::<c_void>(),
            progress: Some(list_progress),
            password: Some(list_password),
            volume: Some(list_volume),
            archive_name: self
                .archive_name
                .as_deref()
                .map_or(std::ptr::null(), |name| name.as_ptr().cast()),
            archive_name_len: self
                .archive_name
                .as_ref()
                .map_or(0, |name| name.len() as u32),
        }
    }

    pub fn volume_only_callbacks(&mut self) -> ffi::Pulp7zOpenCallbacks {
        ffi::Pulp7zOpenCallbacks {
            user: (self as *mut Self).cast::<c_void>(),
            progress: None,
            password: None,
            volume: Some(list_volume),
            archive_name: self
                .archive_name
                .as_deref()
                .map_or(std::ptr::null(), |name| name.as_ptr().cast()),
            archive_name_len: self
                .archive_name
                .as_ref()
                .map_or(0, |name| name.len() as u32),
        }
    }

    pub fn finish(self) -> Result<OperationReport, ArchiveError> {
        self.common.finish()
    }
}

pub struct ExtractState<'a> {
    pub common: CallbackCommon<'a>,
    pub sink: &'a mut dyn EntrySink,
    pub writer: Option<Box<dyn std::io::Write + 'a>>,
    pub current: Option<ArchiveEntry>,
    pub decision: u32,
    pub test_mode: bool,
    pub current_bytes: u64,
    archive_name: Option<String>,
}

impl<'a> ExtractState<'a> {
    pub fn new(
        sink: &'a mut dyn EntrySink,
        context: &'a OperationContext,
        test_mode: bool,
    ) -> Self {
        Self {
            common: CallbackCommon::new(
                context,
                if test_mode {
                    OperationKind::Test
                } else {
                    OperationKind::Extract
                },
            ),
            sink,
            writer: None,
            current: None,
            decision: 0,
            test_mode,
            current_bytes: 0,
            archive_name: context.archive_name(),
        }
    }

    pub fn callbacks(&mut self) -> ffi::Pulp7zExtractCallbacks {
        ffi::Pulp7zExtractCallbacks {
            user: (self as *mut Self).cast::<c_void>(),
            progress: Some(extract_progress),
            password: Some(extract_password),
            volume: Some(extract_volume),
            archive_name: self
                .archive_name
                .as_deref()
                .map_or(std::ptr::null(), |name| name.as_ptr().cast()),
            archive_name_len: self
                .archive_name
                .as_ref()
                .map_or(0, |name| name.len() as u32),
            begin: Some(extract_begin),
            write: Some(extract_write),
            finish: Some(extract_finish),
        }
    }

    pub fn finish(self) -> Result<OperationReport, ArchiveError> {
        self.common.finish()
    }
}

pub struct SourceState<'a> {
    pub common: CallbackCommon<'a>,
    pub source: *mut (dyn EntrySource + 'static),
    pub entries: Vec<ArchiveEntry>,
    pub reader: Option<Box<dyn Read + 'a>>,
    pub reader_index: Option<u32>,
    pub reader_bytes: u64,
    pub fixed_password: Option<crate::Password>,
}

impl<'a> SourceState<'a> {
    pub fn new(
        source: &'a mut dyn EntrySource,
        entries: Vec<ArchiveEntry>,
        context: &'a OperationContext,
        operation: OperationKind,
        fixed_password: Option<crate::Password>,
    ) -> Self {
        let source = unsafe {
            std::mem::transmute::<*mut dyn EntrySource, *mut (dyn EntrySource + 'static)>(source)
        };
        Self {
            common: CallbackCommon::new(context, operation),
            source,
            entries,
            reader: None,
            reader_index: None,
            reader_bytes: 0,
            fixed_password,
        }
    }

    pub fn callbacks(&mut self) -> ffi::Pulp7zSourceCallbacks {
        ffi::Pulp7zSourceCallbacks {
            user: (self as *mut Self).cast::<c_void>(),
            count: self.entries.len() as u32,
            entry: Some(source_entry),
            read: Some(source_read),
            progress: Some(source_progress),
            password: Some(source_password),
        }
    }

    pub fn finish(self) -> Result<OperationReport, ArchiveError> {
        self.common.finish()
    }
}

pub fn entry_from_native(info: &ffi::Pulp7zEntryInfo) -> Result<ArchiveEntry, ArchiveError> {
    let path = native_text(info.path, info.path_len);
    let name = EntryName::parse(path)?;
    let link_target = if info.link_target.is_null() || info.link_target_len == 0 {
        None
    } else {
        Some(native_text(info.link_target, info.link_target_len))
    };
    let kind = if info.is_dir != 0 {
        EntryKind::Directory
    } else {
        match (info.link_kind, link_target.is_some()) {
            (2, true) => EntryKind::Hardlink,
            (_, true) => EntryKind::Symlink,
            _ => EntryKind::File,
        }
    };
    Ok(ArchiveEntry {
        id: EntryId::new(info.index as u64),
        name,
        kind,
        size: (info.has_size != 0).then_some(info.size),
        packed_size: (info.has_pack_size != 0).then_some(info.pack_size),
        modified: (info.has_mtime != 0)
            .then(|| system_time_from_unix_ns(info.mtime_unix_ns))
            .flatten(),
        accessed: None,
        created: None,
        attributes: (info.has_attrib != 0).then_some(info.attrib as u64),
        posix_attributes: (info.has_posix_attrib != 0).then_some(info.posix_attrib as u64),
        checksum: (info.has_crc != 0).then_some(Checksum {
            algorithm: "CRC32".to_owned(),
            value: info.crc.to_le_bytes().to_vec(),
        }),
        encrypted: info.encrypted != 0,
        link_target,
        method: if info.method.is_null() || info.method_len == 0 {
            None
        } else {
            Some(native_text(info.method, info.method_len))
        },
    })
}

pub fn fill_source_info(
    entry: &ArchiveEntry,
    index: u32,
    output: &mut ffi::Pulp7zEntryInfo,
) -> Result<(), ArchiveError> {
    if matches!(
        entry.kind,
        EntryKind::Special | EntryKind::Hardlink | EntryKind::Symlink
    ) {
        return Err(ArchiveError::invalid_input(format!(
            "entry '{}' uses an unsupported link or special-file kind",
            entry.name
        )));
    }
    let path = entry.name.as_str().as_bytes();
    let path_len = u32::try_from(path.len())
        .map_err(|_| ArchiveError::invalid_input("entry path exceeds native length limit"))?;
    output.index = index;
    output.path = path.as_ptr().cast();
    output.path_len = path_len;
    output.is_dir = u8::from(matches!(entry.kind, EntryKind::Directory));
    output.encrypted = 0;
    if let Some(size) = entry.size {
        output.has_size = 1;
        output.size = size;
    }
    if let Some(time) = entry.modified {
        output.has_mtime = 1;
        output.mtime_unix_ns = unix_ns_from_system_time(time);
    }
    if let Some(attributes) = entry.attributes {
        output.has_attrib = 1;
        output.attrib = attributes.min(u32::MAX as u64) as u32;
    }
    if let Some(attributes) = entry.posix_attributes {
        output.has_posix_attrib = 1;
        output.posix_attrib = attributes.min(u32::MAX as u64) as u32;
    }
    Ok(())
}

unsafe extern "C" fn list_entry(user: *mut c_void, info: *const ffi::Pulp7zEntryInfo) -> i32 {
    callback_status(
        catch_unwind(AssertUnwindSafe(|| unsafe { list_entry_inner(user, info) })),
        user,
        CallbackKind::List,
    )
}

unsafe fn list_entry_inner(user: *mut c_void, info: *const ffi::Pulp7zEntryInfo) -> i32 {
    let Some(state) = (unsafe { user.cast::<ListState<'static>>().as_mut() }) else {
        return ffi::PULP7Z_INVALID_ARGUMENT;
    };
    let Some(info) = (unsafe { info.as_ref() }) else {
        return state
            .common
            .fail(ArchiveError::invalid_input("native list entry was null"));
    };
    if let Err(error) = state.common.context.check_cancelled() {
        return state.common.fail(error);
    }
    let entry = match entry_from_native(info) {
        Ok(entry) => entry,
        Err(error) => return state.common.fail(error),
    };
    if let Err(error) = state.common.accept_entry(&entry) {
        return state.common.fail(error);
    }
    state.common.report.entries_seen = state.common.report.entries_seen.saturating_add(1);
    state.common.context.report(ProgressEvent::EntryStarted {
        id: entry.id,
        name: entry.name.clone(),
        size: entry.size,
    });
    match state.visitor.visit(&entry) {
        Ok(()) => {
            state.common.report.entries_completed =
                state.common.report.entries_completed.saturating_add(1);
            ffi::PULP7Z_OK
        }
        Err(error) => state.common.fail(error),
    }
}

unsafe extern "C" fn extract_begin(
    user: *mut c_void,
    info: *const ffi::Pulp7zEntryInfo,
    ask_mode: u32,
    decision: *mut u32,
) -> i32 {
    callback_status(
        catch_unwind(AssertUnwindSafe(|| unsafe {
            extract_begin_inner(user, info, ask_mode, decision)
        })),
        user,
        CallbackKind::Extract,
    )
}

unsafe fn extract_begin_inner(
    user: *mut c_void,
    info: *const ffi::Pulp7zEntryInfo,
    ask_mode: u32,
    decision: *mut u32,
) -> i32 {
    let Some(state) = (unsafe { user.cast::<ExtractState<'static>>().as_mut() }) else {
        return ffi::PULP7Z_INVALID_ARGUMENT;
    };
    if decision.is_null() {
        return state.common.fail(ArchiveError::invalid_input(
            "native decision pointer was null",
        ));
    }
    unsafe { *decision = 0 };
    let Some(info) = (unsafe { info.as_ref() }) else {
        return state
            .common
            .fail(ArchiveError::invalid_input("native extract entry was null"));
    };
    if let Err(error) = state.common.context.check_cancelled() {
        return state.common.fail(error);
    }
    if state.writer.take().is_some() || state.current.take().is_some() {
        return state.common.fail(ArchiveError::Internal(
            "native started an entry before finishing the previous entry".to_owned(),
        ));
    }
    let entry = match entry_from_native(info) {
        Ok(entry) => entry,
        Err(error) => return state.common.fail(error),
    };
    if let Err(error) = state.common.accept_entry(&entry) {
        return state.common.fail(error);
    }
    state.common.report.entries_seen = state.common.report.entries_seen.saturating_add(1);
    state.common.context.report(ProgressEvent::EntryStarted {
        id: entry.id,
        name: entry.name.clone(),
        size: entry.size,
    });
    let sink_decision = match state.sink.begin(&entry, state.common.context) {
        Ok(decision) => decision,
        Err(error) => return state.common.fail(error),
    };
    state.current_bytes = 0;
    match sink_decision {
        EntrySinkDecision::Skip => {
            unsafe { *decision = 0 };
        }
        EntrySinkDecision::MetadataOnly => {
            unsafe { *decision = 1 };
        }
        EntrySinkDecision::Write(writer) if state.test_mode || ask_mode == ASK_TEST => {
            drop(writer);
            unsafe { *decision = 1 };
        }
        EntrySinkDecision::Write(writer) => {
            if !matches!(entry.kind, EntryKind::File) {
                drop(writer);
                unsafe { *decision = 1 };
                state.decision = 1;
                state.current = Some(entry);
                return ffi::PULP7Z_OK;
            }
            // The native API keeps the output stream between GetStream and
            // SetOperationResult. The core writer may borrow the sink, so the
            // adapter keeps it for exactly that native entry lifetime.
            state.writer = Some(unsafe { extend_writer_lifetime(writer) });
            unsafe { *decision = 2 };
        }
    }
    state.decision = unsafe { *decision };
    state.current = Some(entry);
    ffi::PULP7Z_OK
}

unsafe extern "C" fn extract_write(
    user: *mut c_void,
    data: *const u8,
    size: u32,
    processed: *mut u32,
) -> i32 {
    callback_status(
        catch_unwind(AssertUnwindSafe(|| unsafe {
            extract_write_inner(user, data, size, processed)
        })),
        user,
        CallbackKind::Extract,
    )
}

unsafe fn extract_write_inner(
    user: *mut c_void,
    data: *const u8,
    size: u32,
    processed: *mut u32,
) -> i32 {
    let Some(state) = (unsafe { user.cast::<ExtractState<'static>>().as_mut() }) else {
        return ffi::PULP7Z_INVALID_ARGUMENT;
    };
    if processed.is_null() || (data.is_null() && size != 0) {
        return state
            .common
            .fail(ArchiveError::invalid_input("invalid native output buffer"));
    }
    unsafe { *processed = 0 };
    if size == 0 {
        return ffi::PULP7Z_OK;
    }
    let Some(entry) = state.current.as_ref() else {
        return state.common.fail(ArchiveError::Internal(
            "native wrote without a current entry".to_owned(),
        ));
    };
    let entry_limit = entry
        .size
        .unwrap_or(state.common.context.limits().max_entry_bytes)
        .min(state.common.context.limits().max_entry_bytes);
    let projected_entry_bytes = match state.current_bytes.checked_add(size as u64) {
        Some(bytes) => bytes,
        None => {
            return state.common.fail(ArchiveError::ResourceLimit {
                kind: crate::ResourceLimitKind::EntryBytes,
                message: "extracted entry byte count overflowed".to_owned(),
            });
        }
    };
    if projected_entry_bytes > entry_limit {
        let message = if entry.size.is_some() {
            format!(
                "native output for '{}' exceeds its declared size",
                entry.name
            )
        } else {
            "extracted entry exceeds configured limit".to_owned()
        };
        return state.common.fail(if entry.size.is_some() {
            ArchiveError::DataError(message)
        } else {
            ArchiveError::ResourceLimit {
                kind: crate::ResourceLimitKind::EntryBytes,
                message,
            }
        });
    }
    let projected_total_bytes = match state.common.report.bytes_written.checked_add(size as u64) {
        Some(bytes) => bytes,
        None => {
            return state.common.fail(ArchiveError::ResourceLimit {
                kind: crate::ResourceLimitKind::TotalBytes,
                message: "extracted byte count overflowed".to_owned(),
            });
        }
    };
    if projected_total_bytes > state.common.context.limits().max_total_bytes {
        return state.common.fail(ArchiveError::ResourceLimit {
            kind: crate::ResourceLimitKind::TotalBytes,
            message: "extracted byte count exceeds configured limit".to_owned(),
        });
    }
    let Some(writer) = state.writer.as_mut() else {
        return state.common.fail(ArchiveError::Internal(
            "native wrote without a selected output".to_owned(),
        ));
    };
    let buffer = unsafe { std::slice::from_raw_parts(data, size as usize) };
    match std::io::Write::write(writer, buffer) {
        Ok(count) if count <= size as usize => {
            unsafe { *processed = count as u32 };
            state.current_bytes = state.current_bytes.saturating_add(count as u64);
            state.common.report.bytes_written = state
                .common
                .report
                .bytes_written
                .saturating_add(count as u64);
            ffi::PULP7Z_OK
        }
        Ok(_) => state.common.fail(ArchiveError::Internal(
            "writer returned too many bytes".to_owned(),
        )),
        Err(error) => state.common.fail(ArchiveError::Io(error)),
    }
}

unsafe extern "C" fn extract_finish(
    user: *mut c_void,
    info: *const ffi::Pulp7zEntryInfo,
    operation_result: i32,
    bytes: u64,
) -> i32 {
    callback_status(
        catch_unwind(AssertUnwindSafe(|| unsafe {
            extract_finish_inner(user, info, operation_result, bytes)
        })),
        user,
        CallbackKind::Extract,
    )
}

unsafe fn extract_finish_inner(
    user: *mut c_void,
    info: *const ffi::Pulp7zEntryInfo,
    operation_result: i32,
    bytes: u64,
) -> i32 {
    let Some(state) = (unsafe { user.cast::<ExtractState<'static>>().as_mut() }) else {
        return ffi::PULP7Z_INVALID_ARGUMENT;
    };
    let Some(entry) = state.current.take() else {
        return state.common.fail(ArchiveError::Internal(
            "native finished an entry that was not started".to_owned(),
        ));
    };
    // Drop a writer before borrowing the sink again. A filesystem sink may
    // use the writer's drop to publish or remove its temporary file.
    state.writer.take();
    let outcome = if operation_result == 0 {
        if state.decision == 2 {
            EntryOutcome::Written { bytes }
        } else {
            EntryOutcome::Skipped
        }
    } else {
        EntryOutcome::Failed
    };
    let finish_result = state.sink.finish(&entry, outcome, state.common.context);
    if let Err(error) = finish_result {
        return state.common.fail(error);
    }
    if operation_result != 0 {
        return state.common.fail(native_operation_error(operation_result));
    }
    let _ = info;
    state.common.report.entries_completed = state.common.report.entries_completed.saturating_add(1);
    ffi::PULP7Z_OK
}

unsafe extern "C" fn list_progress(
    user: *mut c_void,
    total: u64,
    completed: u64,
    phase: u32,
) -> i32 {
    callback_status(
        catch_unwind(AssertUnwindSafe(|| unsafe {
            let Some(state) = user.cast::<ListState<'static>>().as_mut() else {
                return ffi::PULP7Z_INVALID_ARGUMENT;
            };
            state.common.progress(total, completed, phase)
        })),
        user,
        CallbackKind::List,
    )
}

unsafe extern "C" fn extract_progress(
    user: *mut c_void,
    total: u64,
    completed: u64,
    phase: u32,
) -> i32 {
    callback_status(
        catch_unwind(AssertUnwindSafe(|| unsafe {
            let Some(state) = user.cast::<ExtractState<'static>>().as_mut() else {
                return ffi::PULP7Z_INVALID_ARGUMENT;
            };
            state.common.progress(total, completed, phase)
        })),
        user,
        CallbackKind::Extract,
    )
}

unsafe extern "C" fn list_volume(
    user: *mut c_void,
    name: *const std::ffi::c_char,
    name_len: u32,
    callbacks: *mut ffi::Pulp7zVolumeCallbacks,
) -> i32 {
    callback_status(
        catch_unwind(AssertUnwindSafe(|| unsafe {
            let Some(state) = user.cast::<ListState<'static>>().as_mut() else {
                return ffi::PULP7Z_INVALID_ARGUMENT;
            };
            open_volume_inner(&mut state.common, name, name_len, callbacks)
        })),
        user,
        CallbackKind::List,
    )
}

unsafe extern "C" fn extract_volume(
    user: *mut c_void,
    name: *const std::ffi::c_char,
    name_len: u32,
    callbacks: *mut ffi::Pulp7zVolumeCallbacks,
) -> i32 {
    callback_status(
        catch_unwind(AssertUnwindSafe(|| unsafe {
            let Some(state) = user.cast::<ExtractState<'static>>().as_mut() else {
                return ffi::PULP7Z_INVALID_ARGUMENT;
            };
            open_volume_inner(&mut state.common, name, name_len, callbacks)
        })),
        user,
        CallbackKind::Extract,
    )
}

unsafe fn open_volume_inner(
    common: &mut CallbackCommon<'_>,
    name: *const std::ffi::c_char,
    name_len: u32,
    callbacks: *mut ffi::Pulp7zVolumeCallbacks,
) -> i32 {
    if callbacks.is_null() || (name.is_null() && name_len != 0) {
        return common.fail(ArchiveError::invalid_input(
            "invalid native volume callback arguments",
        ));
    }
    let name = native_text(name, name_len);
    match common.context.open_volume(&name) {
        Ok(reader) => {
            unsafe {
                *callbacks = streams::volume_callbacks(
                    reader,
                    &mut common.error as *mut Option<ArchiveError>,
                );
            }
            ffi::PULP7Z_OK
        }
        Err(ArchiveError::VolumeNotFound(_)) => ffi::PULP7Z_STREAM_UNAVAILABLE,
        Err(error) => common.fail(error),
    }
}

unsafe extern "C" fn list_password(
    user: *mut c_void,
    reason: u32,
    attempt: u32,
    password: *mut u8,
    capacity: u32,
    length: *mut u32,
) -> i32 {
    callback_status(
        catch_unwind(AssertUnwindSafe(|| unsafe {
            let Some(state) = user.cast::<ListState<'static>>().as_mut() else {
                return ffi::PULP7Z_INVALID_ARGUMENT;
            };
            password_inner(
                &mut state.common,
                reason,
                attempt,
                password,
                capacity,
                length,
                true,
            )
        })),
        user,
        CallbackKind::List,
    )
}

unsafe extern "C" fn extract_password(
    user: *mut c_void,
    reason: u32,
    attempt: u32,
    password: *mut u8,
    capacity: u32,
    length: *mut u32,
) -> i32 {
    callback_status(
        catch_unwind(AssertUnwindSafe(|| unsafe {
            let Some(state) = user.cast::<ExtractState<'static>>().as_mut() else {
                return ffi::PULP7Z_INVALID_ARGUMENT;
            };
            password_inner(
                &mut state.common,
                reason,
                attempt,
                password,
                capacity,
                length,
                true,
            )
        })),
        user,
        CallbackKind::Extract,
    )
}

unsafe extern "C" fn source_entry(
    user: *mut c_void,
    index: u32,
    output: *mut ffi::Pulp7zEntryInfo,
) -> i32 {
    callback_status(
        catch_unwind(AssertUnwindSafe(|| unsafe {
            source_entry_inner(user, index, output)
        })),
        user,
        CallbackKind::Source,
    )
}

unsafe fn source_entry_inner(
    user: *mut c_void,
    index: u32,
    output: *mut ffi::Pulp7zEntryInfo,
) -> i32 {
    let Some(state) = (unsafe { user.cast::<SourceState<'static>>().as_mut() }) else {
        return ffi::PULP7Z_INVALID_ARGUMENT;
    };
    let Some(output) = (unsafe { output.as_mut() }) else {
        return state
            .common
            .fail(ArchiveError::invalid_input("native source entry was null"));
    };
    let Some(entry) = state.entries.get(index as usize) else {
        return state.common.fail(ArchiveError::invalid_input(
            "native source index is out of range",
        ));
    };
    *output = ffi::Pulp7zEntryInfo {
        index,
        ..ffi::Pulp7zEntryInfo::default()
    };
    if let Err(error) = fill_source_info(entry, index, output) {
        return state.common.fail(error);
    }
    ffi::PULP7Z_OK
}

unsafe extern "C" fn source_read(
    user: *mut c_void,
    index: u32,
    data: *mut u8,
    size: u32,
    processed: *mut u32,
) -> i32 {
    callback_status(
        catch_unwind(AssertUnwindSafe(|| unsafe {
            source_read_inner(user, index, data, size, processed)
        })),
        user,
        CallbackKind::Source,
    )
}

unsafe fn source_read_inner(
    user: *mut c_void,
    index: u32,
    data: *mut u8,
    size: u32,
    processed: *mut u32,
) -> i32 {
    let Some(state) = (unsafe { user.cast::<SourceState<'static>>().as_mut() }) else {
        return ffi::PULP7Z_INVALID_ARGUMENT;
    };
    if processed.is_null() || (data.is_null() && size != 0) {
        return state
            .common
            .fail(ArchiveError::invalid_input("invalid native source buffer"));
    }
    unsafe { *processed = 0 };
    let Some(entry) = state.entries.get(index as usize).cloned() else {
        return state.common.fail(ArchiveError::invalid_input(
            "native source index is out of range",
        ));
    };
    if size == 0 {
        return ffi::PULP7Z_OK;
    }
    if state.reader_index != Some(index) {
        state.reader.take();
        state.reader_index = Some(index);
        state.reader_bytes = 0;
    }
    if state.reader.is_none() {
        let source = unsafe { &mut *state.source };
        let reader = match source.open(&entry, state.common.context) {
            Ok(reader) => reader,
            Err(error) => return state.common.fail(error),
        };
        state.reader = Some(unsafe { extend_reader_lifetime(reader) });
    }
    let buffer = unsafe { std::slice::from_raw_parts_mut(data, size as usize) };
    let result = match state.reader.as_mut() {
        Some(reader) => reader.read(buffer),
        None => {
            return state.common.fail(ArchiveError::Internal(
                "source reader was not initialized".to_owned(),
            ));
        }
    };
    match result {
        Ok(count) if count <= size as usize => {
            let entry_bytes = match state.reader_bytes.checked_add(count as u64) {
                Some(bytes) => bytes,
                None => {
                    return state.common.fail(ArchiveError::ResourceLimit {
                        kind: crate::ResourceLimitKind::EntryBytes,
                        message: "source entry byte count overflowed".to_owned(),
                    });
                }
            };
            if entry_bytes > entry.size.unwrap_or(u64::MAX) {
                return state.common.fail(ArchiveError::invalid_input(format!(
                    "source entry '{}' produced more bytes than declared",
                    entry.name
                )));
            }
            if entry_bytes > state.common.context.limits().max_entry_bytes {
                return state.common.fail(ArchiveError::ResourceLimit {
                    kind: crate::ResourceLimitKind::EntryBytes,
                    message: "source entry exceeds configured limit".to_owned(),
                });
            }
            let total_bytes = match state.common.report.bytes_read.checked_add(count as u64) {
                Some(bytes) => bytes,
                None => {
                    return state.common.fail(ArchiveError::ResourceLimit {
                        kind: crate::ResourceLimitKind::TotalBytes,
                        message: "source byte count overflowed".to_owned(),
                    });
                }
            };
            if total_bytes > state.common.context.limits().max_total_bytes {
                return state.common.fail(ArchiveError::ResourceLimit {
                    kind: crate::ResourceLimitKind::TotalBytes,
                    message: "source byte count exceeds configured limit".to_owned(),
                });
            }
            state.reader_bytes = entry_bytes;
            unsafe { *processed = count as u32 };
            state.common.report.bytes_read = total_bytes;
            ffi::PULP7Z_OK
        }
        Ok(_) => state.common.fail(ArchiveError::Internal(
            "source reader returned too many bytes".to_owned(),
        )),
        Err(error) => state.common.fail(ArchiveError::Io(error)),
    }
}

unsafe extern "C" fn source_progress(
    user: *mut c_void,
    total: u64,
    completed: u64,
    phase: u32,
) -> i32 {
    callback_status(
        catch_unwind(AssertUnwindSafe(|| unsafe {
            let Some(state) = user.cast::<SourceState<'static>>().as_mut() else {
                return ffi::PULP7Z_INVALID_ARGUMENT;
            };
            state.common.progress(total, completed, phase)
        })),
        user,
        CallbackKind::Source,
    )
}

unsafe extern "C" fn source_password(
    user: *mut c_void,
    reason: u32,
    attempt: u32,
    password: *mut u8,
    capacity: u32,
    length: *mut u32,
) -> i32 {
    callback_status(
        catch_unwind(AssertUnwindSafe(|| unsafe {
            let Some(state) = user.cast::<SourceState<'static>>().as_mut() else {
                return ffi::PULP7Z_INVALID_ARGUMENT;
            };
            if let Some(secret) = state.fixed_password.as_ref() {
                write_password(&mut state.common, secret, password, capacity, length)
            } else {
                password_inner(
                    &mut state.common,
                    reason,
                    attempt,
                    password,
                    capacity,
                    length,
                    false,
                )
            }
        })),
        user,
        CallbackKind::Source,
    )
}

fn password_reason(native_reason: u32) -> PasswordReason {
    if native_reason == 0 {
        PasswordReason::Header
    } else {
        PasswordReason::Data
    }
}

unsafe fn password_inner(
    common: &mut CallbackCommon<'_>,
    native_reason: u32,
    attempt: u32,
    password_buffer: *mut u8,
    capacity: u32,
    length: *mut u32,
    decline_is_error: bool,
) -> i32 {
    if length.is_null() || (password_buffer.is_null() && capacity != 0) {
        return common.fail(ArchiveError::invalid_input(
            "invalid native password buffer",
        ));
    }
    unsafe { *length = 0 };
    let reason = password_reason(native_reason);
    let requested = common
        .context
        .request_password(crate::PasswordRequest { reason, attempt });
    let secret = match requested {
        Ok(Some(password)) => password,
        Ok(None) => {
            if decline_is_error && common.error.is_none() {
                common.error = Some(ArchiveError::PasswordRequired);
            }
            return ffi::PULP7Z_PASSWORD_DECLINED;
        }
        Err(error) => return common.fail(error),
    };
    unsafe { write_password(common, &secret, password_buffer, capacity, length) }
}

unsafe fn write_password(
    common: &mut CallbackCommon<'_>,
    secret: &crate::Password,
    password_buffer: *mut u8,
    capacity: u32,
    length: *mut u32,
) -> i32 {
    if secret.as_bytes().len() > capacity as usize {
        return common.fail(ArchiveError::invalid_input(
            "password exceeds native callback capacity",
        ));
    }
    unsafe {
        if !secret.as_bytes().is_empty() {
            std::ptr::copy_nonoverlapping(
                secret.as_bytes().as_ptr(),
                password_buffer,
                secret.as_bytes().len(),
            );
        }
        *length = secret.as_bytes().len() as u32;
    }
    ffi::PULP7Z_OK
}

#[derive(Clone, Copy)]
enum CallbackKind {
    List,
    Extract,
    Source,
}

fn callback_status(
    result: Result<i32, Box<dyn std::any::Any + Send>>,
    user: *mut c_void,
    kind: CallbackKind,
) -> i32 {
    match result {
        Ok(status) => status,
        Err(_) => unsafe {
            let error = ArchiveError::Internal("Rust native callback panicked".to_owned());
            match kind {
                CallbackKind::List => user
                    .cast::<ListState<'static>>()
                    .as_mut()
                    .map(|state| state.common.fail(error)),
                CallbackKind::Extract => user
                    .cast::<ExtractState<'static>>()
                    .as_mut()
                    .map(|state| state.common.fail(error)),
                CallbackKind::Source => user
                    .cast::<SourceState<'static>>()
                    .as_mut()
                    .map(|state| state.common.fail(error)),
            }
            .unwrap_or(ffi::PULP7Z_CALLBACK_ERROR)
        },
    }
}

fn native_operation_error(result: i32) -> ArchiveError {
    match result {
        9 => ArchiveError::WrongPassword,
        2 | 3 | 5 | 6 | 7 | 8 => {
            ArchiveError::DataError(format!("native extraction result {result}"))
        }
        1 => ArchiveError::UnsupportedOperation {
            operation: OperationKind::Extract,
            format: None,
        },
        other => ArchiveError::native(other, "native extraction failed"),
    }
}

fn native_text(pointer: *const std::ffi::c_char, length: u32) -> String {
    if pointer.is_null() || length == 0 {
        return String::new();
    }
    let bytes = unsafe { std::slice::from_raw_parts(pointer.cast::<u8>(), length as usize) };
    String::from_utf8_lossy(bytes).into_owned()
}

fn system_time_from_unix_ns(value: i64) -> Option<SystemTime> {
    if value >= 0 {
        UNIX_EPOCH.checked_add(Duration::from_nanos(value as u64))
    } else {
        UNIX_EPOCH.checked_sub(Duration::from_nanos(value.unsigned_abs()))
    }
}

pub fn unix_ns_from_system_time(value: SystemTime) -> i64 {
    match value.duration_since(UNIX_EPOCH) {
        Ok(duration) => duration.as_nanos().min(i64::MAX as u128) as i64,
        Err(error) => -(error.duration().as_nanos().min(i64::MAX as u128) as i64),
    }
}

unsafe fn extend_writer_lifetime<'a>(
    writer: Box<dyn std::io::Write + '_>,
) -> Box<dyn std::io::Write + 'a> {
    unsafe { std::mem::transmute(writer) }
}

unsafe fn extend_reader_lifetime<'a>(reader: Box<dyn Read + '_>) -> Box<dyn Read + 'a> {
    unsafe { std::mem::transmute(reader) }
}

impl Default for ffi::Pulp7zEntryInfo {
    fn default() -> Self {
        Self {
            index: 0,
            path: std::ptr::null(),
            path_len: 0,
            is_dir: 0,
            encrypted: 0,
            link_kind: 0,
            has_size: 0,
            has_pack_size: 0,
            has_mtime: 0,
            has_attrib: 0,
            has_posix_attrib: 0,
            has_crc: 0,
            size: 0,
            pack_size: 0,
            mtime_unix_ns: 0,
            attrib: 0,
            posix_attrib: 0,
            crc: 0,
            method: std::ptr::null(),
            method_len: 0,
            link_target: std::ptr::null(),
            link_target_len: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::{CallbackCommon, ffi, password_reason};
    use crate::{
        CancellationToken, OperationContext, OperationKind, PasswordReason, ProgressEvent,
    };

    #[test]
    fn repeated_native_password_callbacks_reuse_the_same_password() {
        assert_eq!(password_reason(0), PasswordReason::Header);
        assert_eq!(password_reason(1), PasswordReason::Data);
        assert_eq!(password_reason(1), password_reason(1));
    }

    #[test]
    fn progress_reports_cumulative_thresholds_and_completion() {
        let events = Arc::new(Mutex::new(Vec::new()));
        let recorded = Arc::clone(&events);
        let context = OperationContext::new().with_progress(move |event| {
            recorded.lock().expect("progress mutex").push(event);
        });
        let mut common = CallbackCommon::new(&context, OperationKind::Extract);

        assert_eq!(common.progress(2 * 1024 * 1024, 0, 1), ffi::PULP7Z_OK);
        common.last_progress_at = Some(std::time::Instant::now());
        assert_eq!(
            common.progress(2 * 1024 * 1024, 512 * 1024, 1),
            ffi::PULP7Z_OK
        );
        assert_eq!(
            common.progress(2 * 1024 * 1024, 1024 * 1024, 1),
            ffi::PULP7Z_OK
        );
        common.last_progress_at = Some(std::time::Instant::now());
        assert_eq!(
            common.progress(2 * 1024 * 1024, 1536 * 1024, 1),
            ffi::PULP7Z_OK
        );
        assert_eq!(
            common.progress(2 * 1024 * 1024, 2 * 1024 * 1024, 1),
            ffi::PULP7Z_OK
        );

        let events = events.lock().expect("progress mutex");
        let bytes = events
            .iter()
            .filter_map(|event| match event {
                ProgressEvent::Bytes {
                    delta, processed, ..
                } => Some((*delta, *processed)),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            bytes,
            [
                (0, 0),
                (1024 * 1024, 1024 * 1024),
                (1024 * 1024, 2 * 1024 * 1024)
            ]
        );
    }

    #[test]
    fn progress_checks_cancellation_before_coalescing() {
        let cancellation = CancellationToken::new();
        cancellation.cancel();
        let context = OperationContext::new().with_cancellation(cancellation);
        let mut common = CallbackCommon::new(&context, OperationKind::Extract);

        assert_eq!(
            common.progress(10 * 1024 * 1024, 1, 1),
            ffi::PULP7Z_CALLBACK_ERROR
        );
        assert!(common.error.is_some());
    }

    #[test]
    fn native_stream_totals_are_not_exposed_as_operation_progress() {
        let events = Arc::new(Mutex::new(Vec::new()));
        let recorded = Arc::clone(&events);
        let context = OperationContext::new().with_progress(move |event| {
            recorded.lock().expect("progress mutex").push(event);
        });
        let mut common = CallbackCommon::new(&context, OperationKind::Extract);

        common.progress(4 * 1024 * 1024, 0, 1);
        common.last_progress_at = Some(std::time::Instant::now());
        common.progress(4 * 1024 * 1024, 4 * 1024 * 1024, 1);
        common.last_progress_at = Some(std::time::Instant::now());
        common.progress(2 * 1024 * 1024, 2 * 1024 * 1024, 1);

        let bytes = events
            .lock()
            .expect("progress mutex")
            .iter()
            .filter_map(|event| match event {
                ProgressEvent::Bytes {
                    processed, total, ..
                } => Some((*processed, *total)),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            bytes,
            [(0, None), (4 * 1024 * 1024, None), (4 * 1024 * 1024, None)]
        );
    }
}
