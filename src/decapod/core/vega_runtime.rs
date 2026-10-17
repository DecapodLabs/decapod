//! Per-maintainer runtime binding, owned by explicit Vega setup, outside Git.
//! A registration pins local executable bytes; it is not build provenance,
//! numerical parity, or evidence that inference has run.
use super::bounded_process::Cancellation;
use super::{fs_permissions, vega_readiness::Installation, vega_setup};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::{self, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::Instant;

const SCHEMA: &str = "decapod.vega-local-runtime.v1";
const MAX_RUNTIME: u64 = 512 * 1024 * 1024;
const MAX_BINDING: usize = 8192;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct LocalBinding {
    schema: String,
    installation: Installation,
}

fn invalid(reason: &str) -> io::Error {
    io::Error::other(reason.to_owned())
}

/// Accept only this host's native executable format. Never accept a script or
/// foreign architecture and rely on its claimed response device afterwards.
fn native_header(file: &mut fs::File, length: u64) -> io::Result<()> {
    let mut header = [0; 64];
    file.read_exact(&mut header)?;
    let matches = if cfg!(target_os = "macos") {
        header[..4] == [0xcf, 0xfa, 0xed, 0xfe]
            && match std::env::consts::ARCH {
                "aarch64" => header[4..8] == [12, 0, 0, 1],
                "x86_64" => header[4..8] == [7, 0, 0, 1],
                _ => false,
            }
    } else if cfg!(target_os = "linux") {
        header[..6] == [0x7f, b'E', b'L', b'F', 2, 1]
            && match std::env::consts::ARCH {
                "aarch64" => header[18..20] == [183, 0],
                "x86_64" => header[18..20] == [62, 0],
                _ => false,
            }
    } else if cfg!(target_os = "windows") && header[..2] == *b"MZ" {
        let offset = u32::from_le_bytes(header[60..64].try_into().unwrap()) as u64;
        if offset > length.saturating_sub(6) {
            return Err(invalid("vega_runtime_invalid_pe_header"));
        }
        file.seek(SeekFrom::Start(offset))?;
        let mut pe = [0; 6];
        file.read_exact(&mut pe)?;
        pe[..4] == *b"PE\0\0"
            && match std::env::consts::ARCH {
                "aarch64" => pe[4..] == [0x64, 0xaa],
                "x86_64" => pe[4..] == [0x64, 0x86],
                _ => false,
            }
    } else {
        false
    };
    if !matches {
        return Err(invalid("vega_runtime_not_native_executable"));
    }
    file.rewind()
}

pub(crate) fn runtime_digest(
    path: &Path,
    deadline: Instant,
    cancel: &Cancellation,
) -> io::Result<String> {
    if Instant::now() >= deadline || cancel.is_cancelled() {
        return Err(invalid("vega_runtime_verification_cancelled_or_timeout"));
    }
    if !path.is_absolute() || path.canonicalize()? != path {
        return Err(invalid("vega_runtime_path_not_canonical"));
    }
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    let mut file = options.open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() || !(64..=MAX_RUNTIME).contains(&metadata.len()) {
        return Err(invalid("vega_runtime_not_bounded_regular_file"));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        let mode = metadata.permissions().mode();
        let uid = unsafe { libc::geteuid() };
        if (metadata.uid() != uid && metadata.uid() != 0)
            || metadata.nlink() != 1
            || mode & 0o022 != 0
            || mode & 0o111 == 0
        {
            return Err(invalid("vega_runtime_unsafe_permissions_or_links"));
        }
    }
    native_header(&mut file, metadata.len())?;
    let mut hasher = Sha256::new();
    let mut buffer = [0; 65536];
    let mut count = 0u64;
    loop {
        if Instant::now() >= deadline || cancel.is_cancelled() {
            return Err(invalid("vega_runtime_verification_cancelled_or_timeout"));
        }
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        count += n as u64;
        if count > MAX_RUNTIME {
            return Err(invalid("vega_runtime_size_changed"));
        }
        hasher.update(&buffer[..n]);
    }
    if count != metadata.len() {
        return Err(invalid("vega_runtime_size_changed"));
    }
    Ok(format!("sha256:{:x}", hasher.finalize()))
}

fn binding_path(root: &Path, manifest: &str) -> io::Result<PathBuf> {
    if !super::vega_setup::valid_digest(manifest) {
        return Err(invalid("vega_local_binding_manifest_required"));
    }
    Ok(root
        .join("runtimes")
        .join(format!("{}.json", &manifest[7..])))
}

pub(crate) fn register(installation: Installation) -> io::Result<()> {
    register_at(&vega_setup::default_cache()?, installation)
}

fn validate_binding(binding: &LocalBinding, manifest: &str) -> io::Result<()> {
    let installation = &binding.installation;
    if binding.schema != SCHEMA
        || installation.manifest_digest != manifest
        || !installation.directory.is_dir()
        || installation.directory.canonicalize()? != installation.directory
        || installation.runtime.canonicalize()? != installation.runtime
        || !vega_setup::valid_digest(&installation.runtime_sha256)
    {
        return Err(invalid("vega_local_binding_invalid"));
    }
    Ok(())
}

fn register_at(root: &Path, installation: Installation) -> io::Result<()> {
    let path = binding_path(root, &installation.manifest_digest)?;
    let binding = LocalBinding {
        schema: SCHEMA.into(),
        installation,
    };
    validate_binding(&binding, &binding.installation.manifest_digest)?;
    let bytes = serde_json::to_vec_pretty(&binding).map_err(io::Error::other)?;
    if bytes.len() > MAX_BINDING {
        return Err(invalid("vega_local_binding_overflow"));
    }
    fs_permissions::ensure_private_dir(root)?;
    fs_permissions::ensure_private_dir(path.parent().unwrap())?;
    fs_permissions::write_private(path, bytes)
}

pub(crate) fn resolve(manifest: &str) -> io::Result<Installation> {
    resolve_at(&vega_setup::default_cache()?, manifest)
}

fn resolve_at(root: &Path, manifest: &str) -> io::Result<Installation> {
    let path = binding_path(root, manifest)?;
    let mut options = OpenOptions::new();
    options.read(true);
    let mut file = fs_permissions::open_private_file(&path, &mut options)?;
    if !file.metadata()?.is_file() {
        return Err(invalid("vega_local_binding_not_regular"));
    }
    let mut bytes = Vec::new();
    file.by_ref()
        .take((MAX_BINDING + 1) as u64)
        .read_to_end(&mut bytes)?;
    if bytes.len() > MAX_BINDING {
        return Err(invalid("vega_local_binding_overflow"));
    }
    let binding: LocalBinding = serde_json::from_slice(&bytes).map_err(io::Error::other)?;
    validate_binding(&binding, manifest)?;
    Ok(binding.installation)
}

#[cfg(all(
    test,
    any(target_os = "macos", target_os = "linux", target_os = "windows"),
    any(target_arch = "aarch64", target_arch = "x86_64")
))]
mod tests {
    use super::*;
    use std::time::Duration;

    // Format fixtures are never executed and do not establish native inference.
    fn fixture(root: &Path, label: u8) -> Installation {
        let directory = root.join("model");
        fs::create_dir(&directory).unwrap();
        let runtime = root.join("worker");
        let mut bytes = [0u8; 128];
        if cfg!(target_os = "macos") {
            bytes[..4].copy_from_slice(&[0xcf, 0xfa, 0xed, 0xfe]);
            bytes[4..8].copy_from_slice(if cfg!(target_arch = "aarch64") {
                &[12, 0, 0, 1]
            } else {
                &[7, 0, 0, 1]
            });
        } else if cfg!(target_os = "linux") {
            bytes[..6].copy_from_slice(&[0x7f, b'E', b'L', b'F', 2, 1]);
            bytes[18..20].copy_from_slice(if cfg!(target_arch = "aarch64") {
                &[183, 0]
            } else {
                &[62, 0]
            });
        } else {
            bytes[..2].copy_from_slice(b"MZ");
            bytes[60..64].copy_from_slice(&64u32.to_le_bytes());
            bytes[64..68].copy_from_slice(b"PE\0\0");
            bytes[68..70].copy_from_slice(if cfg!(target_arch = "aarch64") {
                &[0x64, 0xaa]
            } else {
                &[0x64, 0x86]
            });
        }
        bytes[127] = label;
        fs::write(&runtime, bytes).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&runtime, fs::Permissions::from_mode(0o700)).unwrap();
        }
        let runtime = runtime.canonicalize().unwrap();
        let runtime_sha256 = runtime_digest(
            &runtime,
            Instant::now() + Duration::from_secs(10),
            &Cancellation::default(),
        )
        .unwrap();
        Installation {
            directory: directory.canonicalize().unwrap(),
            manifest_digest: super::super::vega_readiness::hash(b"shared model"),
            runtime,
            runtime_sha256,
        }
    }

    #[test]
    fn shared_model_resolves_independent_private_local_bindings() {
        let first = tempfile::tempdir().unwrap();
        let second = tempfile::tempdir().unwrap();
        let a = fixture(first.path(), 1);
        let b = fixture(second.path(), 2);
        assert_eq!(a.manifest_digest, b.manifest_digest);
        assert_ne!(a.runtime_sha256, b.runtime_sha256);
        register_at(first.path(), a.clone()).unwrap();
        register_at(second.path(), b.clone()).unwrap();
        assert_eq!(resolve_at(first.path(), &a.manifest_digest).unwrap(), a);
        assert_eq!(resolve_at(second.path(), &b.manifest_digest).unwrap(), b);
        fs_permissions::write_private(first.path().join("active.json"), b"not runtime authority")
            .unwrap();
        assert_eq!(resolve_at(first.path(), &a.manifest_digest).unwrap(), a);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let path = binding_path(first.path(), &a.manifest_digest).unwrap();
            assert_eq!(fs::metadata(path).unwrap().permissions().mode() & 0o077, 0);
        }
    }

    #[test]
    fn invalid_registration_preserves_binding_and_malformed_records_fail_closed() {
        let root = tempfile::tempdir().unwrap();
        let installation = fixture(root.path(), 3);
        register_at(root.path(), installation.clone()).unwrap();
        let path = binding_path(root.path(), &installation.manifest_digest).unwrap();
        let before = fs::read(&path).unwrap();
        let mut invalid = installation.clone();
        invalid.runtime_sha256 = "floating".into();
        assert!(register_at(root.path(), invalid).is_err());
        assert_eq!(fs::read(&path).unwrap(), before);
        let other = super::super::vega_readiness::hash(b"other model");
        assert!(resolve_at(root.path(), &other).is_err());
        let mut record: serde_json::Value = serde_json::from_slice(&before).unwrap();
        record["schema"] = "unknown".into();
        fs_permissions::write_private(&path, serde_json::to_vec(&record).unwrap()).unwrap();
        assert!(resolve_at(root.path(), &installation.manifest_digest).is_err());
        fs_permissions::write_private(&path, vec![b'x'; 8193]).unwrap();
        assert!(resolve_at(root.path(), &installation.manifest_digest).is_err());
    }

    #[test]
    fn executable_identity_checks_format_cancellation_and_unsafe_links() {
        let root = tempfile::tempdir().unwrap();
        let installation = fixture(root.path(), 4);
        let cancel = Cancellation::default();
        cancel.cancel();
        assert!(runtime_digest(&installation.runtime, Instant::now(), &cancel).is_err());
        #[cfg(unix)]
        {
            use std::os::unix::fs::{PermissionsExt, symlink};
            let link = root.path().join("hardlink");
            fs::hard_link(&installation.runtime, &link).unwrap();
            assert!(
                runtime_digest(
                    &installation.runtime,
                    Instant::now() + Duration::from_secs(10),
                    &Cancellation::default()
                )
                .is_err()
            );
            fs::remove_file(link).unwrap();
            let link = root.path().join("symlink");
            symlink(&installation.runtime, &link).unwrap();
            assert!(
                runtime_digest(
                    &link,
                    Instant::now() + Duration::from_secs(10),
                    &Cancellation::default()
                )
                .is_err()
            );
            fs::set_permissions(&installation.runtime, fs::Permissions::from_mode(0o722)).unwrap();
            assert!(
                runtime_digest(
                    &installation.runtime,
                    Instant::now() + Duration::from_secs(10),
                    &Cancellation::default()
                )
                .is_err()
            );
            fs::set_permissions(&installation.runtime, fs::Permissions::from_mode(0o700)).unwrap();
        }
        fs::write(&installation.runtime, [0; 128]).unwrap();
        assert!(
            runtime_digest(
                &installation.runtime,
                Instant::now() + Duration::from_secs(10),
                &Cancellation::default()
            )
            .is_err()
        );
    }
}
