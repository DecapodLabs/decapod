//! Bounded input owner for the existing epoch and repository-signal algorithms.
use super::EpochInput;
use crate::core::{
    error::DecapodError,
    project_specs::{self, RepoSignalIo},
};
use std::{
    fs,
    io::{self, Read},
    path::{Component, Path, PathBuf},
    time::{Duration, Instant},
};

pub(crate) struct BoundedEpochInput {
    root: PathBuf,
    directory: fs::File,
    deadline: Instant,
    remaining_bytes: u64,
    remaining_files: usize,
}

fn invalid(message: &str) -> DecapodError {
    DecapodError::ValidationError(format!("EPOCH_INPUT_UNSUPPORTED: {message}"))
}

impl BoundedEpochInput {
    pub(crate) fn new(root: &Path) -> Result<Self, DecapodError> {
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            let directory = fs::OpenOptions::new()
                .read(true)
                .custom_flags(
                    libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC,
                )
                .open(root)?;
            Ok(Self {
                root: root.to_path_buf(),
                directory,
                deadline: Instant::now() + Duration::from_secs(60),
                remaining_bytes: 512 * 1024 * 1024,
                remaining_files: 100000,
            })
        }
        #[cfg(not(unix))]
        {
            let _ = root;
            Err(invalid(
                "bounded epoch capture requires Unix directory handles",
            ))
        }
    }

    /// Open every component relative to the pinned root. O_NOFOLLOW applies to
    /// ancestors too, so a concurrent symlink swap cannot escape the snapshot
    /// reader or turn a config/spec read into an unbounded FIFO open.
    fn open(&self, path: &Path, directory: bool) -> Result<fs::File, DecapodError> {
        self.open_inner(path, directory, 0)
    }

    /// Shared safe opener for other existing proof owners, which retain their
    /// own deadline, byte budget, digest and authority checks.
    pub(crate) fn open_regular(&self, path: &Path) -> Result<fs::File, DecapodError> {
        self.open(path, false)
    }

    fn open_inner(
        &self,
        path: &Path,
        directory: bool,
        links: usize,
    ) -> Result<fs::File, DecapodError> {
        if links > 16 {
            return Err(invalid("file symlink resolution limit exceeded"));
        }
        #[cfg(unix)]
        {
            use std::os::{
                fd::{AsRawFd, FromRawFd},
                unix::ffi::OsStrExt,
            };
            let relative = path
                .strip_prefix(&self.root)
                .map_err(|_| invalid("path is outside epoch root"))?;
            let components = relative.components().collect::<Vec<_>>();
            let mut current = self.directory.try_clone()?;
            for (index, component) in components.iter().enumerate() {
                let Component::Normal(name) = component else {
                    return Err(invalid("non-normal epoch path"));
                };
                let name = std::ffi::CString::new(name.as_bytes())
                    .map_err(|_| invalid("NUL in epoch path"))?;
                let mut flags =
                    libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC;
                if index + 1 < components.len() || directory {
                    flags |= libc::O_DIRECTORY;
                }
                let fd = unsafe { libc::openat(current.as_raw_fd(), name.as_ptr(), flags) };
                if fd < 0 {
                    let error = io::Error::last_os_error();
                    if !directory
                        && index + 1 == components.len()
                        && error.raw_os_error() == Some(libc::ELOOP)
                    {
                        let mut bytes = [0u8; 4096];
                        let size = unsafe {
                            libc::readlinkat(
                                current.as_raw_fd(),
                                name.as_ptr(),
                                bytes.as_mut_ptr().cast(),
                                bytes.len(),
                            )
                        };
                        if size < 0 {
                            return Err(io::Error::last_os_error().into());
                        }
                        if size as usize == bytes.len() {
                            return Err(invalid("file symlink target exceeds bound"));
                        }
                        let target =
                            Path::new(std::ffi::OsStr::from_bytes(&bytes[..size as usize]));
                        if target.is_absolute() {
                            return Err(invalid("absolute file symlink target"));
                        }
                        let mut normalized =
                            relative.parent().unwrap_or(Path::new("")).to_path_buf();
                        for component in target.components() {
                            match component {
                                Component::Normal(value) => normalized.push(value),
                                Component::CurDir => {}
                                Component::ParentDir if normalized.pop() => {}
                                _ => return Err(invalid("file symlink escapes epoch root")),
                            }
                        }
                        if normalized.starts_with(".git")
                            || normalized.starts_with(".decapod/data")
                            || normalized.starts_with(".decapod/workspaces")
                        {
                            return Err(invalid("file symlink targets runtime bookkeeping"));
                        }
                        return self.open_inner(&self.root.join(normalized), false, links + 1);
                    }
                    return Err(error.into());
                }
                current = unsafe { fs::File::from_raw_fd(fd) };
            }
            if directory && !current.metadata()?.is_dir() {
                return Err(invalid("epoch directory is not a directory"));
            }
            if !directory && !current.metadata()?.is_file() {
                return Err(invalid("epoch input is not a regular file"));
            }
            Ok(current)
        }
        #[cfg(not(unix))]
        {
            let _ = (path, directory);
            Err(invalid(
                "bounded epoch capture requires Unix directory handles",
            ))
        }
    }
}

impl RepoSignalIo for BoundedEpochInput {
    fn checkpoint(&mut self) -> Result<(), DecapodError> {
        if Instant::now() >= self.deadline {
            return Err(DecapodError::ValidationError(
                "EPOCH_INPUT_LIMIT: input deadline exceeded".into(),
            ));
        }
        Ok(())
    }
    fn read(&mut self, path: &Path) -> Result<Vec<u8>, DecapodError> {
        self.checkpoint()?;
        self.remaining_files = self
            .remaining_files
            .checked_sub(1)
            .ok_or_else(|| invalid("file count limit exceeded"))?;
        let mut file = self.open(path, false)?;
        let mut bytes = Vec::new();
        let mut chunk = [0u8; 65536];
        loop {
            self.checkpoint()?;
            let length = file.read(&mut chunk)?;
            if length == 0 {
                break;
            }
            self.remaining_bytes =
                self.remaining_bytes
                    .checked_sub(length as u64)
                    .ok_or_else(|| {
                        DecapodError::ValidationError(
                            "EPOCH_INPUT_LIMIT: total byte limit exceeded".into(),
                        )
                    })?;
            if bytes.len() + length > 64 * 1024 * 1024 {
                return Err(DecapodError::ValidationError(
                    "EPOCH_INPUT_LIMIT: individual file exceeds 64 MiB".into(),
                ));
            }
            bytes.extend_from_slice(&chunk[..length]);
        }
        Ok(bytes)
    }
    fn directory(&mut self, path: &Path) -> Result<(fs::File, fs::ReadDir), DecapodError> {
        self.checkpoint()?;
        let directory = self.open(path, true)?;
        #[cfg(unix)]
        {
            use std::os::fd::AsRawFd;
            // ReadDir stays tied to the held directory, not a replaceable path.
            #[cfg(target_os = "linux")]
            let handle_path = format!("/proc/self/fd/{}", directory.as_raw_fd());
            #[cfg(not(target_os = "linux"))]
            let handle_path = format!("/dev/fd/{}", directory.as_raw_fd());
            let entries = fs::read_dir(handle_path)?;
            Ok((directory, entries))
        }
        #[cfg(not(unix))]
        {
            let _ = directory;
            Err(invalid("directory handles are unsupported"))
        }
    }
    fn regular_file(&mut self, path: &Path) -> Result<bool, DecapodError> {
        self.checkpoint()?;
        Ok(self.open_regular(path)?.metadata()?.is_file())
    }
}

impl EpochInput for BoundedEpochInput {
    fn read_optional(&mut self, path: &Path) -> Result<Option<Vec<u8>>, DecapodError> {
        match self.read(path) {
            Ok(bytes) => Ok(Some(bytes)),
            Err(DecapodError::IoError(error)) if error.kind() == io::ErrorKind::NotFound => {
                Ok(None)
            }
            Err(error) => Err(error),
        }
    }
    fn fingerprint(&mut self, root: &Path) -> Result<String, DecapodError> {
        project_specs::repo_signal_fingerprint_with_io(root, self)
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;

    #[test]
    fn bounded_epoch_preserves_ordinary_epoch_digest() {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir(root.path().join("src")).unwrap();
        fs::write(
            root.path().join("src/lib.rs"),
            "pub fn answer() -> u8 { 42 }\n",
        )
        .unwrap();
        fs::write(root.path().join("README.md"), "# Ordinary project\n").unwrap();
        fs::write(
            root.path().join("src/target"),
            "ordinary source file named target",
        )
        .unwrap();
        symlink("src/lib.rs", root.path().join("alias.rs")).unwrap();
        fs::create_dir_all(root.path().join(".config/build")).unwrap();
        fs::write(
            root.path().join(".config/build/rust-toolchain.toml"),
            "[toolchain]\nchannel='stable'\n",
        )
        .unwrap();
        symlink(
            ".config/build/rust-toolchain.toml",
            root.path().join("rust-toolchain.toml"),
        )
        .unwrap();
        assert_eq!(
            super::super::active_validation_epoch(root.path()).unwrap(),
            super::super::active_validation_epoch_bounded(root.path()).unwrap()
        );
    }

    #[test]
    fn bounded_epoch_refuses_symlink_directories_and_preserves_build_exclusions() {
        let root = tempfile::tempdir().unwrap();
        symlink(root.path(), root.path().join("cycle")).unwrap();
        assert!(super::super::active_validation_epoch_bounded(root.path()).is_err());
        fs::remove_file(root.path().join("cycle")).unwrap();
        symlink(root.path(), root.path().join("target")).unwrap();
        assert!(super::super::active_validation_epoch_bounded(root.path()).is_ok());
    }

    #[test]
    fn bounded_epoch_refuses_symlinked_spec_ancestors() {
        let root = tempfile::tempdir().unwrap();
        let other = tempfile::tempdir().unwrap();
        fs::create_dir(root.path().join(".decapod")).unwrap();
        symlink(other.path(), root.path().join(".decapod/managed")).unwrap();
        assert!(super::super::active_validation_epoch_bounded(root.path()).is_err());
    }

    #[test]
    fn bounded_epoch_enforces_byte_and_depth_limits() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("README.md");
        fs::write(&path, "too many bytes").unwrap();
        let mut input = BoundedEpochInput::new(root.path()).unwrap();
        input.remaining_bytes = 3;
        assert!(
            input
                .read(&path)
                .unwrap_err()
                .to_string()
                .contains("EPOCH_INPUT_LIMIT")
        );
        let mut nested = root.path().to_path_buf();
        for _ in 0..65 {
            nested.push("d");
            fs::create_dir(&nested).unwrap();
        }
        assert!(
            super::super::active_validation_epoch_bounded(root.path())
                .unwrap_err()
                .to_string()
                .contains("depth")
        );
    }

    #[test]
    fn bounded_epoch_refuses_fifo_without_waiting_for_writer() {
        use std::os::unix::ffi::OsStrExt;
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("README.md");
        let raw = std::ffi::CString::new(path.as_os_str().as_bytes()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(raw.as_ptr(), 0o600) }, 0);
        let mut input = BoundedEpochInput::new(root.path()).unwrap();
        assert!(input.read(&path).is_err());
    }
}
