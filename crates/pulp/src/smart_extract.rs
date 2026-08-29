use std::collections::HashSet;

use crate::{ArchiveEntry, ArchiveError, ArchiveResult, EntryId, EntryKind, EntryName};

/// Where an extraction operation should place the archive's entries.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExtractDestination {
    /// Choose a Bandizip-style destination from the archive contents.
    Smart,
    /// Use a caller-selected, already validated display name.
    Explicit(String),
}

/// How a caller wants existing names to be handled during extraction.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum CollisionPolicy {
    /// Ask the UI or caller when a collision is found.
    #[default]
    Ask,
    /// Keep the existing destination and skip the entry.
    Skip,
    /// Replace the existing destination after policy checks.
    Overwrite,
    /// Pick a free suffixed name.
    AutoRename,
}

/// A non-fatal condition found while building an extraction plan.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PolicyWarning {
    /// Stable warning code for UI and CLI filtering.
    pub code: String,
    /// Human-readable explanation.
    pub message: String,
    /// Related entry, when the warning is entry-specific.
    pub entry: Option<EntryId>,
}

impl PolicyWarning {
    fn new(code: impl Into<String>, message: impl Into<String>, entry: Option<EntryId>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            entry,
        }
    }
}

/// An entry and its archive-relative destination name.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlannedEntry {
    /// Original archive entry metadata.
    pub entry: ArchiveEntry,
    /// Destination name relative to the chosen extraction root.
    pub destination_name: EntryName,
}

/// A preflight result for a smart extraction operation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExtractPlan {
    /// Directory name selected relative to the caller's destination parent.
    /// An empty string means the caller's current directory.
    pub destination_name: String,
    /// Entries mapped into that destination.
    pub entries: Vec<PlannedEntry>,
    /// Non-fatal information the caller may show before execution.
    pub warnings: Vec<PolicyWarning>,
    /// Collision behavior selected for execution.
    pub collision_policy: CollisionPolicy,
}

impl ExtractPlan {
    /// Returns the number of regular files in this plan.
    #[must_use]
    pub fn file_count(&self) -> usize {
        self.entries
            .iter()
            .filter(|planned| planned.entry.kind == EntryKind::File)
            .count()
    }

    /// Returns the sum of known unpacked bytes.
    #[must_use]
    pub fn declared_bytes(&self) -> u64 {
        self.entries
            .iter()
            .filter_map(|planned| planned.entry.size)
            .fold(0, u64::saturating_add)
    }
}

/// Supplies names already present in the caller's destination directory.
pub trait ExistingNameSet {
    /// Returns whether a name is occupied.
    fn contains(&self, name: &str) -> bool;
}

impl ExistingNameSet for HashSet<String> {
    fn contains(&self, name: &str) -> bool {
        HashSet::contains(self, name)
    }
}

impl ExistingNameSet for &[String] {
    fn contains(&self, name: &str) -> bool {
        self.iter().any(|candidate| candidate == name)
    }
}

/// Builds the default smart-extraction plan.
pub fn plan_smart_destination(
    archive_stem: &str,
    entries: &[ArchiveEntry],
    existing_names: &dyn ExistingNameSet,
) -> ArchiveResult<ExtractPlan> {
    plan_smart_destination_with_policy(
        archive_stem,
        entries,
        existing_names,
        CollisionPolicy::AutoRename,
    )
}

/// Builds a smart-extraction plan with an explicit collision policy.
pub fn plan_smart_destination_with_policy(
    archive_stem: &str,
    entries: &[ArchiveEntry],
    existing_names: &dyn ExistingNameSet,
    collision_policy: CollisionPolicy,
) -> ArchiveResult<ExtractPlan> {
    validate_entries(entries)?;
    let stem = validate_stem(archive_stem)?;
    let destination_name = choose_destination(&stem, entries, existing_names, collision_policy)?;
    let prefix = (!destination_name.is_empty()).then_some(destination_name.as_str());
    let planned_entries = entries
        .iter()
        .map(|entry| {
            let destination = match prefix {
                Some(prefix) => EntryName::new(format!("{prefix}/{}", entry.name))?,
                None => entry.name.clone(),
            };
            Ok(PlannedEntry {
                entry: entry.clone(),
                destination_name: destination,
            })
        })
        .collect::<ArchiveResult<Vec<_>>>()?;

    let mut warnings = Vec::new();
    if entries.is_empty() {
        warnings.push(PolicyWarning::new(
            "empty-archive",
            "The archive does not contain extractable entries",
            None,
        ));
    }
    if entries.iter().any(|entry| entry.kind != EntryKind::File) {
        warnings.push(PolicyWarning::new(
            "non-regular-entry",
            "The archive contains directories or links; extraction safety policy still applies",
            None,
        ));
    }
    Ok(ExtractPlan {
        destination_name,
        entries: planned_entries,
        warnings,
        collision_policy,
    })
}

/// Builds a plan rooted at a caller-selected relative display name.
pub fn plan_explicit_destination(
    destination_name: &str,
    entries: &[ArchiveEntry],
    collision_policy: CollisionPolicy,
) -> ArchiveResult<ExtractPlan> {
    validate_entries(entries)?;
    let destination_name = if destination_name.trim().is_empty() {
        String::new()
    } else {
        EntryName::new(destination_name)?.into_string()
    };
    let prefix = (!destination_name.is_empty()).then_some(destination_name.as_str());
    let planned_entries = entries
        .iter()
        .map(|entry| {
            let destination = match prefix {
                Some(prefix) => EntryName::new(format!("{prefix}/{}", entry.name))?,
                None => entry.name.clone(),
            };
            Ok(PlannedEntry {
                entry: entry.clone(),
                destination_name: destination,
            })
        })
        .collect::<ArchiveResult<Vec<_>>>()?;
    Ok(ExtractPlan {
        destination_name,
        entries: planned_entries,
        warnings: Vec::new(),
        collision_policy,
    })
}

fn validate_entries(entries: &[ArchiveEntry]) -> ArchiveResult<()> {
    let mut names = HashSet::with_capacity(entries.len());
    for entry in entries {
        if !names.insert(entry.name.as_str().to_owned()) {
            return Err(ArchiveError::PolicyViolation(format!(
                "duplicate archive entry: {}",
                entry.name
            )));
        }
        if entry.kind == EntryKind::Special {
            return Err(ArchiveError::PolicyViolation(format!(
                "special archive entry is not allowed: {}",
                entry.name
            )));
        }
    }
    Ok(())
}

fn validate_stem(value: &str) -> ArchiveResult<String> {
    let value = value.trim();
    if value.is_empty() {
        return Ok(String::from("archive"));
    }
    if value.contains('/') || value.contains('\\') {
        return Err(ArchiveError::invalid_input(
            "archive stem must not contain a path separator",
        ));
    }
    Ok(EntryName::new(value)?.into_string())
}

fn choose_destination(
    stem: &str,
    entries: &[ArchiveEntry],
    existing_names: &dyn ExistingNameSet,
    collision_policy: CollisionPolicy,
) -> ArchiveResult<String> {
    if entries.len() == 1 && entries[0].kind == EntryKind::File {
        return Ok(String::new());
    }

    let top_level = entries
        .iter()
        .filter_map(|entry| entry.name.as_str().split('/').next())
        .collect::<HashSet<_>>();
    let single_directory = top_level.len() == 1
        && entries
            .iter()
            .all(|entry| entry.name.as_str().split('/').next() == top_level.iter().next().copied());
    if single_directory {
        // The top-level directory is already present in each archive entry,
        // so extracting into the caller's current destination preserves it
        // without producing a duplicated directory component.
        return Ok(String::new());
    }

    let candidate = match collision_policy {
        CollisionPolicy::AutoRename => free_suffix(stem, existing_names),
        CollisionPolicy::Ask | CollisionPolicy::Skip | CollisionPolicy::Overwrite => {
            stem.to_owned()
        }
    };
    if matches!(collision_policy, CollisionPolicy::Ask) && existing_names.contains(&candidate) {
        return Err(ArchiveError::PolicyViolation(format!(
            "extraction destination already exists: {candidate}"
        )));
    }
    Ok(candidate)
}

fn free_suffix(stem: &str, existing_names: &dyn ExistingNameSet) -> String {
    if !existing_names.contains(stem) {
        return stem.to_owned();
    }
    for suffix in 2..=u32::MAX {
        let candidate = format!("{stem} ({suffix})");
        if !existing_names.contains(&candidate) {
            return candidate;
        }
    }
    format!("{stem} (2)")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{EntryId, EntryName};

    fn file(name: &str) -> ArchiveEntry {
        ArchiveEntry::file(
            EntryId::new(1),
            EntryName::new(name).expect("valid test name"),
            Some(4),
        )
    }

    fn directory(name: &str) -> ArchiveEntry {
        ArchiveEntry::new(
            EntryId::new(1),
            EntryName::new(name).expect("valid test name"),
            EntryKind::Directory,
        )
    }

    #[test]
    fn one_file_is_extracted_into_the_current_directory() {
        let plan = plan_smart_destination("report", &[file("report.txt")], &HashSet::new())
            .expect("plan should succeed");
        assert_eq!(plan.destination_name, "");
        assert_eq!(plan.entries[0].destination_name.as_str(), "report.txt");
    }

    #[test]
    fn one_top_level_directory_is_preserved() {
        let entries = vec![directory("project"), file("project/src/main.rs")];
        let plan = plan_smart_destination("archive", &entries, &HashSet::new())
            .expect("plan should succeed");
        assert_eq!(plan.destination_name, "");
        assert_eq!(
            plan.entries[1].destination_name.as_str(),
            "project/src/main.rs"
        );
    }

    #[test]
    fn multiple_top_level_entries_use_archive_stem() {
        let entries = vec![file("a.txt"), file("b.txt")];
        let plan = plan_smart_destination("archive", &entries, &HashSet::new())
            .expect("plan should succeed");
        assert_eq!(plan.destination_name, "archive");
        assert_eq!(plan.entries[1].destination_name.as_str(), "archive/b.txt");
    }

    #[test]
    fn existing_stem_gets_a_numbered_suffix() {
        let existing = HashSet::from([String::from("archive"), String::from("archive (2)")]);
        let plan = plan_smart_destination("archive", &[file("a.txt"), file("b.txt")], &existing)
            .expect("plan should succeed");
        assert_eq!(plan.destination_name, "archive (3)");
    }

    #[test]
    fn duplicate_entries_are_rejected_before_execution() {
        let entries = vec![file("same"), file("same")];
        let error = plan_smart_destination("archive", &entries, &HashSet::new())
            .expect_err("duplicates must be rejected");
        assert!(matches!(error, ArchiveError::PolicyViolation(_)));
    }

    #[test]
    fn invalid_stem_does_not_become_a_filesystem_path() {
        let error = plan_smart_destination("../outside", &[file("a")], &HashSet::new())
            .expect_err("path-like stems must be rejected");
        assert!(matches!(error, ArchiveError::InvalidInput(_)));
    }
}
