use std::sync::Mutex;

use crate::{CompressionMethod, EngineInfo, FormatDescriptor};

mod callbacks;
mod decoder;
mod encoder;
mod error;
mod ffi;
mod loader;
mod metadata;
mod streams;
mod temp;

pub use error::Format7zError;

/// A Format7zF-backed archive engine.
pub struct ArchiveEngine {
    runtime: Mutex<loader::NativeRuntime>,
    info: EngineInfo,
    formats: Vec<FormatDescriptor>,
    methods: Vec<CompressionMethod>,
}
