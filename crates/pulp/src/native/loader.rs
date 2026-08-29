//! Static Format7zF provider lifetime management.

use std::ptr::NonNull;

use crate::ArchiveError;

use super::error::Format7zError;
use super::ffi;

pub struct NativeRuntime {
    bridge: NonNull<ffi::Pulp7zBridge>,
}

impl NativeRuntime {
    pub fn bridge(&self) -> *mut ffi::Pulp7zBridge {
        self.bridge.as_ptr()
    }
}

impl Drop for NativeRuntime {
    fn drop(&mut self) {
        unsafe {
            ffi::pulp7z_bridge_destroy(self.bridge.as_ptr());
        }
    }
}

// The bridge owns only immutable function pointers and a provider handle. The
// engine serializes calls through its Mutex, so moving it between workers is
// safe and the statically linked provider has process lifetime.
unsafe impl Send for NativeRuntime {}
unsafe impl Sync for NativeRuntime {}

pub fn open() -> Result<NativeRuntime, Format7zError> {
    let mut bridge = std::ptr::null_mut();
    let mut error = ffi::Pulp7zError::default();
    let status = unsafe {
        ffi::pulp7z_bridge_create(
            ffi::CreateObject,
            ffi::GetNumberOfFormats,
            ffi::GetHandlerProperty,
            ffi::GetHandlerProperty2,
            ffi::GetNumberOfMethods,
            ffi::GetMethodProperty,
            &mut bridge,
            &mut error,
        )
    };
    if status != ffi::PULP7Z_OK || bridge.is_null() {
        return Err(bridge_error(status, &error));
    }
    let bridge = NonNull::new(bridge).ok_or_else(|| Format7zError::Bridge {
        status: ffi::PULP7Z_NATIVE_ERROR,
        message: "bridge returned a null handle".to_owned(),
    })?;
    Ok(NativeRuntime { bridge })
}

fn bridge_error(status: i32, error: &ffi::Pulp7zError) -> Format7zError {
    let message =
        error_message(error).unwrap_or_else(|| "native bridge initialization failed".to_owned());
    Format7zError::Bridge { status, message }
}

fn error_message(error: &ffi::Pulp7zError) -> Option<String> {
    let length = error
        .message
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(error.message.len());
    if length == 0 {
        None
    } else {
        Some(
            String::from_utf8_lossy(
                &error.message[..length]
                    .iter()
                    .map(|byte| *byte as u8)
                    .collect::<Vec<_>>(),
            )
            .into_owned(),
        )
    }
}

pub fn native_status_error(status: i32, error: &ffi::Pulp7zError, fallback: &str) -> ArchiveError {
    ArchiveError::native(
        status,
        error_message(error).unwrap_or_else(|| fallback.to_owned()),
    )
}
