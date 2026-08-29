//! Private fixed-width declarations for the C++ bridge.

use std::ffi::{c_char, c_void};

#[repr(C)]
pub struct Pulp7zBridge {
    _private: [u8; 0],
}

pub type CreateObjectFn = unsafe extern "C" fn(
    class_id: *const c_void,
    interface_id: *const c_void,
    object: *mut *mut c_void,
) -> i32;
pub type GetNumberOfFormatsFn = unsafe extern "C" fn(number: *mut u32) -> i32;
pub type GetHandlerPropertyFn = unsafe extern "C" fn(property_id: u32, value: *mut c_void) -> i32;
pub type GetHandlerProperty2Fn =
    unsafe extern "C" fn(index: u32, property_id: u32, value: *mut c_void) -> i32;
pub type GetNumberOfMethodsFn = unsafe extern "C" fn(number: *mut u32) -> i32;
pub type GetMethodPropertyFn =
    unsafe extern "C" fn(index: u32, property_id: u32, value: *mut c_void) -> i32;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct Pulp7zError {
    pub status: i32,
    pub message: [c_char; 512],
}

impl Default for Pulp7zError {
    fn default() -> Self {
        Self {
            status: 0,
            message: [0; 512],
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct Pulp7zFormatInfo {
    pub index: u32,
    pub class_id: [u8; 16],
    pub has_class_id: u8,
    pub flags: u32,
    pub time_flags: u32,
    pub signature_offset: u32,
    pub update: u8,
    pub keep_name: u8,
    pub alt_streams: u8,
    pub nt_secure: u8,
    pub name: *const c_char,
    pub name_len: u32,
    pub extension: *const c_char,
    pub extension_len: u32,
    pub add_extension: *const c_char,
    pub add_extension_len: u32,
    pub signature: *const u8,
    pub signature_len: u32,
    pub multi_signature: *const u8,
    pub multi_signature_len: u32,
}

pub type FormatCallback =
    unsafe extern "C" fn(user: *mut c_void, info: *const Pulp7zFormatInfo) -> i32;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct Pulp7zMethodInfo {
    pub index: u32,
    pub id: [u8; 16],
    pub has_id: u8,
    pub decoder: [u8; 16],
    pub has_decoder: u8,
    pub encoder: [u8; 16],
    pub has_encoder: u8,
    pub name: *const c_char,
    pub name_len: u32,
    pub description: *const c_char,
    pub description_len: u32,
    pub decoder_assigned: u8,
    pub encoder_assigned: u8,
    pub is_filter: u8,
}

pub type MethodCallback =
    unsafe extern "C" fn(user: *mut c_void, info: *const Pulp7zMethodInfo) -> i32;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct Pulp7zEntryInfo {
    pub index: u32,
    pub path: *const c_char,
    pub path_len: u32,
    pub is_dir: u8,
    pub encrypted: u8,
    pub link_kind: u8,
    pub has_size: u8,
    pub has_pack_size: u8,
    pub has_mtime: u8,
    pub has_attrib: u8,
    pub has_posix_attrib: u8,
    pub has_crc: u8,
    pub size: u64,
    pub pack_size: u64,
    pub mtime_unix_ns: i64,
    pub attrib: u32,
    pub posix_attrib: u32,
    pub crc: u32,
    pub method: *const c_char,
    pub method_len: u32,
    pub link_target: *const c_char,
    pub link_target_len: u32,
}

pub type EntryCallback =
    unsafe extern "C" fn(user: *mut c_void, info: *const Pulp7zEntryInfo) -> i32;

pub type ReadCallback =
    unsafe extern "C" fn(user: *mut c_void, data: *mut u8, size: u32, processed: *mut u32) -> i32;
pub type SeekCallback =
    unsafe extern "C" fn(user: *mut c_void, offset: i64, origin: u32, position: *mut u64) -> i32;
pub type WriteCallback =
    unsafe extern "C" fn(user: *mut c_void, data: *const u8, size: u32, processed: *mut u32) -> i32;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct Pulp7zInputCallbacks {
    pub user: *mut c_void,
    pub read: Option<ReadCallback>,
    pub seek: Option<SeekCallback>,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct Pulp7zOutputCallbacks {
    pub user: *mut c_void,
    pub write: Option<WriteCallback>,
    pub seek: Option<SeekCallback>,
}

pub type ProgressCallback =
    unsafe extern "C" fn(user: *mut c_void, total: u64, completed: u64, phase: u32) -> i32;
pub type PasswordCallback = unsafe extern "C" fn(
    user: *mut c_void,
    reason: u32,
    attempt: u32,
    password: *mut u8,
    capacity: u32,
    length: *mut u32,
) -> i32;

pub type VolumeCloseCallback = unsafe extern "C" fn(user: *mut c_void);

#[repr(C)]
#[derive(Clone, Copy)]
pub struct Pulp7zVolumeCallbacks {
    pub user: *mut c_void,
    pub read: Option<ReadCallback>,
    pub seek: Option<SeekCallback>,
    pub close: Option<VolumeCloseCallback>,
}

pub type OpenVolumeCallback = unsafe extern "C" fn(
    user: *mut c_void,
    name: *const c_char,
    name_len: u32,
    callbacks: *mut Pulp7zVolumeCallbacks,
) -> i32;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct Pulp7zOpenCallbacks {
    pub user: *mut c_void,
    pub progress: Option<ProgressCallback>,
    pub password: Option<PasswordCallback>,
    pub volume: Option<OpenVolumeCallback>,
    pub archive_name: *const c_char,
    pub archive_name_len: u32,
}

pub type ExtractBeginCallback = unsafe extern "C" fn(
    user: *mut c_void,
    info: *const Pulp7zEntryInfo,
    ask_mode: u32,
    decision: *mut u32,
) -> i32;
pub type ExtractWriteCallback =
    unsafe extern "C" fn(user: *mut c_void, data: *const u8, size: u32, processed: *mut u32) -> i32;
pub type ExtractFinishCallback = unsafe extern "C" fn(
    user: *mut c_void,
    info: *const Pulp7zEntryInfo,
    operation_result: i32,
    bytes: u64,
) -> i32;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct Pulp7zExtractCallbacks {
    pub user: *mut c_void,
    pub progress: Option<ProgressCallback>,
    pub password: Option<PasswordCallback>,
    pub volume: Option<OpenVolumeCallback>,
    pub archive_name: *const c_char,
    pub archive_name_len: u32,
    pub begin: Option<ExtractBeginCallback>,
    pub write: Option<ExtractWriteCallback>,
    pub finish: Option<ExtractFinishCallback>,
}

pub type SourceEntryCallback =
    unsafe extern "C" fn(user: *mut c_void, index: u32, info: *mut Pulp7zEntryInfo) -> i32;
pub type SourceReadCallback = unsafe extern "C" fn(
    user: *mut c_void,
    index: u32,
    data: *mut u8,
    size: u32,
    processed: *mut u32,
) -> i32;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct Pulp7zSourceCallbacks {
    pub user: *mut c_void,
    pub count: u32,
    pub entry: Option<SourceEntryCallback>,
    pub read: Option<SourceReadCallback>,
    pub progress: Option<ProgressCallback>,
    pub password: Option<PasswordCallback>,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct Pulp7zUpdateOptions {
    pub method: *const c_char,
    pub method_len: u32,
    pub level: i32,
    pub solid: i32,
    pub header_encryption: i32,
}

pub const PULP7Z_OK: i32 = 0;
pub const PULP7Z_INVALID_ARGUMENT: i32 = -1;
pub const PULP7Z_CALLBACK_ERROR: i32 = -2;
pub const PULP7Z_PASSWORD_DECLINED: i32 = -3;
pub const PULP7Z_NATIVE_ERROR: i32 = -4;
pub const PULP7Z_STREAM_UNAVAILABLE: i32 = -5;

#[link(name = "pulp_7z_sdk", kind = "static", modifiers = "+whole-archive")]
unsafe extern "C" {
    pub fn CreateObject(
        class_id: *const c_void,
        interface_id: *const c_void,
        object: *mut *mut c_void,
    ) -> i32;
    pub fn GetNumberOfFormats(number: *mut u32) -> i32;
    pub fn GetHandlerProperty(property_id: u32, value: *mut c_void) -> i32;
    pub fn GetHandlerProperty2(index: u32, property_id: u32, value: *mut c_void) -> i32;
    pub fn GetNumberOfMethods(number: *mut u32) -> i32;
    pub fn GetMethodProperty(index: u32, property_id: u32, value: *mut c_void) -> i32;
}

unsafe extern "C" {
    pub fn pulp7z_bridge_create(
        create_object: CreateObjectFn,
        get_number_of_formats: GetNumberOfFormatsFn,
        get_handler_property: GetHandlerPropertyFn,
        get_handler_property2: GetHandlerProperty2Fn,
        get_number_of_methods: GetNumberOfMethodsFn,
        get_method_property: GetMethodPropertyFn,
        out_bridge: *mut *mut Pulp7zBridge,
        out_error: *mut Pulp7zError,
    ) -> i32;

    pub fn pulp7z_bridge_destroy(bridge: *mut Pulp7zBridge);

    pub fn pulp7z_bridge_enumerate_formats(
        bridge: *mut Pulp7zBridge,
        callback: FormatCallback,
        user: *mut c_void,
        out_error: *mut Pulp7zError,
    ) -> i32;

    pub fn pulp7z_bridge_enumerate_methods(
        bridge: *mut Pulp7zBridge,
        callback: MethodCallback,
        user: *mut c_void,
        out_error: *mut Pulp7zError,
    ) -> i32;

    pub fn pulp7z_bridge_list(
        bridge: *mut Pulp7zBridge,
        class_id: *const u8,
        input: *const Pulp7zInputCallbacks,
        open_callbacks: *const Pulp7zOpenCallbacks,
        callback: EntryCallback,
        user: *mut c_void,
        out_error: *mut Pulp7zError,
    ) -> i32;

    pub fn pulp7z_bridge_probe(
        bridge: *mut Pulp7zBridge,
        class_id: *const u8,
        input: *const Pulp7zInputCallbacks,
        open_callbacks: *const Pulp7zOpenCallbacks,
        out_error: *mut Pulp7zError,
    ) -> i32;

    pub fn pulp7z_bridge_copy_entry(
        bridge: *mut Pulp7zBridge,
        class_id: *const u8,
        input: *const Pulp7zInputCallbacks,
        open_callbacks: *const Pulp7zOpenCallbacks,
        index: u32,
        output: *const Pulp7zOutputCallbacks,
        out_error: *mut Pulp7zError,
    ) -> i32;

    pub fn pulp7z_bridge_extract(
        bridge: *mut Pulp7zBridge,
        class_id: *const u8,
        input: *const Pulp7zInputCallbacks,
        indices: *const u32,
        index_count: u32,
        test_mode: i32,
        callbacks: *const Pulp7zExtractCallbacks,
        out_error: *mut Pulp7zError,
    ) -> i32;

    pub fn pulp7z_bridge_update(
        bridge: *mut Pulp7zBridge,
        class_id: *const u8,
        output: *const Pulp7zOutputCallbacks,
        source: *const Pulp7zSourceCallbacks,
        options: *const Pulp7zUpdateOptions,
        out_error: *mut Pulp7zError,
    ) -> i32;
}
