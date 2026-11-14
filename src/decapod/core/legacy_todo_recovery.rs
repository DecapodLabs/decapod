//! Versioned review of mixed local task stores. No source is opened by SQLite.
//!
//! V1 can retire only empty input or a byte-identical canonical copy. Other
//! legacy layouts require separately reviewed reconciliation, never import.
use crate::core::dactyl::DactylBridge;
use crate::core::db::Connection;
use crate::core::error::{DecapodError, StorageFailureKind};
use crate::core::fs_permissions;
use crate::core::storage_lock::{StorageLock, StorageLockMode};
use crate::plugins::policy;
use dactyl_db::AccessMode;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

const SCHEMA: &str = "decapod.todo.legacy-reconciliation.v1";
const MAX_SOURCE_BYTES: u64 = 64 * 1024 * 1024;
const MAX_CANONICAL_BYTES: u64 = 512 * 1024 * 1024;
const MAX_RECEIPT_BYTES: u64 = 16 * 1024;
const RECEIPT_NAME: &str = "legacy-todo-recovery-v1.json";
const ARCHIVE_NAME: &str = "legacy-todo-recovery-v1.db";

fn invalid(message: &str) -> DecapodError {
    DecapodError::ValidationError(format!("LEGACY_TODO_RECONCILIATION: {message}"))
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileIdentity {
    device: u64,
    inode: u64,
}

fn identity(metadata: &fs::Metadata) -> Result<FileIdentity, DecapodError> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        Ok(FileIdentity { device: metadata.dev(), inode: metadata.ino() })
    }
    #[cfg(not(unix))]
    {
        let _ = metadata;
        Err(invalid("this physical recovery protocol requires Unix file identities"))
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Binding {
    schema: String,
    store_identity: String,
    canonical_identity: FileIdentity,
    canonical_sha256: String,
    source_identity: FileIdentity,
    source_sha256: String,
    source_bytes: u64,
    classification: String,
    policy_sha256: String,
    actor: String,
}

impl Binding {
    fn digest(&self) -> Result<String, DecapodError> {
        Ok(sha256(&serde_json::to_vec(self).map_err(|_| invalid("binding encoding failed"))?))
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Receipt {
    schema: String,
    phase: String,
    review_sha256: String,
    binding: Binding,
}

fn sha256(bytes: &[u8]) -> String {
    format!("sha256:{:x}", Sha256::digest(bytes))
}

fn open_source(path: &Path) -> Result<File, DecapodError> {
    fs_permissions::check_file(path, false)?;
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    let file = options.open(path)?;
    if !file.metadata()?.is_file() {
        return Err(invalid("source must be a regular file"));
    }
    Ok(file)
}

struct Snapshot {
    digest: String,
    identity: FileIdentity,
    bytes: u64,
    path: PathBuf,
}

fn snapshot(path: &Path, output: &Path, ceiling: u64, deadline: Instant) -> Result<Snapshot, DecapodError> {
    let mut file = open_source(path)?;
    let before = file.metadata()?;
    if before.len() > ceiling {
        return Err(invalid("source exceeds the bounded snapshot size"));
    }
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    let mut target = fs_permissions::open_private_file(output, &mut options)?;
    let mut hash = Sha256::new();
    let mut total = 0_u64;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        if Instant::now() >= deadline {
            return Err(invalid("snapshot deadline exceeded; no source was changed"));
        }
        let count = file.read(&mut buffer)?;
        if count == 0 { break; }
        total += count as u64;
        if total > ceiling { return Err(invalid("source grew beyond the snapshot size bound")); }
        hash.update(&buffer[..count]);
        target.write_all(&buffer[..count])?;
    }
    let source_identity = identity(&before)?;
    let after = fs::symlink_metadata(path)?;
    if !after.is_file() || source_identity != identity(&after)? || total != before.len() {
        return Err(invalid("source identity changed during review"));
    }
    Ok(Snapshot { digest: format!("sha256:{:x}", hash.finalize()), identity: source_identity, bytes: total, path: output.to_path_buf() })
}

fn quiescent(root: &Path) -> Result<(), DecapodError> {
    for name in ["decapod.db", "todo.db"] {
        for suffix in ["-wal", "-shm", "-journal"] {
            match fs::symlink_metadata(root.join(format!("{name}{suffix}"))) {
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
                Err(error) => return Err(error.into()),
                Ok(_) => return Err(invalid("SQLite sidecars require quiescence; they are never removed as recovery")),
            }
        }
    }
    Ok(())
}

fn store_identity(root: &Path) -> Result<String, DecapodError> {
    let canonical = root.canonicalize()?;
    Ok(sha256(canonical.to_str().ok_or_else(|| invalid("store path is not UTF-8"))?.as_bytes()))
}

fn classify(source: &Snapshot, canonical: &Snapshot) -> Result<&'static str, DecapodError> {
    if source.bytes == 0 { return Ok("empty"); }
    let bridge = match DactylBridge::open_local(&source.path, AccessMode::ReadOnly) {
        Ok(bridge) => bridge,
        Err(error) => return Ok(if error.storage_failure_kind() == StorageFailureKind::Corrupt { "corrupt" } else { "unverifiable" }),
    };
    if let Err(error) = bridge.verify_integrity() {
        return Ok(if error.storage_failure_kind() == StorageFailureKind::Corrupt { "corrupt" } else { "unverifiable" });
    }
    drop(bridge);
    if source.digest == canonical.digest { return Ok("canonical_copy"); }
    let conn = Connection::open_with_options(&source.path, AccessMode::ReadOnly, Duration::from_secs(2))?;
    let objects: u64 = conn.query_row("SELECT COUNT(*) FROM sqlite_schema WHERE name NOT LIKE 'sqlite_%'", [], |row| row.get(0))?;
    Ok(if objects == 0 { "empty" } else { "unmatched" })
}

fn review_locked(root: &Path, actor: &str) -> Result<(Value, Option<Binding>), DecapodError> {
    quiescent(root)?;
    let temporary = tempfile::tempdir()?;
    let deadline = Instant::now() + Duration::from_secs(30);
    let canonical = snapshot(&root.join("decapod.db"), &temporary.path().join("canonical.db"), MAX_CANONICAL_BYTES, deadline)?;
    let source = snapshot(&root.join("todo.db"), &temporary.path().join("legacy.db"), MAX_SOURCE_BYTES, deadline)?;
    if source.identity == canonical.identity { return Err(invalid("legacy path aliases the canonical datastore")); }
    let conn = Connection::open_with_options(&canonical.path, AccessMode::ReadOnly, Duration::from_secs(2))?;
    let journal = super::task_journal::verify(&conn)?;
    if journal["status"] != "ok" { return Err(invalid("canonical task journal is not verified; no reconciliation is admissible")); }
    let classification = classify(&source, &canonical)?;
    let authority = policy::authorize_rbac_exception(root, actor, "todo.journal.repair");
    let supported = matches!(classification, "empty" | "canonical_copy");
    let store_id = store_identity(root)?;
    let binding = authority.ok().filter(|_| supported).map(|policy_sha256| Binding {
        schema: SCHEMA.to_string(), store_identity: store_id,
        canonical_identity: canonical.identity, canonical_sha256: canonical.digest.clone(),
        source_identity: source.identity, source_sha256: source.digest.clone(), source_bytes: source.bytes,
        classification: classification.to_string(), policy_sha256, actor: actor.to_string(),
    });
    let review = binding.as_ref().map(Binding::digest).transpose()?;
    Ok((json!({
        "schema": SCHEMA, "status": if binding.is_some() { "ok" } else { "blocked" },
        "operation": "review", "classification": classification,
        "review_sha256": review, "source_sha256": source.digest,
        "canonical_sha256": canonical.digest, "source_bytes": source.bytes,
        "canonical_journal_verified": true, "source_preserved": true,
        "can_reconcile": binding.is_some(), "required_permission": "todo.journal.repair",
        "identity_assurance": "cooperative_not_authenticated", "mutations_replayed": 0,
        "publication_ready": false, "completion_proven": false,
        "next_action": if !supported { "preserve inputs; unmatched layouts/history require separately reviewed reconciliation" } else if binding.is_none() { "obtain the existing canonical journal-repair grant, then review again" } else { "review these exact inputs before requesting reconciliation" }
    }), binding))
}

/// Read-only physical review; the kernel's actual journal verifier is retained.
pub(super) fn review(root: &Path, actor: &str) -> Result<Value, DecapodError> {
    if super::DbBroker::new(root).is_cloud() { return Err(invalid("cloud authority never falls back to local recovery")); }
    fs_permissions::check_directory(root, false)?;
    let _canonical = StorageLock::acquire(&root.join("decapod.db"), StorageLockMode::Exclusive, Duration::from_secs(5))?;
    let _legacy = StorageLock::acquire(&root.join("todo.db"), StorageLockMode::Exclusive, Duration::from_secs(5))?;
    let (report, _) = review_locked(root, actor)?;
    Ok(report)
}
