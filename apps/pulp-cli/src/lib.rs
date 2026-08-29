#![forbid(unsafe_code)]
#![doc = "Command-line archive application for Pulp."]

use std::fs::File;
use std::io::{Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use clap::{Parser, Subcommand, ValueEnum};
use pulp::{
    ArchiveEngine, ArchiveEntry, ArchiveError, ArchiveFormatId, ArchiveResult, AtomicFile,
    CreateOptions, EntryVisitor, FileSystemSink, FileSystemSource, FileVolumeProvider,
    FormatCapability, OperationContext, OperationKind, OverwritePolicy, Password, PasswordProvider,
    PasswordReason, PasswordRequest, ProgressEvent, TestOptions, plan_smart_destination,
    resolve_split_archive_path, sniff_prefix, validate_output_path,
};

/// Pulp archive command-line interface.
#[derive(Debug, Parser)]
#[command(
    name = "pulp",
    bin_name = "pulp",
    version,
    about = "Inspect, test, extract, and create archives with Format7zF",
    arg_required_else_help = true
)]
pub struct Cli {
    /// Prompt on the terminal when an archive requests a password.
    #[arg(long, global = true)]
    password_prompt: bool,
    #[command(subcommand)]
    command: Command,
}

/// How extraction handles an existing destination.
#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
enum OverwriteMode {
    /// Stop when a destination already exists.
    Error,
    /// Replace an existing regular file or link.
    Replace,
    /// Keep the existing destination and skip the archive entry.
    Skip,
}

impl From<OverwriteMode> for OverwritePolicy {
    fn from(mode: OverwriteMode) -> Self {
        match mode {
            OverwriteMode::Error => Self::Error,
            OverwriteMode::Replace => Self::Replace,
            OverwriteMode::Skip => Self::Skip,
        }
    }
}

/// An archive operation exposed by the command-line application.
#[derive(Debug, Subcommand)]
enum Command {
    /// Show handlers and compression methods exposed by Format7zF.
    Formats,
    /// Detect an archive format from its contents and filename hint.
    Detect { input: PathBuf },
    /// List archive entries and metadata.
    List {
        archive: PathBuf,
        #[arg(long, value_name = "FORMAT")]
        format: Option<String>,
    },
    /// Verify archive structure and file data.
    Test {
        archive: PathBuf,
        #[arg(long, value_name = "FORMAT")]
        format: Option<String>,
    },
    /// Extract an archive into a destination directory.
    Extract {
        archive: PathBuf,
        #[arg(short = 'o', long = "output", value_name = "DIR")]
        destination: PathBuf,
        #[arg(long, value_name = "FORMAT")]
        format: Option<String>,
        #[arg(long, value_enum, default_value_t = OverwriteMode::Error)]
        overwrite: OverwriteMode,
        #[arg(long)]
        smart: bool,
    },
    /// Create an archive from one or more files or directories.
    Create {
        archive: PathBuf,
        #[arg(required = true, num_args = 1..)]
        sources: Vec<PathBuf>,
        #[arg(long, value_name = "FORMAT")]
        format: Option<String>,
        #[arg(long, value_name = "0-9", value_parser = parse_compression_level)]
        level: Option<u32>,
        #[arg(long, value_name = "METHOD")]
        method: Option<String>,
    },
}

/// Executes one parsed command-line invocation.
pub fn run(cli: Cli) -> ArchiveResult<()> {
    let engine = ArchiveEngine::load()
        .map_err(|error| ArchiveError::backend("Format7zF", error.to_string()))?;
    let password_prompt = cli.password_prompt;
    match cli.command {
        Command::Formats => print_formats(&engine),
        Command::Detect { input } => detect_archive(&engine, &input),
        Command::List { archive, format } => {
            list_archive(&engine, &archive, format.as_deref(), password_prompt)
        }
        Command::Test { archive, format } => {
            test_archive(&engine, &archive, format.as_deref(), password_prompt)
        }
        Command::Extract {
            archive,
            destination,
            format,
            overwrite,
            smart,
        } => extract_archive(
            &engine,
            &archive,
            &destination,
            format.as_deref(),
            overwrite.into(),
            smart,
            password_prompt,
        ),
        Command::Create {
            archive,
            sources,
            format,
            level,
            method,
        } => create_archive(
            &engine,
            &archive,
            &sources,
            format.as_deref(),
            level,
            method,
            password_prompt,
        ),
    }
}

fn print_formats(engine: &ArchiveEngine) -> ArchiveResult<()> {
    println!(
        "Format7zF: embedded static provider ({} formats)",
        engine.formats().len()
    );
    for descriptor in engine.formats() {
        let capabilities = descriptor
            .capabilities
            .iter()
            .map(|capability| format!("{capability:?}"))
            .collect::<Vec<_>>()
            .join(",");
        println!(
            "{}\t{}\t[{}]\t{}",
            descriptor.id,
            descriptor.name,
            descriptor.extensions.join(","),
            capabilities
        );
    }
    println!("methods:");
    for method in engine.methods() {
        println!(
            "{}\tdecode={} encode={}\t{}",
            method.id, method.can_decode, method.can_encode, method.name
        );
    }
    Ok(())
}

fn detect_archive(engine: &ArchiveEngine, path: &Path) -> ArchiveResult<()> {
    let path = resolve_split_archive_path(path);
    let mut archive = File::open(&path)?;
    let filename_hint = path.to_string_lossy().into_owned();
    let hint = sniff_prefix(&mut archive, Some(&filename_hint))?;
    let context = archive_context(OperationKind::Detect, false, &path);
    match engine.detect(&mut archive, Some(&hint), &context)? {
        pulp::DetectionResult::Detected(candidate) => {
            println!("{}", candidate.descriptor.id);
            Ok(())
        }
        pulp::DetectionResult::Ambiguous(candidates) => Err(ArchiveError::invalid_input(format!(
            "archive format is ambiguous: {}",
            candidates
                .iter()
                .map(|candidate| candidate.descriptor.id.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        ))),
        pulp::DetectionResult::Unknown => Err(ArchiveError::UnsupportedFormat(format!(
            "could not detect archive format for {}",
            path.display()
        ))),
    }
}

fn list_archive(
    engine: &ArchiveEngine,
    path: &Path,
    format_hint: Option<&str>,
    password_prompt: bool,
) -> ArchiveResult<()> {
    let path = resolve_split_archive_path(path);
    let mut archive = File::open(&path)?;
    let context = archive_context(OperationKind::List, password_prompt, &path);
    let mut entries = PrintEntries;
    let report = if let Some(format) = format_hint {
        let format = require_format(engine, format)?;
        engine.list_format(&format, &mut archive, &mut entries, &context)?
    } else {
        engine.list(&mut archive, &context, &mut entries)?
    };
    eprintln!("{} entries", report.entries_seen);
    Ok(())
}

fn test_archive(
    engine: &ArchiveEngine,
    path: &Path,
    format_hint: Option<&str>,
    password_prompt: bool,
) -> ArchiveResult<()> {
    let path = resolve_split_archive_path(path);
    let mut archive = File::open(&path)?;
    let context = archive_context(OperationKind::Test, password_prompt, &path);
    let report = if let Some(format) = format_hint {
        let format = require_format(engine, format)?;
        engine.test_format(&format, &mut archive, &TestOptions::default(), &context)?
    } else {
        engine.test(&mut archive, &context, &TestOptions::default())?
    };
    println!(
        "verified: {} ({})",
        path.display(),
        report
            .format
            .unwrap_or_else(|| { ArchiveFormatId::new("unknown") })
    );
    Ok(())
}

fn extract_archive(
    engine: &ArchiveEngine,
    path: &Path,
    destination: &Path,
    format_hint: Option<&str>,
    overwrite: OverwritePolicy,
    smart: bool,
    password_prompt: bool,
) -> ArchiveResult<()> {
    let path = resolve_split_archive_path(path);
    let mut archive = File::open(&path)?;
    let context = archive_context(OperationKind::Extract, password_prompt, &path);
    let destination = if smart {
        let list_context = archive_context(OperationKind::List, password_prompt, &path);
        let mut collector = ArchiveEntryCollector::default();
        archive.seek(SeekFrom::Start(0))?;
        if let Some(format) = format_hint {
            let format = require_format(engine, format)?;
            engine.list_format(&format, &mut archive, &mut collector, &list_context)?;
        } else {
            engine.list(&mut archive, &list_context, &mut collector)?;
        }
        archive.seek(SeekFrom::Start(0))?;
        let existing_names = read_existing_names(destination)?;
        let plan =
            plan_smart_destination(&archive_stem(&path), &collector.entries, &existing_names)?;
        for warning in plan.warnings {
            context.report(ProgressEvent::Warning(warning.message));
        }
        destination.join(plan.destination_name)
    } else {
        destination.to_owned()
    };
    let mut sink = FileSystemSink::new(destination.clone()).with_overwrite(overwrite);
    archive.seek(SeekFrom::Start(0))?;
    if let Some(format) = format_hint {
        let format = require_format(engine, format)?;
        engine.extract_format(
            &format,
            &mut archive,
            &mut sink,
            &pulp::ExtractOptions::default(),
            &context,
        )?;
    } else {
        engine.extract(
            &mut archive,
            &context,
            &mut sink,
            &pulp::ExtractOptions::default(),
        )?;
    }
    sink.finalize()?;
    println!("extracted: {} -> {}", path.display(), destination.display());
    Ok(())
}

fn create_archive(
    engine: &ArchiveEngine,
    archive_path: &Path,
    sources: &[PathBuf],
    format_hint: Option<&str>,
    compression_level: Option<u32>,
    compression_method: Option<String>,
    password_prompt: bool,
) -> ArchiveResult<()> {
    for source in sources {
        validate_output_path(source, archive_path)?;
    }
    let format = resolve_output_format(engine, archive_path, format_hint)?;
    let mut source = FileSystemSource::from_paths(sources.iter().cloned())?;
    let mut archive = AtomicFile::create(archive_path)?;
    let result = engine.create(
        &format,
        &mut source,
        &mut archive,
        &CreateOptions {
            compression_method,
            compression_level,
            ..CreateOptions::default()
        },
        &operation_context(OperationKind::Create, password_prompt),
    );
    match result {
        Ok(report) => {
            archive.commit()?;
            println!(
                "created: {} ({format}, {} bytes)",
                archive_path.display(),
                report.bytes_written
            );
            Ok(())
        }
        Err(error) => Err(error),
    }
}

fn resolve_output_format(
    engine: &ArchiveEngine,
    path: &Path,
    explicit: Option<&str>,
) -> ArchiveResult<ArchiveFormatId> {
    if let Some(explicit) = explicit {
        return require_format(engine, explicit);
    }
    let hint = path.to_string_lossy();
    let mut matches = engine
        .formats()
        .iter()
        .filter(|descriptor| {
            descriptor.matches_primary_extension(&hint)
                && descriptor.supports(FormatCapability::Create)
        })
        .collect::<Vec<_>>();
    if matches.is_empty() {
        matches = engine
            .formats()
            .iter()
            .filter(|descriptor| {
                descriptor.matches_extension(&hint) && descriptor.supports(FormatCapability::Create)
            })
            .collect();
    }
    match matches.as_slice() {
        [descriptor] => Ok(descriptor.id.clone()),
        [] => Err(ArchiveError::UnsupportedFormat(format!(
            "no writable Format7zF handler matches {}",
            path.display()
        ))),
        _ => Err(ArchiveError::invalid_input(format!(
            "output format is ambiguous: {}",
            matches
                .iter()
                .map(|descriptor| descriptor.id.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        ))),
    }
}

fn require_format(engine: &ArchiveEngine, value: &str) -> ArchiveResult<ArchiveFormatId> {
    let format = ArchiveFormatId::new(value);
    if engine
        .formats()
        .iter()
        .any(|descriptor| descriptor.id == format)
    {
        Ok(format)
    } else {
        Err(ArchiveError::UnsupportedFormat(value.to_owned()))
    }
}

fn parse_compression_level(value: &str) -> Result<u32, String> {
    let level = value
        .parse::<u32>()
        .map_err(|_| format!("compression level must be an integer from 0 to 9, got {value:?}"))?;
    if level > 9 {
        return Err(format!(
            "compression level must be an integer from 0 to 9, got {level}"
        ));
    }
    Ok(level)
}

fn operation_context(operation: OperationKind, password_prompt: bool) -> OperationContext {
    let context = OperationContext::new().with_progress(move |event| match event {
        ProgressEvent::EntryStarted { name, .. } => eprintln!("{operation}: {name}"),
        ProgressEvent::Warning(message) => eprintln!("warning: {message}"),
        ProgressEvent::Started { .. }
        | ProgressEvent::Bytes { .. }
        | ProgressEvent::Finished(_) => {}
    });
    if password_prompt {
        context.with_password_provider(Arc::new(TerminalPasswordProvider::default()))
    } else {
        context
    }
}

fn archive_context(
    operation: OperationKind,
    password_prompt: bool,
    path: &Path,
) -> OperationContext {
    operation_context(operation, password_prompt)
        .with_volume_provider(Arc::new(FileVolumeProvider::new(path.to_owned())))
}

#[derive(Debug, Default)]
struct TerminalPasswordProvider {
    cached: Mutex<Option<Password>>,
}

impl PasswordProvider for TerminalPasswordProvider {
    fn request(&self, request: PasswordRequest) -> ArchiveResult<Option<Password>> {
        let mut cached = self
            .cached
            .lock()
            .map_err(|_| ArchiveError::Internal("password cache is poisoned".to_owned()))?;
        if request.reason == PasswordReason::Retry {
            cached.take();
        } else if let Some(password) = cached.as_ref() {
            return Ok(Some(password.clone()));
        }
        drop(cached);

        let prompt = format!(
            "password ({:?}, attempt {}): ",
            request.reason, request.attempt
        );
        let password = rpassword::prompt_password(prompt)?;
        let password = (!password.is_empty()).then(|| Password::new(password));
        if let Some(password) = password.as_ref() {
            let mut cached = self
                .cached
                .lock()
                .map_err(|_| ArchiveError::Internal("password cache is poisoned".to_owned()))?;
            *cached = Some(password.clone());
        }
        Ok(password)
    }
}

struct PrintEntries;

impl EntryVisitor for PrintEntries {
    fn visit(&mut self, entry: &ArchiveEntry) -> ArchiveResult<()> {
        println!(
            "{:?}\t{}\t{}",
            entry.kind,
            entry
                .size
                .map_or_else(|| "-".to_owned(), |size| size.to_string()),
            entry.name
        );
        Ok(())
    }
}

#[derive(Default)]
struct ArchiveEntryCollector {
    entries: Vec<ArchiveEntry>,
}

impl EntryVisitor for ArchiveEntryCollector {
    fn visit(&mut self, entry: &ArchiveEntry) -> ArchiveResult<()> {
        self.entries.push(entry.clone());
        Ok(())
    }
}

fn archive_stem(path: &Path) -> String {
    path.file_stem()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .unwrap_or("archive")
        .to_owned()
}

fn read_existing_names(path: &Path) -> ArchiveResult<std::collections::HashSet<String>> {
    let metadata = match std::fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Default::default()),
        Err(error) => return Err(error.into()),
    };
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(ArchiveError::PolicyViolation(format!(
            "extraction destination is not a directory: {}",
            path.display()
        )));
    }
    std::fs::read_dir(path)?
        .map(|entry| {
            entry
                .map(|entry| entry.file_name().to_string_lossy().into_owned())
                .map_err(ArchiveError::from)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::{Cli, Command, OverwriteMode, parse_compression_level};

    #[test]
    fn parses_list_command_with_a_path() {
        let cli = Cli::try_parse_from(["pulp", "list", "archive.zip"])
            .expect("list command should parse");
        assert!(matches!(cli.command, Command::List { .. }));
    }

    #[test]
    fn parses_multiple_create_sources() {
        let cli = Cli::try_parse_from(["pulp", "create", "archive.7z", "one", "two"])
            .expect("multiple sources should parse");
        let Command::Create { sources, .. } = cli.command else {
            panic!("expected create command");
        };
        assert_eq!(sources.len(), 2);
    }

    #[test]
    fn parses_extract_output_and_overwrite_options() {
        let cli = Cli::try_parse_from([
            "pulp",
            "extract",
            "archive.zip",
            "--output",
            "out",
            "--overwrite",
            "replace",
            "--smart",
        ])
        .expect("extract options should parse");
        let Command::Extract {
            destination,
            overwrite,
            smart,
            ..
        } = cli.command
        else {
            panic!("expected extract command");
        };
        assert_eq!(destination, std::path::PathBuf::from("out"));
        assert_eq!(overwrite, OverwriteMode::Replace);
        assert!(smart);
    }

    #[test]
    fn bounds_creation_compression_level() {
        assert_eq!(parse_compression_level("9"), Ok(9));
        assert!(parse_compression_level("10").is_err());
    }
}
