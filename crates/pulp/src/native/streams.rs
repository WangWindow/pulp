//! Adapters between Rust I/O traits and the bridge stream callbacks.

use std::ffi::c_void;
use std::io::{Read, Seek, SeekFrom};
use std::panic::{AssertUnwindSafe, catch_unwind};

use crate::{ArchiveError, ReadSeek, WriteSeek};

use super::ffi;

pub struct InputCallbacks<'a> {
    state: InputState<'a>,
    raw: ffi::Pulp7zInputCallbacks,
}

pub struct InputState<'a> {
    reader: &'a mut dyn ReadSeek,
    error: Option<ArchiveError>,
}

impl<'a> InputCallbacks<'a> {
    pub fn new(reader: &'a mut dyn ReadSeek) -> Self {
        Self {
            state: InputState {
                reader,
                error: None,
            },
            raw: ffi::Pulp7zInputCallbacks {
                user: std::ptr::null_mut(),
                read: Some(input_read),
                seek: Some(input_seek),
            },
        }
    }

    pub fn raw(&mut self) -> &ffi::Pulp7zInputCallbacks {
        self.raw.user = (&mut self.state as *mut InputState<'a>).cast::<c_void>();
        &self.raw
    }

    pub fn take_error(&mut self) -> Option<ArchiveError> {
        self.state.error.take()
    }
}

struct VolumeState {
    reader: Box<dyn ReadSeek>,
    error_sink: *mut Option<ArchiveError>,
}

/// Transfers ownership of a volume reader to the native provider.
pub fn volume_callbacks(
    reader: Box<dyn ReadSeek>,
    error_sink: *mut Option<ArchiveError>,
) -> ffi::Pulp7zVolumeCallbacks {
    let state = Box::new(VolumeState { reader, error_sink });
    ffi::Pulp7zVolumeCallbacks {
        user: Box::into_raw(state).cast(),
        read: Some(volume_read),
        seek: Some(volume_seek),
        close: Some(volume_close),
    }
}

pub struct OutputCallbacks<'a> {
    state: OutputState<'a>,
    raw: ffi::Pulp7zOutputCallbacks,
}

pub struct OutputState<'a> {
    writer: &'a mut dyn WriteSeek,
    error: Option<ArchiveError>,
    bytes: u64,
}

impl<'a> OutputCallbacks<'a> {
    pub fn new(writer: &'a mut dyn WriteSeek) -> Self {
        Self {
            state: OutputState {
                writer,
                error: None,
                bytes: 0,
            },
            raw: ffi::Pulp7zOutputCallbacks {
                user: std::ptr::null_mut(),
                write: Some(output_write),
                seek: Some(output_seek),
            },
        }
    }

    pub fn raw(&mut self) -> &ffi::Pulp7zOutputCallbacks {
        self.raw.user = (&mut self.state as *mut OutputState<'a>).cast::<c_void>();
        &self.raw
    }

    pub fn take_error(&mut self) -> Option<ArchiveError> {
        self.state.error.take()
    }

    pub fn bytes(&self) -> u64 {
        self.state.bytes
    }
}

unsafe extern "C" fn input_read(
    user: *mut c_void,
    data: *mut u8,
    size: u32,
    processed: *mut u32,
) -> i32 {
    let result = catch_unwind(AssertUnwindSafe(|| unsafe {
        input_read_inner(user, data, size, processed)
    }));
    match result {
        Ok(status) => status,
        Err(_) => unsafe {
            fail_input(
                user,
                ArchiveError::Internal("input callback panicked".to_owned()),
            )
        },
    }
}

unsafe fn input_read_inner(
    user: *mut c_void,
    data: *mut u8,
    size: u32,
    processed: *mut u32,
) -> i32 {
    if user.is_null() || processed.is_null() || (data.is_null() && size != 0) {
        return unsafe {
            fail_input(
                user,
                ArchiveError::invalid_input("invalid input callback buffer"),
            )
        };
    }
    unsafe { *processed = 0 };
    if size == 0 {
        return ffi::PULP7Z_OK;
    }
    let state = unsafe { &mut *user.cast::<InputState<'static>>() };
    let buffer = unsafe { std::slice::from_raw_parts_mut(data, size as usize) };
    match state.reader.read(buffer) {
        Ok(count) if count <= size as usize => {
            unsafe { *processed = count as u32 };
            ffi::PULP7Z_OK
        }
        Ok(_) => unsafe {
            fail_input(
                user,
                ArchiveError::Internal("reader returned too many bytes".to_owned()),
            )
        },
        Err(error) => unsafe { fail_input(user, ArchiveError::Io(error)) },
    }
}

unsafe extern "C" fn input_seek(
    user: *mut c_void,
    offset: i64,
    origin: u32,
    position: *mut u64,
) -> i32 {
    let result = catch_unwind(AssertUnwindSafe(|| unsafe {
        input_seek_inner(user, offset, origin, position)
    }));
    match result {
        Ok(status) => status,
        Err(_) => unsafe {
            fail_input(
                user,
                ArchiveError::Internal("seek callback panicked".to_owned()),
            )
        },
    }
}

unsafe fn input_seek_inner(user: *mut c_void, offset: i64, origin: u32, position: *mut u64) -> i32 {
    if user.is_null() {
        return unsafe {
            fail_input(
                user,
                ArchiveError::invalid_input("invalid seek callback arguments"),
            )
        };
    }
    let seek_from = match origin {
        0 if offset >= 0 => SeekFrom::Start(offset as u64),
        1 => SeekFrom::Current(offset),
        2 => SeekFrom::End(offset),
        _ => {
            return unsafe {
                fail_input(
                    user,
                    ArchiveError::invalid_input("invalid seek origin or offset"),
                )
            };
        }
    };
    let state = unsafe { &mut *user.cast::<InputState<'static>>() };
    match state.reader.seek(seek_from) {
        Ok(value) => {
            if !position.is_null() {
                unsafe { *position = value };
            }
            ffi::PULP7Z_OK
        }
        Err(error) => unsafe { fail_input(user, ArchiveError::Io(error)) },
    }
}

unsafe extern "C" fn volume_read(
    user: *mut c_void,
    data: *mut u8,
    size: u32,
    processed: *mut u32,
) -> i32 {
    let result = catch_unwind(AssertUnwindSafe(|| unsafe {
        volume_read_inner(user, data, size, processed)
    }));
    match result {
        Ok(status) => status,
        Err(_) => unsafe {
            fail_volume(
                user,
                ArchiveError::Internal("volume read callback panicked".to_owned()),
            )
        },
    }
}

unsafe fn volume_read_inner(
    user: *mut c_void,
    data: *mut u8,
    size: u32,
    processed: *mut u32,
) -> i32 {
    if user.is_null() || processed.is_null() || (data.is_null() && size != 0) {
        return unsafe {
            fail_volume(
                user,
                ArchiveError::invalid_input("invalid volume read buffer"),
            )
        };
    }
    unsafe { *processed = 0 };
    if size == 0 {
        return ffi::PULP7Z_OK;
    }
    let state = unsafe { &mut *user.cast::<VolumeState>() };
    let buffer = unsafe { std::slice::from_raw_parts_mut(data, size as usize) };
    match state.reader.read(buffer) {
        Ok(count) if count <= size as usize => {
            unsafe { *processed = count as u32 };
            ffi::PULP7Z_OK
        }
        Ok(_) => unsafe {
            fail_volume(
                user,
                ArchiveError::Internal("volume reader returned too many bytes".to_owned()),
            )
        },
        Err(error) => unsafe { fail_volume(user, ArchiveError::Io(error)) },
    }
}

unsafe extern "C" fn volume_seek(
    user: *mut c_void,
    offset: i64,
    origin: u32,
    position: *mut u64,
) -> i32 {
    let result = catch_unwind(AssertUnwindSafe(|| unsafe {
        volume_seek_inner(user, offset, origin, position)
    }));
    match result {
        Ok(status) => status,
        Err(_) => unsafe {
            fail_volume(
                user,
                ArchiveError::Internal("volume seek callback panicked".to_owned()),
            )
        },
    }
}

unsafe fn volume_seek_inner(
    user: *mut c_void,
    offset: i64,
    origin: u32,
    position: *mut u64,
) -> i32 {
    if user.is_null() {
        return unsafe {
            fail_volume(
                user,
                ArchiveError::invalid_input("invalid volume seek arguments"),
            )
        };
    }
    let seek_from = match origin {
        0 if offset >= 0 => SeekFrom::Start(offset as u64),
        1 => SeekFrom::Current(offset),
        2 => SeekFrom::End(offset),
        _ => {
            return unsafe {
                fail_volume(
                    user,
                    ArchiveError::invalid_input("invalid volume seek origin or offset"),
                )
            };
        }
    };
    let state = unsafe { &mut *user.cast::<VolumeState>() };
    match state.reader.seek(seek_from) {
        Ok(value) => {
            if !position.is_null() {
                unsafe { *position = value };
            }
            ffi::PULP7Z_OK
        }
        Err(error) => unsafe { fail_volume(user, ArchiveError::Io(error)) },
    }
}

unsafe extern "C" fn volume_close(user: *mut c_void) {
    if !user.is_null() {
        unsafe { drop(Box::from_raw(user.cast::<VolumeState>())) };
    }
}

unsafe extern "C" fn output_write(
    user: *mut c_void,
    data: *const u8,
    size: u32,
    processed: *mut u32,
) -> i32 {
    let result = catch_unwind(AssertUnwindSafe(|| unsafe {
        output_write_inner(user, data, size, processed)
    }));
    match result {
        Ok(status) => status,
        Err(_) => unsafe {
            fail_output(
                user,
                ArchiveError::Internal("output callback panicked".to_owned()),
            )
        },
    }
}

unsafe extern "C" fn output_seek(
    user: *mut c_void,
    offset: i64,
    origin: u32,
    position: *mut u64,
) -> i32 {
    let result = catch_unwind(AssertUnwindSafe(|| unsafe {
        output_seek_inner(user, offset, origin, position)
    }));
    match result {
        Ok(status) => status,
        Err(_) => unsafe {
            fail_output(
                user,
                ArchiveError::Internal("output seek callback panicked".to_owned()),
            )
        },
    }
}

unsafe fn output_seek_inner(
    user: *mut c_void,
    offset: i64,
    origin: u32,
    position: *mut u64,
) -> i32 {
    if user.is_null() {
        return unsafe {
            fail_output(
                user,
                ArchiveError::invalid_input("invalid output seek arguments"),
            )
        };
    }
    let seek_from = match origin {
        0 if offset >= 0 => SeekFrom::Start(offset as u64),
        1 => SeekFrom::Current(offset),
        2 => SeekFrom::End(offset),
        _ => {
            return unsafe {
                fail_output(
                    user,
                    ArchiveError::invalid_input("invalid output seek origin or offset"),
                )
            };
        }
    };
    let state = unsafe { &mut *user.cast::<OutputState<'static>>() };
    match state.writer.seek(seek_from) {
        Ok(value) => {
            if !position.is_null() {
                unsafe { *position = value };
            }
            ffi::PULP7Z_OK
        }
        Err(error) => unsafe { fail_output(user, ArchiveError::Io(error)) },
    }
}

unsafe fn output_write_inner(
    user: *mut c_void,
    data: *const u8,
    size: u32,
    processed: *mut u32,
) -> i32 {
    if user.is_null() || processed.is_null() || (data.is_null() && size != 0) {
        return unsafe {
            fail_output(
                user,
                ArchiveError::invalid_input("invalid output callback buffer"),
            )
        };
    }
    unsafe { *processed = 0 };
    if size == 0 {
        return ffi::PULP7Z_OK;
    }
    let state = unsafe { &mut *user.cast::<OutputState<'static>>() };
    let buffer = unsafe { std::slice::from_raw_parts(data, size as usize) };
    match state.writer.write(buffer) {
        Ok(count) => {
            if count > size as usize {
                return unsafe {
                    fail_output(
                        user,
                        ArchiveError::Internal("writer returned too many bytes".to_owned()),
                    )
                };
            }
            unsafe { *processed = count as u32 };
            state.bytes = state.bytes.saturating_add(count as u64);
            ffi::PULP7Z_OK
        }
        Err(error) => unsafe { fail_output(user, ArchiveError::Io(error)) },
    }
}

unsafe fn fail_input(user: *mut c_void, error: ArchiveError) -> i32 {
    if !user.is_null() {
        let state = unsafe { &mut *user.cast::<InputState<'static>>() };
        if state.error.is_none() {
            state.error = Some(error);
        }
    }
    ffi::PULP7Z_CALLBACK_ERROR
}

unsafe fn fail_output(user: *mut c_void, error: ArchiveError) -> i32 {
    if !user.is_null() {
        let state = unsafe { &mut *user.cast::<OutputState<'static>>() };
        if state.error.is_none() {
            state.error = Some(error);
        }
    }
    ffi::PULP7Z_CALLBACK_ERROR
}

unsafe fn fail_volume(user: *mut c_void, error: ArchiveError) -> i32 {
    if !user.is_null() {
        let state = unsafe { &mut *user.cast::<VolumeState>() };
        if !state.error_sink.is_null() {
            let sink = unsafe { &mut *state.error_sink };
            if sink.is_none() {
                *sink = Some(error);
            }
        }
    }
    ffi::PULP7Z_CALLBACK_ERROR
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::{InputCallbacks, OutputCallbacks};

    #[test]
    fn inline_input_callbacks_preserve_read_and_seek_behavior() {
        let mut input = Cursor::new(b"abc".to_vec());
        let mut callbacks = InputCallbacks::new(&mut input);
        let (user, read, seek) = {
            let raw = callbacks.raw();
            (
                raw.user,
                raw.read.expect("read callback"),
                raw.seek.expect("seek callback"),
            )
        };

        let mut buffer = [0_u8; 8];
        let mut processed = 0;
        let status = unsafe {
            read(
                user,
                buffer.as_mut_ptr(),
                buffer.len() as u32,
                &mut processed,
            )
        };
        assert_eq!(status, 0);
        assert_eq!(processed, 3);
        assert_eq!(&buffer[..3], b"abc");

        let mut position = 99;
        let status = unsafe { seek(user, 0, 0, &mut position) };
        assert_eq!(status, 0);
        assert_eq!(position, 0);

        let status = unsafe { read(user, std::ptr::null_mut(), 0, &mut processed) };
        assert_eq!(status, 0);
        assert!(callbacks.take_error().is_none());
    }

    #[test]
    fn inline_output_callbacks_preserve_write_and_seek_behavior() {
        let mut output = Cursor::new(Vec::new());
        let mut callbacks = OutputCallbacks::new(&mut output);
        let (user, write, seek) = {
            let raw = callbacks.raw();
            (
                raw.user,
                raw.write.expect("write callback"),
                raw.seek.expect("seek callback"),
            )
        };

        let data = b"abc";
        let mut processed = 0;
        let status = unsafe { write(user, data.as_ptr(), data.len() as u32, &mut processed) };
        assert_eq!(status, 0);
        assert_eq!(processed, 3);

        let status = unsafe { write(user, std::ptr::null(), 0, &mut processed) };
        assert_eq!(status, 0);
        let status = unsafe { seek(user, 0, 0, std::ptr::null_mut()) };
        assert_eq!(status, 0);
        assert_eq!(callbacks.bytes(), 3);
        assert!(callbacks.take_error().is_none());

        drop(callbacks);
        assert_eq!(output.into_inner(), data);
    }
}
