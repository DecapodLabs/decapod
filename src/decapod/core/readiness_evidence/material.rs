//! Read-only working-material capture. Like governance_document::tree_material,
//! material is a sorted path/mode/content map, but bytes come from the actual
//! worktree rather than its potentially stale index. Neither the user's index
//! nor Git's object store is changed. Capture before and after the entire run.
//!
//! All tracked/HEAD files and declared untracked inputs are bound. Undeclared
//! nonignored untracked files fail closed. Ignored build inputs must be declared
//! by the independently derived input inventory; ignored generated outputs are
//! not inputs. Submodules, non-UTF-8 paths, non-Unix modes and symlink ancestors
//! fail explicitly until a faithful material adapter is implemented.

use super::{digest, valid_path};
use serde::{Deserialize, Serialize};
#[cfg(unix)]
use sha2::{Digest, Sha256};
#[cfg(unix)]
use std::fs;
use std::{
    collections::{BTreeMap, BTreeSet},
    io,
    path::Path,
    process::Command,
};

/// Fixed exclusions, not caller-supplied globs. No source/config/spec/workflow/tool
/// path is excluded. Durable policy is separately bound by ExpectedPlan. These
/// are the existing receipt and runtime bookkeeping locations, never assurance.
pub const BOOKKEEPING_FILE: &str = ".decapod/governance.json";
pub const BOOKKEEPING_DIRECTORIES: &[&str] = &[
    ".decapod/data",
    ".decapod/workspaces",
    ".decapod/governance",
];

pub fn is_bookkeeping(path: &str) -> bool {
    path == BOOKKEEPING_FILE
        || BOOKKEEPING_DIRECTORIES
            .iter()
            .any(|directory| path == *directory || path.starts_with(&format!("{directory}/")))
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct MaterialEntry {
    /// Git-compatible 100644/100755/120000; 000000 is a deletion tombstone.
    pub mode: String,
    pub content_digest: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct WorkingMaterial {
    pub head_revision: String,
    pub work_branch: String,
    pub declared_untracked: BTreeSet<String>,
    pub files: BTreeMap<String, MaterialEntry>,
}

impl WorkingMaterial {
    pub fn digest(&self) -> Result<String, serde_json::Error> {
        digest(self)
    }
    /// Source branch/head are separate identity fields. An owned detached
    /// execution snapshot must match these content/mode/deletion inputs exactly.
    pub fn content_digest(&self) -> Result<String, serde_json::Error> {
        digest(&("working-content-v1", &self.declared_untracked, &self.files))
    }
}

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

/// I/O seam for an executor that shares one suite deadline and cancellation
/// token across Git queries and material reads. Implementations must measure
/// real bytes; this is not an evidence import seam.
pub trait CaptureIo {
    fn git(&mut self, root: &Path, args: &[&str]) -> io::Result<Vec<u8>>;
    fn file_digest(&mut self, path: &Path) -> io::Result<String>;
    fn checkpoint(&mut self) -> io::Result<()>;
}

struct HostIo;
impl CaptureIo for HostIo {
    fn git(&mut self, root: &Path, args: &[&str]) -> io::Result<Vec<u8>> {
        git(root, args)
    }
    fn file_digest(&mut self, path: &Path) -> io::Result<String> {
        #[cfg(unix)]
        {
            Ok(format!("sha256:{:x}", Sha256::digest(fs::read(path)?)))
        }
        #[cfg(not(unix))]
        {
            let _ = path;
            Err(invalid("material capture requires Unix"))
        }
    }
    fn checkpoint(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn git(root: &Path, args: &[&str]) -> io::Result<Vec<u8>> {
    let output = Command::new("git")
        .arg("--no-optional-locks")
        .args(args)
        .current_dir(root)
        .env("GIT_OPTIONAL_LOCKS", "0")
        .output()?;
    if !output.status.success() {
        return Err(invalid(format!("material capture: git {} failed", args[0])));
    }
    Ok(output.stdout)
}

fn utf8(bytes: &[u8]) -> io::Result<&str> {
    std::str::from_utf8(bytes).map_err(|_| invalid("material capture: non-UTF-8 path/revision"))
}

fn validate_relative(path: &str) -> io::Result<()> {
    if !valid_path(path) {
        return Err(invalid("material capture: invalid relative path"));
    }
    Ok(())
}

#[cfg(unix)]
fn check_ancestors(root: &Path, relative: &str) -> io::Result<()> {
    let path = Path::new(relative);
    let mut ancestor = root.to_path_buf();
    let components: Vec<_> = path.components().collect();
    for component in components.iter().take(components.len().saturating_sub(1)) {
        ancestor.push(component);
        match fs::symlink_metadata(&ancestor) {
            Ok(meta) if !meta.file_type().is_dir() => {
                return Err(invalid(
                    "material capture: non-directory or symlink ancestor",
                ));
            }
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => break,
            Err(error) => return Err(error),
        }
    }
    Ok(())
}

// A symlink is material, but may not smuggle an external or deliberately
// excluded runtime dependency into an otherwise fully bound source tree.
#[cfg(unix)]
fn checked_symlink_target(relative: &str, target: &Path) -> io::Result<()> {
    use std::path::Component;
    let mut components: Vec<_> = Path::new(relative)
        .parent()
        .unwrap_or(Path::new(""))
        .components()
        .collect();
    for component in target.components() {
        match component {
            Component::Normal(_) => components.push(component),
            Component::CurDir => {}
            Component::ParentDir if !components.is_empty() => {
                components.pop();
            }
            _ => return Err(invalid("material capture: escaping symlink target")),
        }
    }
    let resolved: std::path::PathBuf = components.iter().collect();
    let path = resolved
        .to_str()
        .ok_or_else(|| invalid("material capture: non-UTF-8 symlink target"))?;
    if is_bookkeeping(path) || path.split('/').any(|part| part == ".git") {
        return Err(invalid(
            "material capture: symlink target is excluded bookkeeping",
        ));
    }
    Ok(())
}

#[cfg(unix)]
fn entry(root: &Path, relative: &str, io: &mut dyn CaptureIo) -> io::Result<MaterialEntry> {
    io.checkpoint()?;
    use std::os::unix::{ffi::OsStrExt, fs::PermissionsExt};
    check_ancestors(root, relative)?;
    let path = root.join(relative);
    let metadata = match fs::symlink_metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Ok(MaterialEntry {
                mode: "000000".into(),
                content_digest: None,
            });
        }
        Err(error) => return Err(error),
    };
    let (mode, content_digest) = if metadata.file_type().is_symlink() {
        let target = fs::read_link(&path)?;
        checked_symlink_target(relative, &target)?;
        (
            "120000",
            format!("sha256:{:x}", Sha256::digest(target.as_os_str().as_bytes())),
        )
    } else if metadata.file_type().is_file() {
        let mode = if metadata.permissions().mode() & 0o111 != 0 {
            "100755"
        } else {
            "100644"
        };
        (mode, io.file_digest(&path)?)
    } else {
        return Err(invalid(
            "material capture: unsupported directory/submodule/special file",
        ));
    };
    Ok(MaterialEntry {
        mode: mode.into(),
        content_digest: Some(content_digest),
    })
}

#[cfg(not(unix))]
fn entry(_root: &Path, _relative: &str, _io: &mut dyn CaptureIo) -> io::Result<MaterialEntry> {
    Err(invalid(
        "material capture: faithful file modes require a platform adapter",
    ))
}

fn capture_once(
    root: &Path,
    declared: &BTreeSet<String>,
    io: &mut dyn CaptureIo,
    detached: bool,
) -> io::Result<WorkingMaterial> {
    io.checkpoint()?;
    let head_revision = utf8(&io.git(root, &["rev-parse", "--verify", "HEAD^{commit}"])?)?
        .trim()
        .to_owned();
    let work_branch = if detached {
        if utf8(&io.git(root, &["rev-parse", "--abbrev-ref", "HEAD"])?)?.trim() != "HEAD" {
            return Err(invalid("execution snapshot must have detached HEAD"));
        }
        String::new()
    } else {
        utf8(&io.git(root, &["symbolic-ref", "--quiet", "--short", "HEAD"])?)?
            .trim()
            .to_owned()
    };
    let mut paths = BTreeSet::new();
    // Keep HEAD paths as well as index paths. A staged deletion must not make its
    // working bytes disappear from the material inventory.
    for record in io
        .git(root, &["ls-tree", "-rz", "--full-tree", "HEAD"])?
        .split(|b| *b == 0)
        .filter(|r| !r.is_empty())
    {
        let record = utf8(record)?;
        let (metadata, path) = record
            .split_once('\t')
            .ok_or_else(|| invalid("malformed Git tree"))?;
        validate_relative(path)?;
        if is_bookkeeping(path) {
            continue;
        }
        if metadata.split_whitespace().nth(1) != Some("blob") {
            return Err(invalid("material capture: submodules are unsupported"));
        }
        paths.insert(path.to_owned());
    }
    for record in io
        .git(root, &["ls-files", "--stage", "-z"])?
        .split(|b| *b == 0)
        .filter(|r| !r.is_empty())
    {
        let record = utf8(record)?;
        let (metadata, path) = record
            .split_once('\t')
            .ok_or_else(|| invalid("malformed Git index"))?;
        validate_relative(path)?;
        if is_bookkeeping(path) {
            continue;
        }
        let columns: Vec<_> = metadata.split_whitespace().collect();
        if columns.len() != 3 || columns[2] != "0" || columns[0] == "160000" {
            return Err(invalid(
                "material capture: conflicts/submodules are unsupported",
            ));
        }
        paths.insert(path.to_owned());
    }
    for path in declared {
        validate_relative(path)?;
        if is_bookkeeping(path) || paths.contains(path) {
            return Err(invalid(
                "material capture: declared untracked input is tracked/bookkeeping",
            ));
        }
        paths.insert(path.clone());
    }
    for path in io
        .git(root, &["ls-files", "--others", "--exclude-standard", "-z"])?
        .split(|b| *b == 0)
        .filter(|p| !p.is_empty())
    {
        let path = utf8(path)?;
        validate_relative(path)?;
        if !is_bookkeeping(path) && !declared.contains(path) && !paths.contains(path) {
            return Err(invalid(format!(
                "material capture: undeclared untracked input {path}"
            )));
        }
    }
    let mut files = BTreeMap::new();
    for path in paths {
        let material = entry(root, &path, io)?;
        if declared.contains(&path) && material.content_digest.is_none() {
            return Err(invalid("material capture: declared input is missing"));
        }
        files.insert(path, material);
    }
    Ok(WorkingMaterial {
        head_revision,
        work_branch,
        declared_untracked: declared.clone(),
        files,
    })
}

/// Capture twice to reject observable concurrent mutations during capture. The
/// caller must additionally compare pre-execution, post-execution and current
/// captures through `verify`; a single capture is never proof of a stable run.
pub fn capture(root: &Path, declared_untracked: &BTreeSet<String>) -> io::Result<WorkingMaterial> {
    capture_with_io(root, declared_untracked, &mut HostIo)
}

/// Identical material semantics, with caller-owned bounded I/O.
pub fn capture_with_io(
    root: &Path,
    declared_untracked: &BTreeSet<String>,
    io: &mut dyn CaptureIo,
) -> io::Result<WorkingMaterial> {
    let first = capture_once(root, declared_untracked, io, false)?;
    if first != capture_once(root, declared_untracked, io, false)? {
        return Err(invalid("material capture: worktree changed during capture"));
    }
    Ok(first)
}

/// Capture a privately owned execution snapshot, requiring genuinely detached
/// Git HEAD. This does not override or invent a source branch identity.
pub fn capture_detached_with_io(
    root: &Path,
    declared_untracked: &BTreeSet<String>,
    io: &mut dyn CaptureIo,
) -> io::Result<WorkingMaterial> {
    let first = capture_once(root, declared_untracked, io, true)?;
    if first != capture_once(root, declared_untracked, io, true)? {
        return Err(invalid("execution snapshot changed during capture"));
    }
    Ok(first)
}
