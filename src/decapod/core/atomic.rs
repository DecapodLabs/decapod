//! Small filesystem primitives for durable governance artifacts.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

/// Replace a file without exposing a partially-written JSON artifact.
/// On Unix, success also requires syncing the containing directory after the
/// rename. A sync failure may leave the new complete file visible: callers must
/// reconcile its contents, rather than treating an error as a rollback receipt.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    write_atomic_with_sync(path, bytes, sync_parent_directory)
}

fn write_atomic_with_sync(
    path: &Path,
    bytes: &[u8],
    sync_parent: impl FnOnce(&Path) -> std::io::Result<()>,
) -> std::io::Result<()> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    crate::core::fs_permissions::ensure_private_dir(parent)?;
    crate::core::fs_permissions::check_file(path, false)?;
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("artifact");
    let temporary = parent.join(format!(".{file_name}.tmp-{}-{nonce}", std::process::id()));
    let mut file = crate::core::fs_permissions::open_private_file(
        &temporary,
        OpenOptions::new().create_new(true).write(true),
    )?;
    let result = (|| {
        file.write_all(bytes)?;
        file.sync_all()?;
        fs::rename(&temporary, path)?;
        sync_parent(parent).map_err(|error| {
            std::io::Error::new(
                error.kind(),
                format!("ARTIFACT_DURABILITY_UNCONFIRMED: complete replacement is visible, but parent directory sync failed: {error}"),
            )
        })
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

/// Flush an already-published directory entry on supported Unix filesystems.
/// Other platforms retain the pre-existing atomic-visibility contract; this
/// helper does not claim power-loss durability where directory fsync is absent.
pub fn sync_parent_directory(parent: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        let directory = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW)
            .open(parent)?;
        directory.sync_all()
    }
    #[cfg(not(unix))]
    {
        let _ = parent;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replacement_is_complete_before_directory_sync() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("state.json");
        write_atomic(&path, b"old").unwrap();
        write_atomic_with_sync(&path, b"new", |parent| {
            assert_eq!(parent, root.path());
            assert_eq!(fs::read(&path).unwrap(), b"new");
            assert_eq!(fs::read_dir(parent).unwrap().count(), 1);
            sync_parent_directory(parent)
        })
        .unwrap();
    }

    #[test]
    fn directory_sync_failure_is_not_reported_as_success_or_rollback() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("state.json");
        write_atomic(&path, b"old").unwrap();
        let error = write_atomic_with_sync(&path, b"new", |_| {
            Err(std::io::Error::other("injected directory sync failure"))
        })
        .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("ARTIFACT_DURABILITY_UNCONFIRMED")
        );
        assert_eq!(fs::read(&path).unwrap(), b"new");
        assert_eq!(fs::read_dir(root.path()).unwrap().count(), 1);
        write_atomic(&path, b"new").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"new");
    }

    #[cfg(unix)]
    #[test]
    fn directory_sync_does_not_follow_a_substituted_symlink() {
        use std::os::unix::fs::symlink;
        let root = tempfile::tempdir().unwrap();
        let link = root.path().join("link");
        symlink(root.path(), &link).unwrap();
        assert!(sync_parent_directory(&link).is_err());
        assert!(sync_parent_directory(&root.path().join("absent")).is_err());
    }
}
