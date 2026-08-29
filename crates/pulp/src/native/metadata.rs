//! Conversion of the bridge's short-lived native metadata into owned Rust data.

use std::ffi::c_void;
use std::slice;

use crate::{
    ArchiveFormatId, CompressionMethod, FormatCapability, FormatDescriptor, LicenseNotice,
    Signature,
};

use super::error::Format7zError;
use super::ffi;
use super::loader::NativeRuntime;

const ARC_FLAG_KEEP_NAME: u32 = 1 << 0;
const ARC_FLAG_NT_SECURE: u32 = 1 << 2;
const ARC_FLAG_FIND_SIGNATURE: u32 = 1 << 3;
const ARC_FLAG_MULTI_SIGNATURE: u32 = 1 << 4;
const ARC_FLAG_SYMLINKS: u32 = 1 << 10;
const ARC_FLAG_HARDLINKS: u32 = 1 << 11;

/// Metadata collected during provider initialization.
#[derive(Debug)]
pub struct RuntimeMetadata {
    pub formats: Vec<FormatDescriptor>,
    pub methods: Vec<CompressionMethod>,
}

#[derive(Debug)]
struct OwnedFormat {
    index: u32,
    class_id: Option<[u8; 16]>,
    flags: u32,
    update: bool,
    name: String,
    extensions: Vec<String>,
    add_extensions: Vec<String>,
    signature_offset: u32,
    signature: Vec<u8>,
    multi_signature: Vec<u8>,
}

#[derive(Debug)]
struct OwnedMethod {
    index: u32,
    id: Option<[u8; 16]>,
    decoder: bool,
    encoder: bool,
    name: String,
    description: String,
}

pub fn collect(runtime: &NativeRuntime) -> Result<RuntimeMetadata, Format7zError> {
    let mut owned_formats = Vec::new();
    let mut format_error = ffi::Pulp7zError::default();
    let format_status = unsafe {
        ffi::pulp7z_bridge_enumerate_formats(
            runtime.bridge(),
            collect_format,
            (&mut owned_formats as *mut Vec<OwnedFormat>).cast::<c_void>(),
            &mut format_error,
        )
    };
    if format_status != ffi::PULP7Z_OK {
        return Err(Format7zError::Bridge {
            status: format_status,
            message: format_error_message(&format_error, "format enumeration failed"),
        });
    }

    let mut owned_methods = Vec::new();
    let mut method_error = ffi::Pulp7zError::default();
    let method_status = unsafe {
        ffi::pulp7z_bridge_enumerate_methods(
            runtime.bridge(),
            collect_method,
            (&mut owned_methods as *mut Vec<OwnedMethod>).cast::<c_void>(),
            &mut method_error,
        )
    };
    if method_status != ffi::PULP7Z_OK {
        return Err(Format7zError::Bridge {
            status: method_status,
            message: format_error_message(&method_error, "method enumeration failed"),
        });
    }

    let methods = build_methods(owned_methods);
    let formats = build_formats(owned_formats, &methods);
    Ok(RuntimeMetadata { formats, methods })
}

unsafe extern "C" fn collect_format(user: *mut c_void, info: *const ffi::Pulp7zFormatInfo) -> i32 {
    if user.is_null() || info.is_null() {
        return ffi::PULP7Z_INVALID_ARGUMENT;
    }
    let Some(info) = (unsafe { info.as_ref() }) else {
        return ffi::PULP7Z_INVALID_ARGUMENT;
    };
    let Some(formats) = (unsafe { user.cast::<Vec<OwnedFormat>>().as_mut() }) else {
        return ffi::PULP7Z_INVALID_ARGUMENT;
    };

    let name = copy_text(info.name, info.name_len);
    let extensions = split_extensions(copy_text(info.extension, info.extension_len));
    let add_extensions = split_extensions(copy_text(info.add_extension, info.add_extension_len));
    formats.push(OwnedFormat {
        index: info.index,
        class_id: (info.has_class_id != 0).then_some(info.class_id),
        flags: info.flags,
        update: info.update != 0,
        name,
        extensions,
        add_extensions,
        signature_offset: info.signature_offset,
        signature: copy_bytes(info.signature, info.signature_len),
        multi_signature: copy_bytes(info.multi_signature, info.multi_signature_len),
    });
    ffi::PULP7Z_OK
}

unsafe extern "C" fn collect_method(user: *mut c_void, info: *const ffi::Pulp7zMethodInfo) -> i32 {
    if user.is_null() || info.is_null() {
        return ffi::PULP7Z_INVALID_ARGUMENT;
    }
    let Some(info) = (unsafe { info.as_ref() }) else {
        return ffi::PULP7Z_INVALID_ARGUMENT;
    };
    let Some(methods) = (unsafe { user.cast::<Vec<OwnedMethod>>().as_mut() }) else {
        return ffi::PULP7Z_INVALID_ARGUMENT;
    };
    methods.push(OwnedMethod {
        index: info.index,
        id: (info.has_id != 0).then_some(info.id),
        decoder: info.has_decoder != 0 && info.decoder_assigned != 0,
        encoder: info.has_encoder != 0 && info.encoder_assigned != 0,
        name: copy_text(info.name, info.name_len),
        description: copy_text(info.description, info.description_len),
    });
    ffi::PULP7Z_OK
}

fn build_formats(owned: Vec<OwnedFormat>, methods: &[CompressionMethod]) -> Vec<FormatDescriptor> {
    let mut used_ids = Vec::<String>::new();
    owned
        .into_iter()
        .map(|format| {
            let base_id = if format.name.trim().is_empty() {
                format!("handler-{}", format.index)
            } else {
                format.name.to_ascii_lowercase()
            };
            let id = unique_id(&base_id, format.index, &mut used_ids);
            let mut capabilities = vec![
                FormatCapability::List,
                FormatCapability::Extract,
                FormatCapability::Test,
                FormatCapability::SeekableInput,
            ];
            let lower_name = format.name.to_ascii_lowercase();
            let writable = format.update && !matches!(lower_name.as_str(), "rar" | "rar5");
            if writable {
                capabilities.extend([
                    FormatCapability::Create,
                    FormatCapability::Update,
                    FormatCapability::SeekableOutput,
                ]);
            }
            if lower_name == "7z" {
                capabilities.extend([FormatCapability::HeaderEncryption, FormatCapability::Solid]);
            }
            if matches!(lower_name.as_str(), "7z" | "zip" | "rar" | "rar5") {
                capabilities.push(FormatCapability::Password);
            }
            let mut descriptor = FormatDescriptor::new(
                ArchiveFormatId::new(id),
                if format.name.is_empty() {
                    "Unnamed handler"
                } else {
                    format.name.as_str()
                },
                format.extensions.clone(),
                capabilities,
            );
            let signatures = signature_list(&format);
            descriptor.add_extensions = format.add_extensions.clone();
            if lower_name == "split"
                || descriptor
                    .add_extensions
                    .iter()
                    .any(|extension| extension.eq_ignore_ascii_case(".tar"))
            {
                descriptor.capabilities.push(FormatCapability::Transparent);
            }
            descriptor.class_id = format.class_id;
            descriptor.priority = format.index.min(u16::MAX as u32) as u16;
            descriptor.license = Some(license_for_name(&format.name));
            descriptor.methods = format_methods(&lower_name, methods);
            descriptor.signatures.extend(signatures);
            if format.flags & ARC_FLAG_KEEP_NAME != 0 {
                descriptor
                    .diagnostics
                    .push("handler preserves archive names".to_owned());
            }
            if format.flags & ARC_FLAG_NT_SECURE != 0 {
                descriptor
                    .diagnostics
                    .push("handler exposes NT security metadata".to_owned());
            }
            if format.flags & ARC_FLAG_HARDLINKS != 0 {
                descriptor
                    .diagnostics
                    .push("handler exposes hard-link metadata".to_owned());
            }
            if format.flags & ARC_FLAG_MULTI_SIGNATURE != 0 {
                descriptor
                    .diagnostics
                    .push("handler exposes multiple signatures".to_owned());
            }
            if format.flags & ARC_FLAG_FIND_SIGNATURE != 0 {
                descriptor
                    .diagnostics
                    .push("handler can search for an archive signature in a prefix".to_owned());
            }
            if format.flags & ARC_FLAG_SYMLINKS != 0 {
                descriptor
                    .diagnostics
                    .push("handler exposes symbolic-link metadata".to_owned());
            }
            if matches!(lower_name.as_str(), "rar" | "rar5") {
                descriptor
                    .diagnostics
                    .push("RAR writing is disabled by the unRAR license".to_owned());
            }
            descriptor
        })
        .collect()
}

fn format_methods(name: &str, methods: &[CompressionMethod]) -> Vec<CompressionMethod> {
    let supported = match name {
        "7z" => [
            "copy",
            "deflate",
            "deflate64",
            "bzip2",
            "lzma",
            "lzma2",
            "ppmd",
            "zstd",
        ]
        .as_slice(),
        "zip" => [
            "copy",
            "deflate",
            "deflate64",
            "bzip2",
            "lzma",
            "ppmd",
            "zstd",
            "xz",
        ]
        .as_slice(),
        "gzip" => ["deflate"].as_slice(),
        "xz" => ["lzma2"].as_slice(),
        "lzma" => ["lzma"].as_slice(),
        _ => &[],
    };
    methods
        .iter()
        .filter(|method| method.can_encode && supported.contains(&method.id.as_str()))
        .cloned()
        .collect()
}

fn build_methods(owned: Vec<OwnedMethod>) -> Vec<CompressionMethod> {
    let mut used_ids = Vec::<String>::new();
    owned
        .into_iter()
        .map(|method| {
            let base_id = if method.name.trim().is_empty() {
                method
                    .id
                    .map(guid_id)
                    .unwrap_or_else(|| format!("method-{}", method.index))
            } else {
                method.name.to_ascii_lowercase()
            };
            let id = unique_id(&base_id, method.index, &mut used_ids);
            let display_name = if method.description.is_empty() {
                method.name
            } else {
                format!("{} — {}", method.name, method.description)
            };
            let mut result = CompressionMethod::new(id, display_name);
            result.can_decode = method.decoder;
            result.can_encode = method.encoder;
            result
        })
        .collect()
}

fn signature_list(format: &OwnedFormat) -> Vec<Signature> {
    let mut signatures = Vec::new();
    if !format.signature.is_empty() {
        signatures.push(Signature {
            offset: format.signature_offset as u64,
            bytes: format.signature.clone(),
        });
    }
    let mut cursor = 0usize;
    while cursor < format.multi_signature.len() {
        let length = format.multi_signature[cursor] as usize;
        cursor += 1;
        let Some(end) = cursor.checked_add(length) else {
            break;
        };
        if end > format.multi_signature.len() || length == 0 {
            break;
        }
        signatures.push(Signature {
            offset: format.signature_offset as u64,
            bytes: format.multi_signature[cursor..end].to_vec(),
        });
        cursor = end;
    }
    signatures
}

fn license_for_name(name: &str) -> LicenseNotice {
    if name.eq_ignore_ascii_case("rar") || name.to_ascii_lowercase().contains("rar") {
        let mut notice = LicenseNotice::new(
            "unrar-restriction",
            "RAR decoding is distributed under the unRAR source license; RAR creation is not provided.",
            "https://www.rarlab.com/rar_add.htm",
        );
        notice.restrictions.push(
            "Do not use this component to create RAR archives; review the unRAR license before redistribution."
                .to_owned(),
        );
        notice
    } else {
        LicenseNotice::new(
            "7zip-lgpl",
            "7-Zip components are distributed under the LGPL-2.1-or-later license.",
            "https://www.7-zip.org/license.txt",
        )
    }
}

fn unique_id(base: &str, index: u32, used: &mut Vec<String>) -> String {
    let mut candidate = base.trim().to_ascii_lowercase();
    if used.iter().any(|value| value == &candidate) {
        candidate = format!("{candidate}-{index}");
    }
    used.push(candidate.clone());
    candidate
}

fn guid_id(value: [u8; 16]) -> String {
    let mut result = String::with_capacity(32);
    for byte in value {
        result.push_str(&format!("{byte:02x}"));
    }
    result
}

fn split_extensions(value: String) -> Vec<String> {
    value
        .split(|character: char| character == ';' || character == ',' || character.is_whitespace())
        .filter(|extension| !extension.is_empty())
        .map(str::to_owned)
        .collect()
}

fn copy_text(pointer: *const std::ffi::c_char, length: u32) -> String {
    if pointer.is_null() || length == 0 {
        return String::new();
    }
    let bytes = unsafe { slice::from_raw_parts(pointer.cast::<u8>(), length as usize) };
    String::from_utf8_lossy(bytes).into_owned()
}

fn copy_bytes(pointer: *const u8, length: u32) -> Vec<u8> {
    if pointer.is_null() || length == 0 {
        return Vec::new();
    }
    unsafe { slice::from_raw_parts(pointer, length as usize).to_vec() }
}

fn format_error_message(error: &ffi::Pulp7zError, fallback: &str) -> String {
    let length = error
        .message
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(error.message.len());
    if length == 0 {
        fallback.to_owned()
    } else {
        String::from_utf8_lossy(
            &error.message[..length]
                .iter()
                .map(|byte| *byte as u8)
                .collect::<Vec<_>>(),
        )
        .into_owned()
    }
}
