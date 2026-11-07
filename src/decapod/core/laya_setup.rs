//! Explicit immutable external data installation; validation only verifies it.
//! Reuses Decapod's bounded public transport, storage boundary and local runtime
//! registry. Cache hits are copied, never hardlinked into active installations.
use super::{
    atomic,
    bounded_process::{BoundedCommand, Cancellation, ProcessLimits, Termination},
    fs_permissions,
    laya_assets,
    laya_protocol::Manifest,
};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, OpenOptions},
    io::{self, Read, Write},
    path::{Path, PathBuf},
    process::Command,
    time::{Duration, Instant},
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RuntimePin {
    pub runtime: PathBuf,
    pub runtime_sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Installation {
    pub directory: PathBuf,
    pub manifest_digest: String,
    #[serde(default)]
    pub runtime: PathBuf,
    #[serde(default)]
    pub runtime_sha256: String,
    /// Separate CPU-only executable when GPU libraries prevent worker startup.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cpu_runtime: Option<RuntimePin>,
}

fn invalid(reason: impl Into<String>) -> io::Error {
    io::Error::other(reason.into())
}

fn valid_revision(value: &str) -> bool {
    value.len() == 40
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

const MAX_ARTIFACT: u64 = 4 * 1024 * 1024 * 1024; // 4 GiB
const DEADLINE: Duration = Duration::from_secs(1800);

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
            "--write-out",
            "%{http_code}",
            "--output",
            destination.to_str().unwrap_or(""),
            url,
        ]);
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
    command
}

fn safe_path(path: &Path) -> io::Result<()> {
    if !path.is_absolute() || path.canonicalize()? != path {
        return Err(invalid("model path must be absolute canonical"));
    }
    let parent = path.parent().ok_or_else(|| invalid("model path parent missing"))?;
    fs_permissions::check_directory(parent, false)?;
    Ok(())
}

fn private_directory(dir: &Path, allow_empty: bool) -> io::Result<()> {
    fs_permissions::check_directory(dir, allow_empty)
}

fn checked_file(path: &Path) -> io::Result<fs::File> {
    safe_path(path)?;
    fs_permissions::open_private_file(path, OpenOptions::new().read(true))
}

fn laya_default_cache() -> io::Result<PathBuf> {
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
    let mut output_file = fs_permissions::open_private_file(
        destination,
        OpenOptions::new().write(true).create_new(true),
    )?;
    let mut command = curl_command(url, destination, maximum, timeout);
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

#[derive(Debug, Default, Clone)]
pub struct SetupOptions {
    pub cache_directory: Option<PathBuf>,
    pub runtime: Option<PathBuf>,
    pub cpu_runtime: Option<PathBuf>,
    pub offline: bool,
}

#[derive(Debug, Serialize)]
pub struct SetupReport {
    pub schema: &'static str,
    pub model_directory: PathBuf,
    pub manifest_digest: String,
    pub runtime_registered: bool,
    pub configuration_toml: String,
    pub accuracy_scope: &'static str,
}

#[derive(Debug, Serialize)]
pub struct UpdateReport {
    pub schema: &'static str,
    pub reviewed_manifest_digest: String,
    pub latest_revisions: std::collections::BTreeMap<String, String>,
    pub metadata_compatible: bool,
    pub weights_verified: bool,
    pub activation_performed: bool,
    pub detail: &'static str,
}

/// Explicit bounded discovery never activates an unreviewed upstream change.
/// Compatibility here means identical reviewed revisions, not that different
/// weights would be safe or preserve a readiness-domain accuracy guarantee.
pub fn check_updates(cache_directory: Option<&Path>) -> io::Result<UpdateReport> {
    let root = cache_directory
        .map(Path::to_path_buf)
        .map(Ok)
        .unwrap_or_else(default_cache)?;
    fs_permissions::ensure_private_dir(&root)?;
    let scratch = tempfile::Builder::new()
        .prefix(".laya-update-")
        .tempdir_in(&root)?;
    let signal = super::bounded_process::SignalCancellationGuard::new()?;
    let deadline = Instant::now() + Duration::from_secs(120);
    let mut latest_revisions = std::collections::BTreeMap::new();
    let mut metadata_compatible = true;
    for (index, (repository, expected, url)) in [
        (
            laya_assets::CHECKPOINT,
            laya_assets::CHECKPOINT_REVISION,
            format!(
                "https://huggingface.co/api/models/{}",
                laya_assets::CHECKPOINT
            ),
        ),
        (
            laya_assets::CONVERSION,
            laya_assets::CONVERSION_REVISION,
            format!(
                "https://huggingface.co/api/models/{}",
                laya_assets::CONVERSION
            ),
        ),
        (
            "NandhaKishorM/laya",
            super::laya_protocol::SDK_REVISION,
            "https://api.github.com/repos/NandhaKishorM/laya/commits?per_page=1".into(),
        ),
    ]
    .into_iter()
    .enumerate()
    {
        let path = scratch.path().join(format!("metadata-{index}.json"));
        vega_setup::download_public_asset(
            &url,
            &path,
            1024 * 1024,
            deadline,
            signal.cancellation(),
        )?;
        let bytes = fs::read(&path)?;
        let value: serde_json::Value = serde_json::from_slice(&bytes).map_err(io::Error::other)?;
        let revision = value
            .get("sha")
            .or_else(|| value.get(0).and_then(|v| v.get("sha")))
            .and_then(|v| v.as_str())
            .filter(|v| vega_setup::valid_revision(v))
            .ok_or_else(|| invalid("laya_update_revision_invalid"))?;
        metadata_compatible &= revision == expected;
        latest_revisions.insert(repository.into(), revision.into());
    }
    Ok(UpdateReport {
        schema: "decapod.laya-update-check.v1",
        reviewed_manifest_digest: laya_assets::manifest_digest(&laya_assets::baseline())
            .map_err(io::Error::other)?,
        latest_revisions,
        metadata_compatible,
        weights_verified: false,
        activation_performed: false,
        detail: "Only identical reviewed revisions are compatible without a new reviewed profile. Different source/model/conversion revisions require explicit review, numerical qualification and a new pinned team policy; no artifact or active decision was changed.",
    })
}

fn checkpoint(deadline: Instant, cancel: &Cancellation) -> io::Result<()> {
    if cancel.is_cancelled() || Instant::now() >= deadline {
        Err(invalid("laya_setup_cancelled_or_timeout"))
    } else {
        Ok(())
    }
}
pub(crate) fn default_cache() -> io::Result<PathBuf> {
    Ok(laya_default_cache()?.with_file_name("laya"))
}

fn read_metadata(path: &Path) -> io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    fs_permissions::open_private_file(path, OpenOptions::new().read(true))?
        .take(16385)
        .read_to_end(&mut bytes)?;
    if bytes.len() > 16384 {
        return Err(invalid("laya_manifest_overflow"));
    }
    Ok(bytes)
}

fn verify_file(
    path: &Path,
    bytes: u64,
    sha256: &str,
    deadline: Instant,
    cancel: &Cancellation,
) -> io::Result<()> {
    checkpoint(deadline, cancel)?;
    let mut file = fs_permissions::open_private_file(path, OpenOptions::new().read(true))?;
    if file.metadata()?.len() != bytes {
        return Err(invalid("laya_asset_size_mismatch"));
    }
    let mut hasher = Sha256::new();
    let mut buffer = [0; 65536];
    let mut count = 0;
    loop {
        checkpoint(deadline, cancel)?;
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        count += n as u64;
        if count > bytes {
            return Err(invalid("laya_asset_grew"));
        }
        hasher.update(&buffer[..n]);
    }
    if count != bytes || format!("{:x}", hasher.finalize()) != sha256 {
        return Err(invalid("laya_asset_integrity_mismatch"));
    }
    Ok(())
}

pub(crate) fn verify_installation(
    directory: &Path,
    expected: &str,
    deadline: Instant,
    cancel: &Cancellation,
) -> io::Result<Manifest> {
    checkpoint(deadline, cancel)?;
    if !directory.is_absolute() || directory.canonicalize()? != directory {
        return Err(invalid("laya_noncanonical_installation"));
    }
    fs_permissions::check_directory(directory, false)?;
    let manifest: Manifest =
        serde_json::from_slice(&read_metadata(&directory.join("manifest.json"))?)
            .map_err(io::Error::other)?;
    if !laya_assets::compatible(&manifest)
        || laya_assets::manifest_digest(&manifest).map_err(io::Error::other)? != expected
    {
        return Err(invalid("laya_manifest_unreviewed_or_changed"));
    }
    for asset in &manifest.assets {
        verify_file(
            &directory.join(&asset.path),
            asset.bytes,
            &asset.sha256,
            deadline,
            cancel,
        )?;
    }
    Ok(manifest)
}

/// Copies through separately opened safe files and hashes the result. A cache
/// hardlink remains rejected at the source boundary; no chmod repairs it.
fn copy_verified(
    source: &Path,
    destination: &Path,
    bytes: u64,
    sha256: &str,
    deadline: Instant,
    cancel: &Cancellation,
) -> io::Result<()> {
    verify_file(source, bytes, sha256, deadline, cancel)?;
    fs_permissions::ensure_private_dir(
        destination
            .parent()
            .ok_or_else(|| invalid("laya_copy_parent_missing"))?,
    )?;
    let mut input = fs_permissions::open_private_file(source, OpenOptions::new().read(true))?;
    let mut output = fs_permissions::open_private_file(
        destination,
        OpenOptions::new().write(true).create_new(true),
    )?;
    let mut buffer = [0; 65536];
    let mut count = 0;
    loop {
        checkpoint(deadline, cancel)?;
        let n = input.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        count += n as u64;
        if count > bytes {
            return Err(invalid("laya_copy_size_changed"));
        }
        output.write_all(&buffer[..n])?;
    }
    output.sync_all()?;
    verify_file(destination, bytes, sha256, deadline, cancel)
}

pub fn run(options: &SetupOptions) -> io::Result<SetupReport> {
    if options.cpu_runtime.is_some() && options.runtime.is_none() {
        return Err(invalid("laya_cpu_runtime_requires_primary_runtime"));
    }
    let root = options
        .cache_directory
        .clone()
        .map(Ok)
        .unwrap_or_else(laya_default_cache)?;
    fs_permissions::ensure_private_dir(&root)?;
    let root = root.canonicalize()?;
    let lock = fs_permissions::open_private_file(
        &root.join("setup.lock"),
        OpenOptions::new().read(true).write(true).create(true),
    )?;
    lock.try_lock_exclusive()
        .map_err(|_| invalid("another_laya_setup_owns_cache"))?;
    let deadline = Instant::now() + Duration::from_secs(3600);
    let signal = super::bounded_process::SignalCancellationGuard::new()?;
    let cancel = signal.cancellation().clone();
    let manifest = laya_assets::baseline();
    let manifest_digest = laya_assets::manifest_digest(&manifest).map_err(io::Error::other)?;
    let versions = root.join("versions");
    fs_permissions::ensure_private_dir(&versions)?;
    let destination = versions.join(&manifest_digest[7..]);
    if destination.try_exists()? {
        verify_installation(&destination, &manifest_digest, deadline, &cancel)?;
    } else {
        let staging = versions.join(format!(".install-{}", super::ulid::new_ulid()));
        fs_permissions::ensure_private_dir(&staging)?;
        let installed = (|| -> io::Result<()> {
            let cache = root.join("artifacts");
            fs_permissions::ensure_private_dir(&cache)?;
            for asset in &manifest.assets {
                checkpoint(deadline, &cancel)?;
                let cached = cache.join(&asset.sha256);
                if cached.try_exists()? {
                    verify_file(&cached, asset.bytes, &asset.sha256, deadline, &cancel)?;
                } else {
                    if options.offline {
                        return Err(invalid(format!(
                            "laya_offline_artifact_missing:{}",
                            asset.path
                        )));
                    }
                    let pending = cache.join(format!(".download-{}", super::ulid::new_ulid()));
                    let result = (|| -> io::Result<()> {
                        let url = format!(
                            "https://huggingface.co/{}/resolve/{}/{}",
                            asset.repository, asset.revision, asset.upstream_path
                        );
                        vega_setup::download_public_asset(
                            &url,
                            &pending,
                            asset.bytes,
                            deadline,
                            &cancel,
                        )?;
                        verify_file(&pending, asset.bytes, &asset.sha256, deadline, &cancel)?;
                        fs::rename(&pending, &cached)?;
                        atomic::sync_parent_directory(&cache)
                    })();
                    if result.is_err() {
                        let _ = fs::remove_file(&pending);
                    }
                    result?;
                }
                copy_verified(
                    &cached,
                    &staging.join(&asset.path),
                    asset.bytes,
                    &asset.sha256,
                    deadline,
                    &cancel,
                )?;
            }
            atomic::write_atomic(
                &staging.join("manifest.json"),
                &serde_json::to_vec_pretty(&manifest).map_err(io::Error::other)?,
            )?;
            verify_installation(&staging, &manifest_digest, deadline, &cancel)?;
            checkpoint(deadline, &cancel)?;
            fs::rename(&staging, &destination)?;
            atomic::sync_parent_directory(&versions)
        })();
        // Only our unique staging tree is removed. Complete older versions and
        // runtime bindings remain untouched on interrupted/failed installation.
        if installed.is_err() {
            let _ = fs::remove_dir_all(&staging);
        }
        installed?;
    }
    if let Some(path) = &options.runtime {
        let runtime = path.canonicalize()?;
        let runtime_sha256 = vega_runtime::runtime_digest(&runtime, deadline, &cancel)?;
        let cpu_runtime = options
            .cpu_runtime
            .as_ref()
            .map(|path| {
                let runtime = path.canonicalize()?;
                let runtime_sha256 = vega_runtime::runtime_digest(&runtime, deadline, &cancel)?;
                Ok::<_, io::Error>(RuntimePin {
                    runtime,
                    runtime_sha256,
                })
            })
            .transpose()?;
        vega_runtime::register_at(
            &root,
            Installation {
                directory: destination.clone(),
                manifest_digest: manifest_digest.clone(),
                runtime,
                runtime_sha256,
                cpu_runtime,
            },
        )?;
    }
    Ok(SetupReport {
        schema: "decapod.laya-setup.v1",
        model_directory: destination,
        manifest_digest: manifest_digest.clone(),
        runtime_registered: options.runtime.is_some(),
        configuration_toml: format!(
            "[laya]\nmode = \"required\"\nmodel_manifest_digest = \"{manifest_digest}\"\n"
        ),
        accuracy_scope: "Pinned upstream calibration and numerical compatibility; readiness-domain accuracy is not established",
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cache_copy_preserves_integrity_and_single_link_contract() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("cache");
        atomic::write_atomic(&source, b"pinned model").unwrap();
        let destination = root.path().join("installed");
        let digest = laya_assets::hash(b"pinned model");
        copy_verified(
            &source,
            &destination,
            12,
            &digest[7..],
            Instant::now() + Duration::from_secs(2),
            &Cancellation::default(),
        )
        .unwrap();
        assert_eq!(fs::read(&destination).unwrap(), b"pinned model");
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            assert_eq!(fs::metadata(&source).unwrap().nlink(), 1);
            assert_eq!(fs::metadata(&destination).unwrap().nlink(), 1);
            assert_ne!(
                fs::metadata(&source).unwrap().ino(),
                fs::metadata(&destination).unwrap().ino()
            );
        }
        assert!(
            copy_verified(
                &source,
                &root.path().join("bad"),
                12,
                &"0".repeat(64),
                Instant::now() + Duration::from_secs(2),
                &Cancellation::default()
            )
            .is_err()
        );
        assert!(!root.path().join("bad").exists());
    }
    #[cfg(unix)]
    #[test]
    fn unsafe_cache_hardlink_is_rejected_without_touching_previous_installation() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("cache");
        atomic::write_atomic(&source, b"pinned model").unwrap();
        fs::hard_link(&source, root.path().join("alias")).unwrap();
        let previous = root.path().join("previous");
        atomic::write_atomic(&previous, b"working").unwrap();
        let error = copy_verified(
            &source,
            &root.path().join("new"),
            12,
            &laya_assets::hash(b"pinned model")[7..],
            Instant::now() + Duration::from_secs(2),
            &Cancellation::default(),
        )
        .unwrap_err();
        assert!(error.to_string().contains("STORAGE_UNSAFE_PERMISSIONS"));
        assert_eq!(fs::read(&previous).unwrap(), b"working");
        assert!(!root.path().join("new").exists());
    }
}
