//! Creation-time permissions for Decapod-owned filesystem state.
//!
//! Never chmod an existing installation. An unsafe path requires the operator
//! to review ownership and sharing before retrying. POSIX checks are not ACL
//! checks; Windows inherits the user's ACLs (see storage-permissions.md).

use std::fs::{self, File, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};

/// Explicit opt-in for a trusted Unix group sharing a datastore, not credentials.
pub const SHARED_STORAGE_ENV: &str = "DECAPOD_STORAGE_SHARED_GROUP";

pub fn shared_storage() -> bool {
    std::env::var(SHARED_STORAGE_ENV).is_ok_and(|value| value == "1")
}

fn unsafe_path(path: &Path, reason: &str) -> io::Error {
    io::Error::new(
        io::ErrorKind::PermissionDenied,
        format!(
            "STORAGE_UNSAFE_PERMISSIONS: '{}': {reason}. No permissions were changed. Stop other writers, review ownership, ACLs and intended sharing, then have the owner remove unintended write access before retrying. For an intentionally trusted datastore group only, set {SHARED_STORAGE_ENV}=1; world-writable state is never accepted.",
            path.display()
        ),
    )
}

fn absolute(path: &Path) -> io::Result<PathBuf> {
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        Ok(std::env::current_dir()?.join(path))
    }
}

fn check_mode(
    path: &Path,
    metadata: &fs::Metadata,
    shared: bool,
    sticky_ancestor: bool,
) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        // A root-owned system ancestor is trusted; another account is not.
        let uid = unsafe { libc::geteuid() };
        if !shared && metadata.uid() != uid && metadata.uid() != 0 {
            return Err(unsafe_path(
                path,
                "state ancestry is owned by another account",
            ));
        }
        let mode = metadata.permissions().mode();
        // A sticky ancestor such as /tmp cannot replace an owned child. The
        // actual state directory itself must still have a safe write boundary.
        if sticky_ancestor && metadata.is_dir() && mode & 0o1000 != 0 {
            return Ok(());
        }
        let forbidden = if shared { 0o002 } else { 0o022 };
        if mode & forbidden != 0 {
            return Err(unsafe_path(path, "unintended group/world write access"));
        }
    }
    #[cfg(not(unix))]
    let _ = (path, metadata, shared, sticky_ancestor);
    Ok(())
}

/// Validate both lexical and resolved ancestry (including symlink parents).
/// Existing safe modes are preserved. This does not defend against the same
/// account, an explicitly trusted group, root, or changes to ACLs after checking.
pub fn check_directory(path: &Path, shared: bool) -> io::Result<()> {
    let path = absolute(path)?;
    let metadata = fs::symlink_metadata(&path)?;
    if !metadata.is_dir() {
        return Err(unsafe_path(
            &path,
            "state directory is not a real directory",
        ));
    }
    for chain in [path.clone(), path.canonicalize()?] {
        for (index, ancestor) in chain.ancestors().enumerate() {
            let link_metadata = fs::symlink_metadata(ancestor)?;
            if link_metadata.file_type().is_symlink() {
                // Validate ownership of the link as well as its destination.
                #[cfg(unix)]
                {
                    use std::os::unix::fs::MetadataExt;
                    let uid = unsafe { libc::geteuid() };
                    if !shared && link_metadata.uid() != uid && link_metadata.uid() != 0 {
                        return Err(unsafe_path(
                            ancestor,
                            "symlink ancestor belongs to another account",
                        ));
                    }
                }
            }
            let metadata = fs::metadata(ancestor)?;
            #[cfg(unix)]
            if shared {
                use std::os::unix::fs::{MetadataExt, PermissionsExt};
                let leaf_gid = fs::metadata(&path)?.gid();
                let uid = unsafe { libc::geteuid() };
                if metadata.uid() != uid && metadata.uid() != 0 && metadata.gid() != leaf_gid {
                    return Err(unsafe_path(
                        ancestor,
                        "ancestor owner is outside the trusted sharing boundary",
                    ));
                }
                let mode = metadata.permissions().mode();
                if mode & 0o020 != 0 && mode & 0o1000 == 0 && metadata.gid() != leaf_gid {
                    return Err(unsafe_path(
                        ancestor,
                        "writable ancestor belongs to a different sharing group",
                    ));
                }
            }
            check_mode(ancestor, &metadata, shared, index > 0)?;
        }
    }
    Ok(())
}

fn ensure_dir(path: &Path, shared: bool) -> io::Result<()> {
    let path = absolute(path)?;
    match fs::symlink_metadata(&path) {
        Ok(_) => return check_directory(&path, shared),
        Err(error) if error.kind() == io::ErrorKind::NotFound => (),
        Err(error) => return Err(error),
    }
    if let Some(parent) = path.parent() {
        // Permit a sticky system temporary directory only as an ancestor of a
        // newly created private child, never as the directory holding state.
        if fs::symlink_metadata(parent).is_ok() {
            let metadata = fs::metadata(parent)?;
            check_mode(parent, &metadata, shared, true)?;
        } else {
            // Sharing is scoped to this datastore directory, not newly
            // invented config/session/workspace ancestors. Operators arrange
            // group traversal and setgid inheritance explicitly.
            ensure_dir(parent, false)?;
        }
    }
    let mut builder = fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(if shared { 0o770 } else { 0o700 });
    }
    match builder.create(&path) {
        Ok(()) => (),
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => (),
        Err(error) => return Err(error),
    }
    check_directory(&path, shared)
}

/// Convenience form for existing owned-state writers, with std::fs::write's
/// argument shape and atomic private creation/replacement semantics.
pub fn write_private(path: impl AsRef<Path>, bytes: impl AsRef<[u8]>) -> io::Result<()> {
    crate::core::atomic::write_atomic(path.as_ref(), bytes.as_ref())
}

pub fn ensure_private_dir(path: &Path) -> io::Result<()> {
    ensure_dir(path, false)
}
pub fn ensure_storage_dir(path: &Path) -> io::Result<()> {
    ensure_dir(path, shared_storage())
}

pub fn check_file(path: &Path, shared: bool) -> io::Result<()> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    check_directory(parent, shared)?;
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            if !metadata.is_file() {
                return Err(unsafe_path(
                    path,
                    "state file is not a regular file (symlinks are not accepted)",
                ));
            }
            #[cfg(unix)]
            {
                use std::os::unix::fs::MetadataExt;
                let uid = unsafe { libc::geteuid() };
                if shared
                    && (metadata.mode() & 0o020 != 0
                        || (metadata.uid() != uid && metadata.uid() != 0))
                    && metadata.gid() != fs::metadata(parent)?.gid()
                {
                    return Err(unsafe_path(
                        path,
                        "writable file belongs to a different sharing group",
                    ));
                }
                if metadata.nlink() != 1 {
                    return Err(unsafe_path(path, "state file has multiple hard links"));
                }
            }
            check_mode(path, &metadata, shared, false)
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

fn open_file(path: &Path, options: &mut OpenOptions, shared: bool) -> io::Result<File> {
    check_file(path, shared)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(if shared { 0o660 } else { 0o600 });
        options.custom_flags(libc::O_NOFOLLOW);
    }
    let file = options.open(path)?;
    check_mode(path, &file.metadata()?, shared, false)?;
    Ok(file)
}

/// Configure flags on `options`; this boundary adds safe creation permissions.
pub fn open_private_file(path: &Path, options: &mut OpenOptions) -> io::Result<File> {
    open_file(path, options, false)
}
pub fn open_storage_file(path: &Path, options: &mut OpenOptions) -> io::Result<File> {
    open_file(path, options, shared_storage())
}

/// Validate the database and every existing SQLite/coordination sidecar before
/// Dactyl sees them. Newly created SQLite sidecars inherit the database mode.
pub fn check_storage(path: &Path) -> io::Result<()> {
    if path == Path::new(":memory:") {
        return Ok(());
    }
    for suffix in ["", "-wal", "-shm", "-journal", ".lock"] {
        let mut name = path.as_os_str().to_os_string();
        name.push(suffix);
        check_file(Path::new(&name), shared_storage())?;
    }
    Ok(())
}

#[cfg(test)]
#[path = "../../../tests/unit/core/fs_permissions_tests.rs"]
mod tests;
