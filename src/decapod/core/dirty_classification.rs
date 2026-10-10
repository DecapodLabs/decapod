//! Deterministic classification of repository modifications.

use crate::core::bounded_process::{BoundedCommand, CONTROL_TIMEOUT};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;
use std::process::Command;

pub const DIRTY_SCHEMA_VERSION: &str = "1.0.0";

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum DirtyFileClass {
    UserAuthored,
    GovernanceTracked,
    DeterministicProjection,
    RuntimeEphemeral,
    BackupTemporary,
    PreExistingUnrelated,
    Unknown,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct DirtyFile {
    pub path: String,
    pub status: String,
    pub class: DirtyFileClass,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct DirtyGroup {
    pub class: DirtyFileClass,
    pub count: usize,
    pub files: Vec<String>,
    pub limit: Option<usize>,
    pub state: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct DirtyClassification {
    pub schema_version: String,
    pub kind: String,
    pub files: Vec<DirtyFile>,
    pub groups: Vec<DirtyGroup>,
    pub blocked: bool,
    pub blocker_classes: Vec<DirtyFileClass>,
}

pub fn classify(
    repo_root: &Path,
    max_user_authored: usize,
) -> Result<DirtyClassification, std::io::Error> {
    classify_with_pre_existing(repo_root, max_user_authored, &[])
}

pub fn classify_with_pre_existing(
    repo_root: &Path,
    max_user_authored: usize,
    pre_existing: &[String],
) -> Result<DirtyClassification, std::io::Error> {
    let output = Command::new("git")
        .current_dir(repo_root)
        .args(["status", "--porcelain=v1", "--untracked-files=all"])
        .bounded_output(CONTROL_TIMEOUT)?;
    if !output.status.success() {
        return Err(std::io::Error::other("git status --porcelain=v1 failed"));
    }
    let mut files = String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(parse_status_line)
        .map(|(status, path)| DirtyFile {
            class: classify_path(&path, pre_existing),
            path,
            status,
        })
        .collect::<Vec<_>>();
    files.sort_by(|left, right| {
        left.path
            .cmp(&right.path)
            .then(left.status.cmp(&right.status))
    });

    let mut grouped = BTreeMap::<DirtyFileClass, Vec<String>>::new();
    for file in &files {
        grouped
            .entry(file.class)
            .or_default()
            .push(file.path.clone());
    }
    let mut groups = grouped
        .into_iter()
        .map(|(class, mut paths)| {
            paths.sort();
            let limit = (class == DirtyFileClass::UserAuthored).then_some(max_user_authored);
            let state = match class {
                DirtyFileClass::UserAuthored if paths.len() > max_user_authored => "blocked",
                DirtyFileClass::Unknown => "blocked",
                DirtyFileClass::GovernanceTracked
                | DirtyFileClass::DeterministicProjection
                | DirtyFileClass::RuntimeEphemeral
                | DirtyFileClass::BackupTemporary
                | DirtyFileClass::PreExistingUnrelated => "ignored",
                DirtyFileClass::UserAuthored => "within_limit",
            };
            DirtyGroup {
                class,
                count: paths.len(),
                files: paths,
                limit,
                state: state.to_string(),
            }
        })
        .collect::<Vec<_>>();
    groups.sort_by_key(|group| group.class);
    let blocker_classes = groups
        .iter()
        .filter(|group| group.state == "blocked")
        .map(|group| group.class)
        .collect::<Vec<_>>();
    Ok(DirtyClassification {
        schema_version: DIRTY_SCHEMA_VERSION.to_string(),
        kind: "dirty_file_classification".to_string(),
        files,
        blocked: !blocker_classes.is_empty(),
        blocker_classes,
        groups,
    })
}

/// Inventory dirty source paths without recursively traversing ignored build/workspace trees.
/// Uses NUL-delimited Git paths so quotes, newlines and rename source names are unambiguous.
pub fn classify_isolation(repo_root: &Path) -> Result<Vec<DirtyFile>, std::io::Error> {
    let output = Command::new("git")
        .current_dir(repo_root)
        .env("GIT_OPTIONAL_LOCKS", "0")
        .args([
            "status",
            "--porcelain=v1",
            "-z",
            "--untracked-files=normal",
            "--ignored=matching",
        ])
        .bounded_output(CONTROL_TIMEOUT)?;
    if !output.status.success() {
        return Err(std::io::Error::other(
            "git source inventory failed; workspace setup refused",
        ));
    }
    let mut entries = output
        .stdout
        .split(|byte| *byte == 0)
        .filter(|entry| !entry.is_empty());
    let mut files = Vec::new();
    while let Some(entry) = entries.next() {
        if entry.len() < 4 || entry[2] != b' ' {
            return Err(std::io::Error::other("invalid Git status record"));
        }
        let status = String::from_utf8(entry[..2].to_vec()).map_err(std::io::Error::other)?;
        let path = String::from_utf8(entry[3..].to_vec()).map_err(std::io::Error::other)?;
        files.push(DirtyFile {
            class: classify_path(&path, &[]),
            path,
            status: status.clone(),
        });
        if status.contains(['R', 'C']) {
            let source = entries
                .next()
                .ok_or_else(|| std::io::Error::other("missing rename source"))?;
            let path = String::from_utf8(source.to_vec()).map_err(std::io::Error::other)?;
            files.push(DirtyFile {
                class: classify_path(&path, &[]),
                path,
                status,
            });
        }
    }
    files.sort_by(|left, right| {
        left.path
            .cmp(&right.path)
            .then(left.status.cmp(&right.status))
    });
    files.dedup();
    Ok(files)
}

pub fn classify_path(path: &str, pre_existing: &[String]) -> DirtyFileClass {
    if pre_existing.iter().any(|candidate| candidate == path) {
        return DirtyFileClass::PreExistingUnrelated;
    }
    if path.ends_with(".before_revert")
        || path.ends_with(".bak")
        || path.ends_with(".tmp")
        || path.contains("/.tmp-")
    {
        return DirtyFileClass::BackupTemporary;
    }
    if path.starts_with(".decapod/governance/") {
        return DirtyFileClass::GovernanceTracked;
    }
    if path.starts_with(".decapod/managed/") || path.starts_with(".decapod/generated/") {
        return DirtyFileClass::DeterministicProjection;
    }
    if path == ".decapod/README.md" {
        return DirtyFileClass::DeterministicProjection;
    }
    if path.starts_with(".decapod/data/") || path.starts_with(".decapod/workspaces/") {
        return DirtyFileClass::RuntimeEphemeral;
    }
    if path.starts_with(".decapod/") {
        if path == ".decapod/config.toml" || path == ".decapod/OVERRIDE.md" {
            return DirtyFileClass::UserAuthored;
        }
        return DirtyFileClass::Unknown;
    }
    DirtyFileClass::UserAuthored
}

fn parse_status_line(line: &str) -> Option<(String, String)> {
    if line.len() < 4 {
        return None;
    }
    let status = line[..2].to_string();
    let raw_path = line[3..].trim();
    let path = raw_path
        .rsplit_once(" -> ")
        .map(|(_, new)| new)
        .unwrap_or(raw_path);
    let path = path.trim_matches('"');
    (!path.is_empty()).then(|| (status, path.to_string()))
}
