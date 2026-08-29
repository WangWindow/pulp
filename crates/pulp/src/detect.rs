use std::io::{Read, SeekFrom};

use crate::error::ArchiveResult;
use crate::format::FormatDescriptor;

/// The bounded, format-independent hint produced before a provider opens an
/// archive. The fields intentionally contain owned strings rather than
/// `infer::Type`, keeping the core API independent of that implementation.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct DetectionHint {
    /// Extension taken from the caller's filename hint, without a leading dot.
    pub extension: Option<String>,
    /// Original filename extension retained for mismatch diagnostics.
    pub filename_extension: Option<String>,
    /// MIME type reported by the in-memory magic matcher.
    pub mime_type: Option<String>,
    /// Broad kind reported by the in-memory magic matcher.
    pub kind: Option<String>,
}

impl DetectionHint {
    /// Returns whether magic bytes produced a useful hint.
    #[must_use]
    pub fn has_magic(&self) -> bool {
        self.mime_type.is_some() || self.kind.is_some()
    }

    /// Returns a short human-readable description for diagnostics.
    #[must_use]
    pub fn summary(&self) -> String {
        let mut parts = Vec::new();
        if let Some(kind) = &self.kind {
            parts.push(kind.as_str());
        }
        if let Some(mime_type) = &self.mime_type {
            parts.push(mime_type.as_str());
        }
        if let Some(extension) = &self.extension {
            parts.push(extension.as_str());
        }
        parts.join(" / ")
    }
}

/// The maximum number of bytes read by [`sniff_prefix`].
pub const DEFAULT_SNIFF_BYTES: usize = 64 * 1024;

/// Reads a bounded prefix and produces an auxiliary magic/extension hint.
///
/// The stream position is restored even when the read fails. This function
/// deliberately accepts only a stream and a filename string: it never opens a
/// path and never makes a decoder decision.
pub fn sniff_prefix(
    input: &mut dyn crate::io::ReadSeek,
    filename_hint: Option<&str>,
) -> ArchiveResult<DetectionHint> {
    let position = input.stream_position()?;
    let mut prefix = Vec::with_capacity(DEFAULT_SNIFF_BYTES.min(4096));
    let mut buffer = [0_u8; 8192];
    let read_result = (|| {
        while prefix.len() < DEFAULT_SNIFF_BYTES {
            let remaining = DEFAULT_SNIFF_BYTES - prefix.len();
            let chunk_size = remaining.min(buffer.len());
            let read = Read::read(input, &mut buffer[..chunk_size])?;
            if read == 0 {
                break;
            }
            prefix.extend_from_slice(&buffer[..read]);
        }
        Ok::<(), std::io::Error>(())
    })();
    let restore_result = input.seek(SeekFrom::Start(position));
    read_result?;
    restore_result?;
    let bytes_read = prefix.len();

    let mut hint = DetectionHint {
        extension: filename_hint.and_then(extension_hint),
        ..DetectionHint::default()
    };
    hint.filename_extension = hint.extension.clone();
    if let Some(file_type) = infer::get(&prefix[..bytes_read]) {
        hint.mime_type = Some(file_type.mime_type().to_owned());
        hint.extension = Some(file_type.extension().to_owned());
        hint.kind = Some(format!("{:?}", file_type.matcher_type()).to_ascii_lowercase());
    }
    Ok(hint)
}

fn extension_hint(value: &str) -> Option<String> {
    let name = value.rsplit(['/', '\\']).next().unwrap_or(value);
    let extension = name.rsplit_once('.')?.1.trim();
    (!extension.is_empty()).then(|| extension.to_ascii_lowercase())
}

/// The method that produced a format candidate.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DetectionMethod {
    /// The selected Format7zF handler successfully opened the input.
    Provider,
    /// A handler signature matched the input bytes.
    Magic {
        /// Confidence from 0 to 100.
        confidence: u8,
    },
    /// A filename extension matched the descriptor.
    ExtensionHint,
}

/// One candidate returned by format detection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DetectionCandidate {
    /// Candidate descriptor.
    pub descriptor: FormatDescriptor,
    /// How it was identified.
    pub method: DetectionMethod,
}

/// The outcome of format detection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DetectionResult {
    /// One best candidate.
    Detected(Box<DetectionCandidate>),
    /// Several candidates share the best score.
    Ambiguous(Vec<DetectionCandidate>),
    /// No candidate matched.
    Unknown,
}

/// A descriptor-based detector used by native and non-native engines.
pub struct FormatDetector<'a> {
    formats: &'a [FormatDescriptor],
    max_probe_bytes: usize,
}

impl<'a> FormatDetector<'a> {
    /// Detects from an already prepared hint without reading a stream.
    pub fn detect_hint(&self, hint: &DetectionHint) -> DetectionResult {
        let Some(extension) = hint.extension.as_deref() else {
            return DetectionResult::Unknown;
        };
        let candidates = self
            .formats
            .iter()
            .filter(|descriptor| descriptor.matches_extension(extension))
            .map(|descriptor| DetectionCandidate {
                descriptor: descriptor.clone(),
                method: DetectionMethod::ExtensionHint,
            })
            .collect::<Vec<_>>();
        if candidates.is_empty() {
            DetectionResult::Unknown
        } else {
            select_candidates(candidates)
        }
    }
}

impl<'a> FormatDetector<'a> {
    /// Creates a detector over a runtime descriptor slice.
    #[must_use]
    pub fn new(formats: &'a [FormatDescriptor]) -> Self {
        Self {
            formats,
            max_probe_bytes: 64 * 1024,
        }
    }

    /// Sets the bounded probe window.
    #[must_use]
    pub fn with_max_probe_bytes(mut self, bytes: usize) -> Self {
        self.max_probe_bytes = bytes.max(1);
        self
    }

    /// Detects a format from a bounded prefix and optional path hint.
    pub fn detect<R: Read + ?Sized>(
        &self,
        input: &mut R,
        hint: Option<&str>,
    ) -> ArchiveResult<DetectionResult> {
        let mut prefix = Vec::new();
        input
            .take(self.max_probe_bytes as u64)
            .read_to_end(&mut prefix)?;

        let mut candidates = Vec::new();
        for descriptor in self.formats {
            let matched = descriptor.signatures.iter().any(|signature| {
                let start = signature.offset as usize;
                let end = start.saturating_add(signature.bytes.len());
                end <= prefix.len() && prefix[start..end] == signature.bytes
            });
            if matched {
                candidates.push(DetectionCandidate {
                    descriptor: descriptor.clone(),
                    method: DetectionMethod::Magic { confidence: 100 },
                });
            }
        }
        if !candidates.is_empty() {
            return Ok(select_candidates(candidates));
        }

        let Some(hint) = hint else {
            return Ok(DetectionResult::Unknown);
        };
        let candidates = self
            .formats
            .iter()
            .filter(|descriptor| descriptor.matches_extension(hint))
            .map(|descriptor| DetectionCandidate {
                descriptor: descriptor.clone(),
                method: DetectionMethod::ExtensionHint,
            })
            .collect::<Vec<_>>();
        if candidates.is_empty() {
            Ok(DetectionResult::Unknown)
        } else {
            Ok(select_candidates(candidates))
        }
    }
}

fn select_candidates(mut candidates: Vec<DetectionCandidate>) -> DetectionResult {
    candidates.sort_by_key(|candidate| std::cmp::Reverse(candidate_rank(candidate)));
    let best = candidate_rank(&candidates[0]);
    let count = candidates
        .iter()
        .take_while(|candidate| candidate_rank(candidate) == best)
        .count();
    if count == 1 {
        DetectionResult::Detected(Box::new(candidates.remove(0)))
    } else {
        DetectionResult::Ambiguous(candidates.into_iter().take(count).collect())
    }
}

fn candidate_rank(candidate: &DetectionCandidate) -> (u8, u16) {
    let confidence = match candidate.method {
        DetectionMethod::Provider => 200,
        DetectionMethod::Magic { confidence } => confidence,
        DetectionMethod::ExtensionHint => 0,
    };
    (confidence, candidate.descriptor.priority)
}

#[cfg(test)]
mod tests {
    use std::io::{Cursor, Read, Seek};

    use super::{DetectionResult, FormatDetector, sniff_prefix};
    use crate::{ArchiveFormatId, FormatCapability, FormatDescriptor, Signature};

    struct CountingReader {
        inner: Cursor<Vec<u8>>,
        bytes_read: usize,
    }

    impl Read for CountingReader {
        fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
            let read = self.inner.read(buffer)?;
            self.bytes_read += read;
            Ok(read)
        }
    }

    fn descriptor(id: &str, signature: &[u8]) -> FormatDescriptor {
        let mut descriptor =
            FormatDescriptor::new(ArchiveFormatId::new(id), id, [id], [FormatCapability::List]);
        descriptor.signatures.push(Signature {
            offset: 0,
            bytes: signature.to_vec(),
        });
        descriptor
    }

    #[test]
    fn detects_signature_before_extension() {
        let formats = [descriptor("zip", b"PK")];
        let result = FormatDetector::new(&formats)
            .detect(&mut &b"PK data"[..], Some("archive.bin"))
            .expect("detection should succeed");
        let DetectionResult::Detected(candidate) = result else {
            panic!("expected detected format");
        };
        assert_eq!(candidate.descriptor.id, ArchiveFormatId::new("zip"));
    }

    #[test]
    fn detects_extension_hint_when_no_signature_matches() {
        let formats = [descriptor("zip", b"PK")];
        let result = FormatDetector::new(&formats)
            .detect(&mut &b"not zip"[..], Some("archive.ZIP"))
            .expect("detection should succeed");
        assert!(matches!(result, DetectionResult::Detected(_)));
    }

    #[test]
    fn detection_reads_only_the_configured_probe_window() {
        let mut reader = CountingReader {
            inner: Cursor::new(vec![0_u8; 32 * 1024]),
            bytes_read: 0,
        };
        let result = FormatDetector::new(&[])
            .with_max_probe_bytes(4096)
            .detect(&mut reader, None)
            .expect("bounded detection should succeed");

        assert!(matches!(result, DetectionResult::Unknown));
        assert_eq!(reader.bytes_read, 4096);
    }

    #[test]
    fn infer_is_only_a_magic_hint_and_wins_over_a_wrong_suffix() {
        let samples = [
            (&b"PK\x03\x04archive"[..], "zip"),
            (&b"7z\xBC\xAF\x27\x1Carchive"[..], "7z"),
            (&b"Rar!\x1A\x07\x00archive"[..], "rar"),
            (&b"Rar!\x1A\x07\x01\x00archive"[..], "rar"),
            (&[0_u8; 512][..], "tar"),
            (&b"\x1F\x8B\x08archive"[..], "gz"),
            (&b"BZh9archive"[..], "bz2"),
            (&b"\xFD7zXZ\x00archive"[..], "xz"),
            (&b"\x28\xB5\x2F\xFDarchive"[..], "zst"),
        ];

        for (bytes, extension) in samples {
            let mut bytes = bytes.to_vec();
            if extension == "tar" {
                bytes[257..262].copy_from_slice(b"ustar");
            }
            let mut input = Cursor::new(bytes);
            let hint = sniff_prefix(&mut input, Some("renamed.bin"))
                .expect("in-memory sniffing should succeed");
            assert_eq!(hint.extension.as_deref(), Some(extension));
            assert_eq!(hint.kind.as_deref(), Some("archive"));
            assert!(hint.has_magic());
            assert_eq!(input.stream_position().expect("position should work"), 0);
        }
    }

    #[test]
    fn unknown_magic_does_not_reject_a_file_or_erase_its_suffix_hint() {
        let mut input = Cursor::new(b"not an archive".to_vec());
        let hint = sniff_prefix(&mut input, Some("unknown.BIN"))
            .expect("unknown input is still a valid hinting operation");
        assert_eq!(hint.extension.as_deref(), Some("bin"));
        assert!(!hint.has_magic());
    }
}
