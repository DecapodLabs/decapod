//! Bounded immutable model-asset transport and private-cache helpers.
use super::{
    bounded_process::{BoundedCommand, Cancellation, ProcessLimits, Termination},
    fs_permissions,
};
use std::{
    fs::{self, File, OpenOptions},
    io,
    path::{Component, Path, PathBuf},
    process::Command,
    time::{Duration, Instant},
};
const MAX_ARTIFACT: u64 = 2 * 1024 * 1024 * 1024;
const DEADLINE: Duration = Duration::from_secs(3600);
fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::other(message.into())
}

pub fn valid_revision(value: &str) -> bool {
    value.len() == 40
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

pub(crate) fn valid_digest(value: &str) -> bool {
    value.strip_prefix("sha256:").is_some_and(|value| {
        value.len() == 64
            && value
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    })
}

/// No lexical traversal or symbolic-link component, including a missing leaf's
/// existing ancestry. Private cache state additionally rejects unsafe ownership
/// and group/world access. Same-account/root mutation is outside this boundary.
fn safe_path(path: &Path) -> io::Result<()> {
    if !path.is_absolute()
        || path
            .components()
            .any(|part| !matches!(part, Component::RootDir | Component::Normal(_)))
    {
        return Err(invalid("model paths must be absolute without traversal"));
    }
    for ancestor in path.ancestors() {
        match fs::symlink_metadata(ancestor) {
            Ok(meta) if meta.file_type().is_symlink() => {
                return Err(invalid("symlinked model cache paths are refused"));
            }
            Ok(_) => (),
            Err(error) if error.kind() == io::ErrorKind::NotFound => (),
            Err(error) => return Err(error),
        }
    }
    Ok(())
}

fn private_directory(path: &Path, create: bool) -> io::Result<()> {
    safe_path(path)?;
    if create {
        fs_permissions::ensure_private_dir(path)?;
    }
    fs_permissions::check_directory(path, false)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let metadata = fs::symlink_metadata(path)?;
        if metadata.uid() != unsafe { libc::geteuid() } || metadata.mode() & 0o077 != 0 {
            return Err(invalid(
                "model cache directories must be owned by this user and mode 0700; no permissions changed",
            ));
        }
    }
    Ok(())
}

fn checked_file(path: &Path) -> io::Result<File> {
    safe_path(path)?;
    fs_permissions::check_file(path, false)?;
    let file = fs_permissions::open_private_file(path, OpenOptions::new().read(true))?;
    let metadata = file.metadata()?;
    if !metadata.is_file() {
        return Err(invalid("model artifact must be a regular file"));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if metadata.uid() != unsafe { libc::geteuid() } || metadata.mode() & 0o077 != 0 {
            return Err(invalid(
                "model cache files must be privately owned regular files",
            ));
        }
    }
    Ok(file)
}

pub(crate) fn default_cache() -> io::Result<PathBuf> {
    let home = || std::env::var_os("HOME").map(PathBuf::from);
    let base = if cfg!(target_os = "macos") {
        home().map(|path| path.join("Library/Caches"))
    } else if cfg!(target_os = "windows") {
        std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var_os("USERPROFILE")
                    .map(|path| PathBuf::from(path).join("AppData/Local"))
            })
    } else {
        std::env::var_os("XDG_CACHE_HOME")
            .map(PathBuf::from)
            .or_else(|| home().map(|path| path.join(".cache")))
    };
    let root = base
        .ok_or_else(|| invalid("local cache home unavailable; supply --cache-directory"))?
        .join("decapod/laya");
    safe_path(&root)?;
    Ok(root)
}

/// Caller supplies only a reviewed, immutable allowlisted URL. This helper
/// preserves the existing bounded, credential-free model transport and safe
/// single-link destination contract for the optional Laya provider.
pub(crate) fn download_public_asset(
    url: &str,
    destination: &Path,
    maximum: u64,
    deadline: Instant,
    cancellation: &Cancellation,
) -> io::Result<()> {
    if maximum == 0 || maximum > MAX_ARTIFACT || !url.starts_with("https://") {
        return Err(invalid("invalid bounded public model transport request"));
    }
    let transport = LocalVerification {
        deadline,
        cancellation,
    };
    transport.checkpoint()?;
    let remaining = deadline
        .checked_duration_since(Instant::now())
        .ok_or_else(|| io::Error::new(io::ErrorKind::TimedOut, "model setup deadline exceeded"))?;
    let timeout = remaining.min(if maximum > 4 * 1024 * 1024 {
        DEADLINE
    } else {
        Duration::from_secs(60)
    });
    if timeout.as_secs() == 0 {
        return Err(io::Error::new(
            io::ErrorKind::TimedOut,
            "model setup deadline exceeded",
        ));
    }
    safe_path(destination)?;
    private_directory(
        destination
            .parent()
            .ok_or_else(|| invalid("download parent missing"))?,
        false,
    )?;
    let output_file = fs_permissions::open_private_file(
        destination,
        OpenOptions::new().write(true).create_new(true),
    )?;
    let mut command = curl_command(url, destination, maximum, timeout);
    // Enforce a child-only file-size bound even for chunked responses.
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        unsafe {
            command.pre_exec(move || {
                let limit = libc::rlimit {
                    rlim_cur: maximum as libc::rlim_t,
                    rlim_max: maximum as libc::rlim_t,
                };
                if libc::setrlimit(libc::RLIMIT_FSIZE, &limit) != 0 {
                    return Err(io::Error::last_os_error());
                }
                libc::umask(0o077);
                Ok(())
            });
        }
    }
    let capture = command.bounded_capture(
        ProcessLimits {
            timeout,
            max_output_bytes: 8192,
        },
        cancellation,
    )?;
    if capture.termination == Termination::Cancelled {
        return Err(io::Error::new(
            io::ErrorKind::Interrupted,
            "model download cancelled",
        ));
    }
    if capture.termination == Termination::TimedOut {
        return Err(io::Error::new(
            io::ErrorKind::TimedOut,
            "model download deadline exceeded",
        ));
    }
    transport.checkpoint()?;
    let http_status = std::str::from_utf8(&capture.output.stdout)
        .ok()
        .and_then(|text| text.trim().parse::<u16>().ok());
    if matches!(http_status, Some(401 | 403)) {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "public model access denied; no fallback, credential or agreement accepted",
        ));
    }
    if capture.termination != Termination::Exited || !capture.output.status.success() {
        return Err(invalid(format!(
            "public model download failed ({:?}, HTTP {:?}, exit {:?}); no checkpoint activated",
            capture.termination,
            http_status,
            capture.output.status.code()
        )));
    }
    let file = checked_file(destination)?;
    if file.metadata()?.len() == 0 || file.metadata()?.len() > maximum {
        return Err(invalid("model download length outside bound"));
    }
    output_file.sync_all()
}

fn curl_command(url: &str, destination: &Path, maximum: u64, timeout: Duration) -> Command {
    let mut command = Command::new("/usr/bin/curl");
    // No ambient token, netrc, proxy, cookies, user config or custom trust roots.
    command
        .env_clear()
        .args([
            "--disable",
            "--silent",
            "--show-error",
            "--fail",
            "--location",
            "--max-redirs",
            "5",
            "--proto",
            "=https",
            "--proto-redir",
            "=https",
            "--noproxy",
            "*",
            "--user-agent",
            "decapod-laya-setup",
            "--connect-timeout",
            "10",
            "--speed-limit",
            "1024",
            "--speed-time",
            "30",
            "--max-time",
        ])
        .arg(timeout.as_secs().to_string())
        .arg("--max-filesize")
        .arg(maximum.to_string())
        .args(["--write-out", "%{http_code}", "--output"])
        .arg(destination)
        .arg("--url")
        .arg(url);
    command
}

struct LocalVerification<'a> {
    deadline: Instant,
    cancellation: &'a Cancellation,
}
impl LocalVerification<'_> {
    fn checkpoint(&self) -> io::Result<()> {
        if self.cancellation.is_cancelled() {
            return Err(io::Error::new(
                io::ErrorKind::Interrupted,
                "model verification cancelled",
            ));
        }
        if Instant::now() >= self.deadline {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "model verification deadline exceeded",
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn transport_rejects_cancelled_or_expired_work_before_creating_a_file() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().canonicalize().unwrap().join("asset");
        let cancel = Cancellation::default();
        cancel.cancel();
        assert_eq!(
            download_public_asset(
                "https://huggingface.co/fixture",
                &path,
                64,
                Instant::now() + Duration::from_secs(10),
                &cancel
            )
            .unwrap_err()
            .kind(),
            io::ErrorKind::Interrupted
        );
        let cancel = Cancellation::default();
        assert_eq!(
            download_public_asset(
                "https://huggingface.co/fixture",
                &path,
                64,
                Instant::now() - Duration::from_secs(1),
                &cancel
            )
            .unwrap_err()
            .kind(),
            io::ErrorKind::TimedOut
        );
        assert!(!path.exists());
    }
    #[test]
    fn transport_never_overwrites_an_existing_private_destination() {
        let root = tempfile::tempdir().unwrap();
        let private = root.path().canonicalize().unwrap().join("private");
        fs_permissions::ensure_private_dir(&private).unwrap();
        let path = private.join("asset");
        fs_permissions::write_private(&path, b"retained").unwrap();
        let error = download_public_asset(
            "https://huggingface.co/fixture",
            &path,
            64,
            Instant::now() + Duration::from_secs(10),
            &Cancellation::default(),
        )
        .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::AlreadyExists, "{error}");
        assert_eq!(fs::read(path).unwrap(), b"retained");
    }
    #[test]
    fn immutable_pin_grammar_rejects_lossy_or_ambiguous_values() {
        assert!(valid_revision(&"a".repeat(40)));
        assert!(valid_digest(&format!("sha256:{}", "f".repeat(64))));
        for revision in [
            "HEAD".to_string(),
            "A".repeat(40),
            "a".repeat(41),
            "é".repeat(20),
        ] {
            assert!(!valid_revision(&revision));
        }
        for digest in [
            "f".repeat(64),
            format!("sha256:{}", "F".repeat(64)),
            format!("sha256:{}", "f".repeat(65)),
        ] {
            assert!(!valid_digest(&digest));
        }
    }
}
