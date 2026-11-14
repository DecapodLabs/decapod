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
const ARCHIVE_NAME: &str = "todo.db.legacy-recovery-v1.archive";

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
        Ok(FileIdentity {
            device: metadata.dev(),
            inode: metadata.ino(),
        })
    }
    #[cfg(not(unix))]
    {
        let _ = metadata;
        Err(invalid(
            "this physical recovery protocol requires Unix file identities",
        ))
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
        Ok(sha256(
            &serde_json::to_vec(self).map_err(|_| invalid("binding encoding failed"))?,
        ))
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Receipt {
    schema: String,
    phase: String,
    review_sha256: String,
    binding: Binding,
    completed_by: Option<String>,
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
    links: u64,
}

fn snapshot(
    path: &Path,
    output: &Path,
    ceiling: u64,
    deadline: Instant,
) -> Result<Snapshot, DecapodError> {
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
        if count == 0 {
            break;
        }
        total += count as u64;
        if total > ceiling {
            return Err(invalid("source grew beyond the snapshot size bound"));
        }
        hash.update(&buffer[..count]);
        target.write_all(&buffer[..count])?;
    }
    let source_identity = identity(&before)?;
    let after = fs::symlink_metadata(path)?;
    if !after.is_file() || source_identity != identity(&after)? || total != before.len() {
        return Err(invalid("source identity changed during review"));
    }
    #[cfg(unix)]
    let links = {
        use std::os::unix::fs::MetadataExt;
        before.nlink()
    };
    #[cfg(not(unix))]
    let links = 0;
    Ok(Snapshot {
        digest: format!("sha256:{:x}", hash.finalize()),
        identity: source_identity,
        bytes: total,
        path: output.to_path_buf(),
        links,
    })
}

fn quiescent(root: &Path) -> Result<(), DecapodError> {
    for name in ["decapod.db", "todo.db"] {
        for suffix in ["-wal", "-shm", "-journal"] {
            match fs::symlink_metadata(root.join(format!("{name}{suffix}"))) {
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
                Err(error) => return Err(error.into()),
                Ok(_) => {
                    return Err(invalid(
                        "SQLite sidecars require quiescence; they are never removed as recovery",
                    ));
                }
            }
        }
    }
    Ok(())
}

fn store_identity(root: &Path) -> Result<String, DecapodError> {
    let canonical = root.canonicalize()?;
    Ok(sha256(
        canonical
            .to_str()
            .ok_or_else(|| invalid("store path is not UTF-8"))?
            .as_bytes(),
    ))
}

fn classify(source: &Snapshot, canonical: &Snapshot) -> Result<&'static str, DecapodError> {
    if source.bytes == 0 {
        return Ok("empty");
    }
    let bridge = match DactylBridge::open_local(&source.path, AccessMode::ReadOnly) {
        Ok(bridge) => bridge,
        Err(error) => {
            return Ok(
                if error.storage_failure_kind() == StorageFailureKind::Corrupt {
                    "corrupt"
                } else {
                    "unverifiable"
                },
            );
        }
    };
    if let Err(error) = bridge.verify_integrity() {
        return Ok(
            if error.storage_failure_kind() == StorageFailureKind::Corrupt {
                "corrupt"
            } else {
                "unverifiable"
            },
        );
    }
    drop(bridge);
    if source.digest == canonical.digest {
        return Ok("canonical_copy");
    }
    let conn =
        Connection::open_with_options(&source.path, AccessMode::ReadOnly, Duration::from_secs(2))?;
    let objects: u64 = conn.query_row(
        "SELECT COUNT(*) FROM sqlite_schema WHERE name NOT LIKE 'sqlite_%'",
        [],
        |row| row.get(0),
    )?;
    Ok(if objects == 0 { "empty" } else { "unmatched" })
}

fn review_locked(
    root: &Path,
    actor: &str,
    source_path: &Path,
    allow_archive_link: bool,
) -> Result<(Value, Option<Binding>), DecapodError> {
    quiescent(root)?;
    let temporary = tempfile::tempdir()?;
    let deadline = Instant::now() + Duration::from_secs(30);
    let canonical = snapshot(
        &root.join("decapod.db"),
        &temporary.path().join("canonical.db"),
        MAX_CANONICAL_BYTES,
        deadline,
    )?;
    let source = snapshot(
        source_path,
        &temporary.path().join("legacy.db"),
        MAX_SOURCE_BYTES,
        deadline,
    )?;
    if source.identity == canonical.identity {
        return Err(invalid("legacy path aliases the canonical datastore"));
    }
    if canonical.links != 1 || (source.links != 1 && !(allow_archive_link && source.links == 2)) {
        return Err(invalid("unrecognized hard-linked input is not admissible"));
    }
    let conn = Connection::open_with_options(
        &canonical.path,
        AccessMode::ReadOnly,
        Duration::from_secs(2),
    )?;
    let budget = super::task_journal::VerifyBudget {
        page_size: 128,
        max_checkpoint_rows: Some(10_000),
        max_task_rows: Some(10_000),
    };
    let journal = super::task_journal::verify_with_budget(&conn, &budget, None)?;
    if journal["status"] != "ok" || journal["complete"] != true || journal["journal_valid"] != true
    {
        return Err(invalid(
            "canonical task journal is not completely verified within the review budget",
        ));
    }
    let classification = classify(&source, &canonical)?;
    let authority = policy::authorize_rbac_exception(root, actor, "todo.legacy.reconcile");
    let authority_status = match &authority {
        Ok(_) => "authorized",
        Err(error) if error.to_string().contains("RBAC_PERMISSION_DENIED") => "permission_denied",
        Err(_) => "policy_unavailable",
    };
    let supported = matches!(classification, "empty" | "canonical_copy");
    let store_id = store_identity(root)?;
    let binding = authority
        .ok()
        .filter(|_| supported)
        .map(|policy_sha256| Binding {
            schema: SCHEMA.to_string(),
            store_identity: store_id,
            canonical_identity: canonical.identity,
            canonical_sha256: canonical.digest.clone(),
            source_identity: source.identity,
            source_sha256: source.digest.clone(),
            source_bytes: source.bytes,
            classification: classification.to_string(),
            policy_sha256,
            actor: actor.to_string(),
        });
    let review = binding.as_ref().map(Binding::digest).transpose()?;
    Ok((
        json!({
            "schema": SCHEMA, "status": if binding.is_some() { "ok" } else { "blocked" },
            "operation": "review", "classification": classification,
            "review_sha256": review, "source_sha256": source.digest,
            "canonical_sha256": canonical.digest, "source_bytes": source.bytes,
            "canonical_journal_verified": true, "source_preserved": true,
            "can_reconcile": binding.is_some(), "required_permission": "todo.legacy.reconcile",
            "authority_status": authority_status,
            "identity_assurance": "cooperative_not_authenticated", "mutations_replayed": 0,
            "publication_ready": false, "completion_proven": false,
            "next_action": if !supported { "preserve inputs; unmatched layouts/history require separately reviewed reconciliation" } else if binding.is_none() { "obtain the existing canonical journal-repair grant, then review again" } else { "review these exact inputs before requesting reconciliation" }
        }),
        binding,
    ))
}

fn is_digest(value: &str) -> bool {
    value.strip_prefix("sha256:").is_some_and(|hex| {
        hex.len() == 64
            && hex
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    })
}

fn valid_actor(actor: &str) -> bool {
    !actor.is_empty()
        && actor.len() <= 128
        && actor
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"-_.:".contains(&byte))
}

fn exists(path: &Path) -> Result<bool, DecapodError> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error.into()),
    }
}

fn load_receipt(root: &Path) -> Result<Option<Receipt>, DecapodError> {
    let path = root.join(RECEIPT_NAME);
    if !exists(&path)? {
        return Ok(None);
    }
    let mut bytes = Vec::new();
    open_source(&path)?
        .take(MAX_RECEIPT_BYTES + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_RECEIPT_BYTES {
        return Err(invalid("recovery receipt exceeds its byte budget"));
    }
    let receipt: Receipt = serde_json::from_slice(&bytes)
        .map_err(|_| invalid("recovery receipt is corrupt or unsupported"))?;
    if receipt.schema != SCHEMA
        || receipt.binding.schema != SCHEMA
        || !matches!(receipt.phase.as_str(), "prepared" | "committed")
        || !matches!(
            receipt.binding.classification.as_str(),
            "empty" | "canonical_copy"
        )
        || !is_digest(&receipt.review_sha256)
        || !is_digest(&receipt.binding.source_sha256)
        || !is_digest(&receipt.binding.canonical_sha256)
        || !is_digest(&receipt.binding.policy_sha256)
        || receipt.binding.source_bytes > MAX_SOURCE_BYTES
        || !valid_actor(&receipt.binding.actor)
        || (receipt.phase == "prepared" && receipt.completed_by.is_some())
        || (receipt.phase == "committed"
            && !receipt.completed_by.as_deref().is_some_and(valid_actor))
        || receipt.binding.store_identity != store_identity(root)?
        || receipt.binding.digest()? != receipt.review_sha256
        || (receipt.binding.classification == "canonical_copy"
            && receipt.binding.source_sha256 != receipt.binding.canonical_sha256)
    {
        return Err(invalid(
            "recovery receipt identity or integrity cannot be established",
        ));
    }
    Ok(Some(receipt))
}

fn save_receipt(root: &Path, receipt: &Receipt) -> Result<(), DecapodError> {
    let bytes = serde_json::to_vec(receipt).map_err(|_| invalid("receipt encoding failed"))?;
    if bytes.len() as u64 > MAX_RECEIPT_BYTES {
        return Err(invalid("receipt exceeds its byte budget"));
    }
    crate::core::atomic::write_atomic(&root.join(RECEIPT_NAME), &bytes)?;
    Ok(())
}

/// The physical operation receipt never carries task state or completion proof.
/// An interrupted operation must be explicitly resumed before normal startup.
pub(super) fn guard_pending(root: &Path) -> Result<(), DecapodError> {
    if let Some(receipt) = load_receipt(root)?
        && receipt.phase != "committed"
    {
        return Err(DecapodError::ValidationError(format!(
            "LEGACY_TODO_RECOVERY_PENDING: recovery is prepared but not committed; resume todo journal reconcile-legacy --expected-review {}",
            receipt.review_sha256
        )));
    }
    Ok(())
}

fn check_archive(root: &Path, receipt: &Receipt) -> Result<(), DecapodError> {
    let temporary = tempfile::tempdir()?;
    let archive = snapshot(
        &root.join(ARCHIVE_NAME),
        &temporary.path().join("archive.db"),
        MAX_SOURCE_BYTES,
        Instant::now() + Duration::from_secs(30),
    )?;
    if archive.digest != receipt.binding.source_sha256
        || archive.identity != receipt.binding.source_identity
        || archive.bytes != receipt.binding.source_bytes
    {
        return Err(invalid(
            "archive identity or content changed; historical outcome cannot authorize current recovery",
        ));
    }
    Ok(())
}

fn check_canonical(root: &Path, receipt: &Receipt) -> Result<(), DecapodError> {
    let temporary = tempfile::tempdir()?;
    let current = snapshot(
        &root.join("decapod.db"),
        &temporary.path().join("canonical.db"),
        MAX_CANONICAL_BYTES,
        Instant::now() + Duration::from_secs(30),
    )?;
    if current.digest != receipt.binding.canonical_sha256
        || current.identity != receipt.binding.canonical_identity
    {
        return Err(invalid(
            "canonical state changed after review; prepared outcome remains unresolved",
        ));
    }
}

fn committed_report(root: &Path, receipt: &Receipt) -> Result<Value, DecapodError> {
    check_archive(root, receipt)?;
    let reappeared = exists(&root.join("todo.db"))?;
    Ok(json!({
        "schema": SCHEMA, "status": if reappeared { "blocked" } else { "ok" },
        "operation": "reconcile", "operation_phase": "committed", "already_committed": true,
        "review_sha256": receipt.review_sha256, "source_sha256": receipt.binding.source_sha256,
        "archive": ARCHIVE_NAME, "source_reappeared": reappeared,
        "canonical_current_validation_performed": false, "mutations_replayed": 0,
        "identity_assurance": "cooperative_not_authenticated",
        "publication_ready": false, "completion_proven": false,
        "next_action": if reappeared { "preserve the new input; the historical receipt does not reconcile it" } else { "historical physical outcome retained; use ordinary current-state validation for task acceptance" }
    }))
}

/// Review, prepare, or explicitly reconcile one quiesced, already-canonical input.
/// No SQL import, task replay, canonical replacement or proof refresh occurs.
pub(super) fn run(
    root: &Path,
    actor: &str,
    expected: Option<&str>,
    prepare_only: bool,
) -> Result<Value, DecapodError> {
    if super::DbBroker::new(root).is_cloud() {
        return Err(invalid(
            "cloud authority never falls back to local recovery",
        ));
    }
    if !valid_actor(actor) {
        return Err(invalid("invalid local actor identity"));
    }
    if expected.is_some_and(|value| !is_digest(value)) || (prepare_only && expected.is_none()) {
        return Err(invalid(
            "expected review must be the exact SHA-256 from a supported review",
        ));
    }
    let authority = if expected.is_some() {
        Some(policy::authorize_rbac_exception(
            root,
            actor,
            "todo.legacy.reconcile",
        )?)
    } else {
        None
    };
    fs_permissions::check_directory(root, false)?;
    let _canonical = StorageLock::acquire(
        &root.join("decapod.db"),
        StorageLockMode::Exclusive,
        Duration::from_secs(5),
    )?;
    let _legacy = StorageLock::acquire(
        &root.join("todo.db"),
        StorageLockMode::Exclusive,
        Duration::from_secs(5),
    )?;
    let prior = load_receipt(root)?;
    if let Some(receipt) = prior.as_ref() {
        if expected.is_some_and(|value| value != receipt.review_sha256) {
            return Err(invalid(
                "another recovery operation already owns the receipt",
            ));
        }
        if receipt.phase == "committed" {
            return committed_report(root, receipt);
        }
    }
    let source_path = if exists(&root.join("todo.db"))? {
        root.join("todo.db")
    } else if prior.is_some() {
        root.join(ARCHIVE_NAME)
    } else {
        return Err(invalid("legacy input is absent; no recovery is inferred"));
    };
    let has_archive = exists(&root.join(ARCHIVE_NAME))?;
    if has_archive {
        check_archive(
            root,
            prior
                .as_ref()
                .ok_or_else(|| invalid("unattributed archive exists; refusing overwrite"))?,
        )?;
    }
    let binding_actor = prior
        .as_ref()
        .map_or(actor, |receipt| receipt.binding.actor.as_str());
    let (mut report, binding) = review_locked(root, binding_actor, &source_path, has_archive)?;
    let Some(binding) = binding else {
        return Ok(report);
    };
    let digest = binding.digest()?;
    if let Some(receipt) = prior.as_ref()
        && binding != receipt.binding
    {
        return Err(invalid("inputs or policy changed after preparation"));
    }
    let Some(expected) = expected else {
        if policy::authorize_rbac_exception(root, actor, "todo.legacy.reconcile").is_err() {
            report["status"] = json!("blocked");
            report["can_reconcile"] = json!(false);
        }
        return Ok(report);
    };
    if digest != expected || authority.as_ref() != Some(&binding.policy_sha256) {
        return Err(invalid(
            "review is stale or belongs to different inputs or policy",
        ));
    }
    let mut receipt = prior.unwrap_or(Receipt {
        schema: SCHEMA.to_string(),
        phase: "prepared".to_string(),
        review_sha256: digest,
        binding,
        completed_by: None,
    });
    save_receipt(root, &receipt)?;
    if prepare_only {
        return Ok(
            json!({"schema": SCHEMA, "status": "ok", "operation_phase": "prepared", "review_sha256": receipt.review_sha256, "source_preserved": true, "mutations_replayed": 0, "publication_ready": false, "completion_proven": false}),
        );
    }
    // Recheck authority immediately before physical publication. The receipt is
    // a local operation journal, not independently authenticated authorization.
    if policy::authorize_rbac_exception(root, actor, "todo.legacy.reconcile")?
        != receipt.binding.policy_sha256
    {
        return Err(invalid("policy changed before archival"));
    }
    check_canonical(root, &receipt)?;
    if !has_archive {
        fs::hard_link(root.join("todo.db"), root.join(ARCHIVE_NAME))?;
    }
    check_archive(root, &receipt)?;
    open_source(&root.join(ARCHIVE_NAME))?.sync_all()?;
    crate::core::atomic::sync_parent_directory(root)?;
    if exists(&root.join("todo.db"))? {
        let temporary = tempfile::tempdir()?;
        let current = snapshot(
            &root.join("todo.db"),
            &temporary.path().join("current.db"),
            MAX_SOURCE_BYTES,
            Instant::now() + Duration::from_secs(30),
        )?;
        if current.digest != receipt.binding.source_sha256
            || current.identity != receipt.binding.source_identity
            || current.links != 2
        {
            return Err(invalid(
                "legacy input changed before unlink; archive and source preserved",
            ));
        }
        if policy::authorize_rbac_exception(root, actor, "todo.legacy.reconcile")?
            != receipt.binding.policy_sha256
        {
            return Err(invalid("policy changed before retiring legacy path"));
        }
        fs::remove_file(root.join("todo.db"))?;
        crate::core::atomic::sync_parent_directory(root)?;
    }
    check_canonical(root, &receipt)?;
    receipt.phase = "committed".to_string();
    receipt.completed_by = Some(actor.to_string());
    save_receipt(root, &receipt)?;
    let mut result = committed_report(root, &receipt)?;
    result["already_committed"] = json!(false);
    result["canonical_bytes_preserved"] = json!(true);
    Ok(result)
}
