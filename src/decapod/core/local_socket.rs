//! Unix endpoints derived from canonical storage, independent of worktree depth.
//! Short paths retain the historical address. Long paths use a confined directory
//! in the system sticky /tmp namespace (not caller-controlled TMPDIR).
use sha2::{Digest, Sha256};
use std::io;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

/// Nonblocking connect avoids an unbounded wait when a Unix backlog is full.
/// WouldBlock means no request was sent and admission may be retried.
pub(crate) fn connect(path: &Path) -> io::Result<std::os::unix::net::UnixStream> {
    use std::os::fd::{AsRawFd, FromRawFd};
    let bytes = path.as_os_str().as_encoded_bytes();
    let mut address: libc::sockaddr_un = unsafe { std::mem::zeroed() };
    if bytes.contains(&0) || bytes.len() >= address.sun_path.len() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "invalid Unix socket address",
        ));
    }
    address.sun_family = libc::AF_UNIX as _;
    for (dst, src) in address.sun_path.iter_mut().zip(bytes) {
        *dst = *src as _;
    }
    let len =
        (std::mem::offset_of!(libc::sockaddr_un, sun_path) + bytes.len() + 1) as libc::socklen_t;
    #[cfg(target_os = "macos")]
    {
        address.sun_len = len as _;
    }
    let fd = unsafe { libc::socket(libc::AF_UNIX, libc::SOCK_STREAM, 0) };
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    let stream = unsafe { std::os::unix::net::UnixStream::from_raw_fd(fd) };
    // Prevent an executed command from keeping the peer or lifetime alive.
    if unsafe { libc::fcntl(fd, libc::F_SETFD, libc::FD_CLOEXEC) } < 0 {
        return Err(io::Error::last_os_error());
    }
    stream.set_nonblocking(true)?;
    if unsafe { libc::connect(fd, (&address as *const libc::sockaddr_un).cast(), len) } < 0 {
        let error = io::Error::last_os_error();
        if error.raw_os_error() != Some(libc::EINPROGRESS) {
            return Err(error);
        }
        let mut poll = libc::pollfd {
            fd: stream.as_raw_fd(),
            events: libc::POLLOUT,
            revents: 0,
        };
        if unsafe { libc::poll(&mut poll, 1, 100) } <= 0 {
            return Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                "Unix connection admission pending",
            ));
        }
        if let Some(error) = stream.take_error()? {
            return Err(error);
        }
    }
    stream.set_nonblocking(false)?;
    Ok(stream)
}

pub(crate) fn path(data: &Path, name: &str) -> io::Result<PathBuf> {
    let data = data.canonicalize()?;
    let original = data.join("broker-runtime").join(name);
    // Darwin has a 104-byte sun_path; Linux has 108. Reserve the terminating NUL.
    if original.as_os_str().as_encoded_bytes().len() < 104 {
        return Ok(original);
    }
    let shared = crate::core::fs_permissions::shared_storage();
    let boundary = if shared {
        format!("g{}", std::fs::metadata(&data)?.gid())
    } else {
        format!("u{}", unsafe { libc::geteuid() })
    };
    let hash = format!("{:x}", Sha256::digest(data.as_os_str().as_encoded_bytes()));
    Ok(Path::new("/tmp")
        .join(format!("decapod-{boundary}-{}", &hash[..32]))
        .join(name))
}

pub(crate) fn ensure_directory(socket: &Path, data: &Path) -> io::Result<()> {
    use std::os::unix::fs::DirBuilderExt;
    let parent = socket.parent().expect("socket parent");
    let shared = crate::core::fs_permissions::shared_storage();
    if parent.parent() == Some(Path::new("/tmp")) {
        match std::fs::DirBuilder::new()
            .mode(if shared { 0o2770 } else { 0o700 })
            .create(parent)
        {
            Ok(()) if shared => {
                // Only a newly created directory may be assigned the datastore's
                // trusted group; never repair or chmod a pre-existing endpoint.
                let gid = std::fs::metadata(data)?.gid();
                let name = std::ffi::CString::new(parent.as_os_str().as_encoded_bytes())?;
                if unsafe { libc::chown(name.as_ptr(), !0, gid) } != 0 {
                    return Err(io::Error::last_os_error());
                }
            }
            Ok(()) => (),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => (),
            Err(e) => return Err(e),
        }
    } else {
        crate::core::fs_permissions::ensure_storage_dir(parent)?;
    }
    crate::core::fs_permissions::check_directory(parent, shared)?;
    let metadata = std::fs::metadata(parent)?;
    if metadata.mode() & if shared { 0o007 } else { 0o077 } != 0
        || (shared && metadata.gid() != std::fs::metadata(data)?.gid())
    {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "BROKER_UNSAFE_SOCKET_DIRECTORY",
        ));
    }
    Ok(())
}

/// Remove only a socket left by a dead peer, under the caller's election lock.
pub(crate) fn remove_stale(socket: &Path) -> io::Result<()> {
    use std::os::unix::fs::FileTypeExt;
    match std::fs::symlink_metadata(socket) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e),
        Ok(m) if !m.file_type().is_socket() => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "BROKER_ENDPOINT_NOT_SOCKET",
            ));
        }
        Ok(_) => (),
    }
    match connect(socket) {
        Err(e) if e.kind() == io::ErrorKind::ConnectionRefused => std::fs::remove_file(socket),
        Err(e) => Err(e),
        Ok(_) => Err(io::Error::new(
            io::ErrorKind::AddrInUse,
            "BROKER_ENDPOINT_LIVE",
        )),
    }
}

/// RAII cleanup never removes a replacement endpoint owned by another process.
pub(crate) struct Endpoint {
    path: PathBuf,
    identity: (u64, u64),
}
impl Endpoint {
    pub(crate) fn capture(path: &Path) -> io::Result<Self> {
        let m = std::fs::symlink_metadata(path)?;
        Ok(Self {
            path: path.to_owned(),
            identity: (m.dev(), m.ino()),
        })
    }
}
impl Drop for Endpoint {
    fn drop(&mut self) {
        if std::fs::symlink_metadata(&self.path).is_ok_and(|m| (m.dev(), m.ino()) == self.identity)
        {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn long_canonical_endpoints_are_short_isolated_and_safe_to_clean() {
        let root = tempfile::tempdir().unwrap();
        let data = root
            .path()
            .join("long-datastore-component".repeat(8))
            .join("data");
        crate::core::fs_permissions::ensure_storage_dir(&data).unwrap();
        let socket = path(&data, "broker.sock").unwrap();
        assert!(socket.as_os_str().as_encoded_bytes().len() < 104);
        ensure_directory(&socket, &data).unwrap();
        let listener = std::os::unix::net::UnixListener::bind(&socket).unwrap();
        let guard = Endpoint::capture(&socket).unwrap();
        assert!(remove_stale(&socket).is_err(), "displaced live peer");
        assert!(connect(&socket).is_ok());
        std::fs::remove_file(&socket).unwrap();
        let replacement = std::os::unix::net::UnixListener::bind(&socket).unwrap();
        drop(guard);
        assert!(socket.exists(), "old owner removed replacement endpoint");
        drop(listener);
        drop(replacement);
        remove_stale(&socket).unwrap();
        assert!(!socket.exists());
        let alias = root.path().join("alias");
        std::os::unix::fs::symlink(&data, &alias).unwrap();
        assert_eq!(path(&alias, "broker.sock").unwrap(), socket);
        let other = data.parent().unwrap().join("other-data");
        crate::core::fs_permissions::ensure_storage_dir(&other).unwrap();
        assert_ne!(path(&other, "broker.sock").unwrap(), socket);
        std::fs::write(&socket, "not a socket").unwrap();
        assert!(remove_stale(&socket).is_err());
        assert_eq!(std::fs::read_to_string(&socket).unwrap(), "not a socket");
        std::fs::remove_file(&socket).unwrap();
        std::fs::remove_dir(socket.parent().unwrap()).unwrap();
    }
}
