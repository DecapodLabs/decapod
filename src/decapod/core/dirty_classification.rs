//! Deterministic classification of repository modifications.

use serde::Serialize;
use std::collections::BTreeMap;
use std::path::Path;
use std::process::Command;

pub const DIRTY_SCHEMA_VERSION: &str = "1.0.0";

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq, PartialOrd, Ord)]
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

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DirtyFile {
    pub path: String,
    pub status: String,
    pub class: DirtyFileClass,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DirtyGroup {
    pub class: DirtyFileClass,
    pub count: usize,
    pub files: Vec<String>,
    pub limit: Option<usize>,
    pub state: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
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
        .output()?;
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
