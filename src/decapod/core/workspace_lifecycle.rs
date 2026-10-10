//! Durable ownership for interrupted workspace creation and container handoffs.
//! A task-looking name is never ownership: the canonical event store binds an
//! invocation to an exact directory identity before Git or a runtime sees it.
use crate::core::db::OptionalExtension;
use crate::core::error::DecapodError;
use crate::core::{db, events, fs_permissions, time};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Receipt {
    event_type: String,
    event_id: String,
    ts: String,
    invocation: String,
    path: PathBuf,
    staging: PathBuf,
    device: u64,
    inode: u64,
    created_nanos: Option<u128>,
    container_expected: bool,
    #[serde(default)]
    ready: bool,
    #[serde(default)]
    producer_lease: bool,
}

#[derive(Debug)]
pub struct OwnedWorkspace {
    root: PathBuf,
    receipt: Receipt,
    _lease: File,
    _directory: File,
}
impl OwnedWorkspace {
    pub fn invocation(&self) -> &str {
        &self.receipt.invocation
    }
    pub fn container_expected(&self) -> bool {
        self.receipt.container_expected
    }
    pub fn producer_lease(&self) -> bool {
        self.receipt.producer_lease
    }
    pub fn remove_empty(self) -> Result<(), DecapodError> {
        self.verify()?;
        fs::remove_dir(self.path()).map_err(DecapodError::IoError)
    }
    pub fn is_ready(&self) -> bool {
        self.receipt.ready
    }
    pub fn mark_ready(&mut self) -> Result<(), DecapodError> {
        self.verify()?;
        self.receipt.ready = true;
        self.persist()
    }
    pub fn path(&self) -> &Path {
        &self.receipt.path
    }
    pub fn require_container(&mut self) -> Result<(), DecapodError> {
        self.verify()?;
        if !self.receipt.container_expected {
            self.receipt.container_expected = true;
            self.persist()?;
        }
        Ok(())
    }
    pub fn verify(&self) -> Result<(), DecapodError> {
        // Keeping the original directory open prevents inode reuse for this
        // invocation even on filesystems without persistent birth identity.
        if identity(&self.receipt.path)
            .is_ok_and(|(dev, ino, _)| dev == self.receipt.device && ino == self.receipt.inode)
        {
            Ok(())
        } else {
            Err(failure("directory identity changed; preserving workspace"))
        }
    }
    pub fn remove(self) -> Result<(), DecapodError> {
        if !self.path().exists() {
            return Ok(());
        }
        self.verify()?;
        fs::remove_dir_all(self.path()).map_err(DecapodError::IoError)
    }
    fn persist(&mut self) -> Result<(), DecapodError> {
        self.receipt.event_id = crate::core::ulid::new_ulid();
        self.receipt.ts = time::now_epoch_z();
        events::append(
            &self.root.join(".decapod/data"),
            events::BROKER,
            &serde_json::to_value(&self.receipt).map_err(|e| failure(&e.to_string()))?,
        )?;
        Ok(())
    }
}

fn failure(detail: &str) -> DecapodError {
    DecapodError::ValidationError(format!("WORKSPACE_LIFECYCLE_RECOVERY: {detail}"))
}

fn canonical_target(repo: &Path, target: &Path) -> Result<(PathBuf, PathBuf), DecapodError> {
    let root = repo.canonicalize()?;
    let parent = root.join(".decapod/workspaces");
    fs_permissions::ensure_private_dir(&parent)?;
    if parent.canonicalize()? != parent
        || target.parent().and_then(|p| p.canonicalize().ok()).as_ref() != Some(&parent)
    {
        return Err(failure(
            "workspace must be a direct child of its canonical managed parent",
        ));
    }
    let name = target
        .file_name()
        .ok_or_else(|| failure("missing workspace name"))?;
    Ok((root, parent.join(name)))
}

fn key(path: &Path) -> String {
    format!(
        "workspace.reservation.{:x}",
        Sha256::digest(path.as_os_str().as_encoded_bytes())
    )
}

fn lease(root: &Path, target: &Path) -> Result<File, DecapodError> {
    let locks = root.join(".decapod/data/workspace-leases");
    fs_permissions::ensure_storage_dir(&locks)?;
    let path = locks.join(format!("{}.lock", key(target)));
    let file = fs_permissions::open_storage_file(
        &path,
        fs::OpenOptions::new().read(true).write(true).create(true),
    )?;
    file.try_lock_exclusive().map_err(|e| {
        failure(&format!(
            "workspace operation is active or lease is unavailable: {e}"
        ))
    })?;
    Ok(file)
}

fn lookup(root: &Path, target: &Path) -> Result<Option<Receipt>, DecapodError> {
    let path = events::canonical_db_path(&root.join(".decapod/data"));
    if !path.exists() {
        return Ok(None);
    }
    let conn = db::db_connect_for_validate(&path.to_string_lossy())?;
    if !conn.has_table("events")? {
        return Ok(None);
    }
    let raw: Option<String> = conn.query_row(
        "SELECT payload FROM events WHERE stream = ?1 AND event_type = ?2 ORDER BY seq DESC LIMIT 1",
        db::params![events::BROKER, key(target)], |row| row.get(0)).optional()?;
    raw.map(|value| {
        serde_json::from_str(&value)
            .map_err(|e| failure(&format!("invalid ownership receipt: {e}")))
    })
    .transpose()
}

#[cfg(unix)]
fn identity(path: &Path) -> Result<(u64, u64, Option<u128>), DecapodError> {
    use std::os::unix::fs::MetadataExt;
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(failure("workspace is not a real directory"));
    }
    let created = metadata
        .created()
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|time| time.as_nanos());
    Ok((metadata.dev(), metadata.ino(), created))
}
#[cfg(not(unix))]
fn identity(_path: &Path) -> Result<(u64, u64, Option<u128>), DecapodError> {
    Err(failure(
        "directory identity is unsupported on this platform",
    ))
}
fn matches_identity(path: &Path, receipt: &Receipt) -> bool {
    identity(path).is_ok_and(|(dev, ino, created)| {
        dev == receipt.device
            && ino == receipt.inode
            && (receipt.created_nanos.is_some() && created == receipt.created_nanos
                || generation_nonce(path, None).as_deref() == Some(receipt.invocation.as_str()))
    })
}

// Optional persistent generation evidence for Unix filesystems without birth
// timestamps. Unsupported xattrs narrow automatic cleanup, never creation.
fn generation_nonce(path: &Path, write: Option<&str>) -> Option<String> {
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    {
        use std::os::unix::ffi::OsStrExt;
        let path = std::ffi::CString::new(path.as_os_str().as_bytes()).ok()?;
        let name = c"user.decapod.invocation";
        if let Some(value) = write {
            #[cfg(target_os = "linux")]
            let result = unsafe {
                libc::setxattr(
                    path.as_ptr(),
                    name.as_ptr(),
                    value.as_ptr().cast(),
                    value.len(),
                    libc::XATTR_CREATE,
                )
            };
            #[cfg(target_os = "macos")]
            let result = unsafe {
                libc::setxattr(
                    path.as_ptr(),
                    name.as_ptr(),
                    value.as_ptr().cast(),
                    value.len(),
                    0,
                    libc::XATTR_CREATE,
                )
            };
            if result != 0 {
                return None;
            }
        }
        let mut bytes = [0u8; 128];
        #[cfg(target_os = "linux")]
        let count = unsafe {
            libc::getxattr(
                path.as_ptr(),
                name.as_ptr(),
                bytes.as_mut_ptr().cast(),
                bytes.len(),
            )
        };
        #[cfg(target_os = "macos")]
        let count = unsafe {
            libc::getxattr(
                path.as_ptr(),
                name.as_ptr(),
                bytes.as_mut_ptr().cast(),
                bytes.len(),
                0,
                0,
            )
        };
        if count <= 0 || count as usize > bytes.len() {
            return None;
        }
        String::from_utf8(bytes[..count as usize].to_vec()).ok()
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        let _ = (path, write);
        None
    }
}

/// Persist the ownership binding before publishing a directory to Git/runtime.
/// The returned lease must span the caller's resource-producing operation.
pub fn reserve(
    repo: &Path,
    target: &Path,
    container_expected: bool,
) -> Result<OwnedWorkspace, DecapodError> {
    let (root, target) = canonical_target(repo, target)?;
    let lease = lease(&root, &target)?;
    if fs::symlink_metadata(&target).is_ok() {
        return Err(failure("target already exists; refusing adoption"));
    }
    // Recover a death after durable intent but before the no-replace rename.
    if let Some(receipt) = lookup(&root, &target)?
        && receipt.path == target
        && receipt.staging.parent() == Some(root.join(".decapod/workspaces/.staging").as_path())
        && matches_identity(&receipt.staging, &receipt)
    {
        publish_directory(&receipt.staging, &target)?;
        return Ok(OwnedWorkspace {
            root,
            receipt,
            _directory: File::open(&target)?,
            _lease: lease,
        });
    }
    let staging_parent = root.join(".decapod/workspaces/.staging");
    fs_permissions::ensure_private_dir(&staging_parent)?;
    let stage = tempfile::Builder::new()
        .prefix("invocation-")
        .tempdir_in(&staging_parent)?;
    let (device, inode, created_nanos) = identity(stage.path())?;
    let receipt = Receipt {
        event_type: key(&target),
        event_id: String::new(),
        ts: String::new(),
        invocation: crate::core::ulid::new_ulid(),
        path: target,
        staging: stage.path().to_path_buf(),
        device,
        inode,
        created_nanos,
        container_expected,
        ready: false,
        producer_lease: container_expected,
    };
    if receipt.created_nanos.is_none() {
        generation_nonce(stage.path(), Some(&receipt.invocation));
    }
    let mut owned = OwnedWorkspace {
        root,
        receipt,
        _lease: lease,
        _directory: File::open(stage.path())?,
    };
    File::open(stage.path())?.sync_all()?;
    owned.persist()?;
    let staging = stage.keep();
    publish_directory(&staging, owned.path())?;
    File::open(owned.path().parent().unwrap())?.sync_all()?;
    Ok(owned)
}

/// Acquire an existing invocation only if path and directory identity match.
pub fn acquire(repo: &Path, target: &Path) -> Result<Option<OwnedWorkspace>, DecapodError> {
    let (root, target) = canonical_target(repo, target)?;
    let lease = lease(&root, &target)?;
    let Some(receipt) = lookup(&root, &target)? else {
        return Ok(None);
    };
    if receipt.path != target || !matches_identity(&target, &receipt) {
        return Ok(None);
    }
    Ok(Some(OwnedWorkspace {
        root,
        receipt,
        _directory: File::open(&target)?,
        _lease: lease,
    }))
}

/// Extend a Git-registered worktree into durable container ownership.
pub fn acquire_registered(repo: &Path, target: &Path) -> Result<OwnedWorkspace, DecapodError> {
    use crate::core::bounded_process::{BoundedCommand, CONTROL_TIMEOUT};
    let (root, target) = canonical_target(repo, target)?;
    let lease = lease(&root, &target)?;
    if let Some(receipt) = lookup(&root, &target)?
        && receipt.path == target
        && matches_identity(&target, &receipt)
    {
        return Ok(OwnedWorkspace {
            root,
            receipt,
            _directory: File::open(&target)?,
            _lease: lease,
        });
    }
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(&root)
        .args(["worktree", "list", "--porcelain"])
        .bounded_output(CONTROL_TIMEOUT)?;
    let registered = output.status.success()
        && String::from_utf8_lossy(&output.stdout)
            .lines()
            .filter_map(|line| line.strip_prefix("worktree "))
            .any(|path| Path::new(path).canonicalize().ok().as_ref() == Some(&target));
    if !registered {
        return Err(failure(
            "existing workspace has no trusted Git registration",
        ));
    }
    let (device, inode, created_nanos) = identity(&target)?;
    let receipt = Receipt {
        event_type: key(&target),
        event_id: String::new(),
        ts: String::new(),
        invocation: crate::core::ulid::new_ulid(),
        path: target,
        staging: PathBuf::new(),
        device,
        inode,
        created_nanos,
        container_expected: false,
        ready: true,
        producer_lease: false,
    };
    if receipt.created_nanos.is_none() {
        generation_nonce(&receipt.path, Some(&receipt.invocation));
    }
    let mut owned = OwnedWorkspace {
        root,
        _directory: File::open(&receipt.path)?,
        receipt,
        _lease: lease,
    };
    owned.persist()?;
    Ok(owned)
}

fn publish_directory(source: &Path, target: &Path) -> Result<(), DecapodError> {
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    {
        use std::os::unix::ffi::OsStrExt;
        let source = std::ffi::CString::new(source.as_os_str().as_bytes())
            .map_err(|_| failure("invalid source path"))?;
        let target = std::ffi::CString::new(target.as_os_str().as_bytes())
            .map_err(|_| failure("invalid target path"))?;
        #[cfg(target_os = "linux")]
        let result = unsafe {
            libc::renameat2(
                libc::AT_FDCWD,
                source.as_ptr(),
                libc::AT_FDCWD,
                target.as_ptr(),
                libc::RENAME_NOREPLACE,
            )
        };
        #[cfg(target_os = "macos")]
        let result =
            unsafe { libc::renamex_np(source.as_ptr(), target.as_ptr(), libc::RENAME_EXCL) };
        if result != 0 {
            return Err(DecapodError::IoError(std::io::Error::last_os_error()));
        }
        Ok(())
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        let _ = (source, target);
        Err(failure(
            "atomic no-replace workspace publication unsupported",
        ))
    }
}

#[cfg(all(test, unix))]
#[path = "../../../tests/unit/core/workspace_lifecycle_tests.rs"]
mod tests;
