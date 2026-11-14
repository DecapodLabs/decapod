//! Shared prerequisites for real SQLite recovery fixtures.
//! Preserve the released adapter guards; never skip an unsupported filesystem.
#[cfg(any(target_os = "linux", target_os = "android"))]
use std::{
    fs,
    path::{Path, PathBuf},
};

#[cfg(any(target_os = "linux", target_os = "android"))]
pub(crate) fn same_recovery_device(devices: &[u64]) -> std::io::Result<()> {
    if devices.is_empty() || devices.iter().any(|device| *device != devices[0]) {
        return Err(std::io::Error::other(format!(
            "recovery fixture requires equal parent/source/stage/rebuilt device IDs: {devices:?}"
        )));
    }
    Ok(())
}

#[cfg(any(target_os = "linux", target_os = "android"))]
fn recovery_filesystem_kind(path: &Path) -> String {
    use std::os::unix::ffi::OsStrExt;
    let Ok(path) = std::ffi::CString::new(path.as_os_str().as_bytes()) else {
        return "unavailable".into();
    };
    let mut stat = std::mem::MaybeUninit::<libc::statfs>::uninit();
    // Read only the filesystem type; never inspect or change mount settings.
    if unsafe { libc::statfs(path.as_ptr(), stat.as_mut_ptr()) } != 0 {
        return format!("unavailable ({})", std::io::Error::last_os_error());
    }
    let kind = unsafe { stat.assume_init() }.f_type;
    match kind {
        0x794c7630 => "overlayfs".into(),
        0x01021994 => "tmpfs".into(),
        _ => format!("0x{kind:x}"),
    }
}

#[cfg(any(target_os = "linux", target_os = "android"))]
fn probe_recovery_directory(candidate: &Path) -> std::io::Result<tempfile::TempDir> {
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::fs::{MetadataExt, PermissionsExt};

    // Resolve ordinary system temp aliases before creating our private fixture;
    // all paths subsequently passed to Dactyl have real directory ancestors.
    let parent = fs::canonicalize(candidate)?;
    for ancestor in parent.ancestors() {
        if !fs::symlink_metadata(ancestor)?.is_dir() {
            return Err(std::io::Error::other("unsafe recovery fixture ancestor"));
        }
    }
    let root = tempfile::Builder::new()
        .prefix("decapod-recovery-test-")
        .permissions(fs::Permissions::from_mode(0o700))
        .tempdir_in(parent)?;
    let metadata = fs::symlink_metadata(root.path())?;
    if metadata.mode() & 0o777 != 0o700 || metadata.uid() != unsafe { libc::geteuid() } {
        return Err(std::io::Error::other(
            "recovery fixture must be owned and private",
        ));
    }
    // These fixtures hold only a few one-row databases and their sidecars.
    // Do not put the entire suite on a capacity-limited or noexec tmpfs.
    const MINIMUM_SPACE: u64 = 4 * 1024 * 1024;
    let available = fs2::available_space(root.path())?;
    if available < MINIMUM_SPACE {
        return Err(std::io::Error::other(format!(
            "recovery fixture needs {MINIMUM_SPACE} available bytes; found {available}"
        )));
    }
    {
        let source = tempfile::NamedTempFile::new_in(root.path())?;
        let stage = tempfile::Builder::new()
            .permissions(fs::Permissions::from_mode(0o700))
            .tempdir_in(root.path())?;
        let rebuilt = tempfile::NamedTempFile::new_in(stage.path())?;
        let identity = source.as_file().metadata()?;
        same_recovery_device(&[
            metadata.dev(),
            identity.dev(),
            fs::symlink_metadata(stage.path())?.dev(),
            rebuilt.as_file().metadata()?.dev(),
        ])?;
        let destination = rebuilt.path().to_path_buf();
        let from = std::ffi::CString::new(source.path().as_os_str().as_bytes())?;
        let to = std::ffi::CString::new(destination.as_os_str().as_bytes())?;
        let rename = || {
            // Match the released recovery primitive's atomic no-clobber rename.
            let result = unsafe {
                libc::syscall(
                    libc::SYS_renameat2,
                    libc::AT_FDCWD,
                    from.as_ptr(),
                    libc::AT_FDCWD,
                    to.as_ptr(),
                    libc::RENAME_NOREPLACE,
                )
            };
            if result == 0 {
                Ok(())
            } else {
                Err(std::io::Error::last_os_error())
            }
        };
        match rename() {
            Err(error) if error.raw_os_error() == Some(libc::EEXIST) => {}
            other => {
                return Err(std::io::Error::other(format!(
                    "no-clobber probe: {other:?}"
                )));
            }
        }
        rebuilt.close()?;
        rename()?;
        let moved = fs::symlink_metadata(&destination)?;
        if (identity.dev(), identity.ino()) != (moved.dev(), moved.ino()) {
            return Err(std::io::Error::other(
                "recovery rename changed file identity",
            ));
        }
    }
    eprintln!(
        "selected recovery fixture: {} (filesystem={}, device={}, available_bytes={available})",
        root.path().display(),
        recovery_filesystem_kind(root.path()),
        metadata.dev()
    );
    Ok(root)
}

#[cfg(any(target_os = "linux", target_os = "android"))]
pub(crate) fn select_recovery_directory(
    candidates: &[PathBuf],
) -> std::io::Result<tempfile::TempDir> {
    let mut failures = Vec::new();
    for candidate in candidates {
        match probe_recovery_directory(candidate) {
            Ok(directory) => return Ok(directory),
            Err(error) => {
                let diagnostic = format!(
                    "{} (filesystem={}): {error}",
                    candidate.display(),
                    recovery_filesystem_kind(candidate)
                );
                eprintln!("rejected recovery fixture candidate: {diagnostic}");
                failures.push(diagnostic);
            }
        }
    }
    Err(std::io::Error::other(format!(
        "no compatible recovery fixture filesystem; {}",
        failures.join("; ")
    )))
}

pub(crate) fn recovery_fixture_directory() -> tempfile::TempDir {
    #[cfg(any(target_os = "linux", target_os = "android"))]
    {
        // Some layered filesystems report different devices for directories and
        // regular files. Preserve Dactyl's guard and run real recovery elsewhere.
        // /dev/shm is only a preexisting candidate, never a mount request or skip.
        select_recovery_directory(&[std::env::temp_dir(), PathBuf::from("/dev/shm")])
            .expect("a filesystem supporting the released secure recovery contract")
    }
    #[cfg(not(any(target_os = "linux", target_os = "android")))]
    tempfile::tempdir().unwrap()
}
