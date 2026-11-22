//! Versioned review and non-destructive recovery of mixed local task stores (#1657).
//!
//! Live `todo.db` is never opened directly by SQLite — all inspection runs on a
//! bounded temporary byte snapshot.
//!
//! Supported operations under explicit `todo.legacy.reconcile` canonical RBAC:
//! - Default review / retire: retires only `empty` or byte-identical `canonical_copy`.
//! - `--export <PATH>`: non-destructively writes a deterministic JSON inspection
//!   bundle (tasks, events, conflicts, digests) to a new file without mutating either DB.
//! - `--quarantine --expected-review <SHA>`: preserves original bytes in an archive
//!   and retires any reviewed legacy `todo.db` without mutating canonical `decapod.db`.
//! - `--import-missing --expected-review <SHA>`: when a valid legacy `todo.db` has
//!   missing tasks and ZERO conflicts with canonical `decapod.db`, archives the
//!   original `todo.db`, imports only the missing tasks into canonical `decapod.db`
//!   with verified task-journal checkpoints, and retires `todo.db`. Conflicting tasks
//!   fail closed (`LEGACY_TODO_CONFLICT_BLOCKED`) without mutating either store.
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
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

const SCHEMA: &str = "decapod.todo.legacy-reconciliation.v1";
const MAX_SOURCE_BYTES: u64 = 64 * 1024 * 1024;
const MAX_CANONICAL_BYTES: u64 = 512 * 1024 * 1024;
const MAX_RECEIPT_BYTES: u64 = 16 * 1024;
const MAX_LOGICAL_ROWS: usize = 10_000;
const RECEIPT_NAME: &str = "legacy-todo-recovery-v1.json";
const ARCHIVE_NAME: &str = "todo.db.legacy-recovery-v1.archive";

/// Options for `todo journal reconcile-legacy` (#1657).
#[derive(Debug, Clone, Default)]
pub struct LegacyRecoveryOptions<'a> {
    pub expected_review: Option<&'a str>,
    pub prepare_only: bool,
    pub quarantine: bool,
    pub import_missing: bool,
    pub export_path: Option<&'a Path>,
}

fn invalid(message: &str) -> DecapodError {
    DecapodError::ValidationError(format!("LEGACY_TODO_RECONCILIATION: {message}"))
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileIdentity {
    device: u64,
    inode: u64,
    owner_uid: u32,
    group_gid: u32,
    mode: u32,
}

fn identity(metadata: &fs::Metadata) -> Result<FileIdentity, DecapodError> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        Ok(FileIdentity {
            device: metadata.dev(),
            inode: metadata.ino(),
            owner_uid: metadata.uid(),
            group_gid: metadata.gid(),
            mode: metadata.mode() & 0o7777,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    mode: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    imported_task_ids: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct LegacyTaskRow {
    id: String,
    hash: String,
    title: String,
    description: String,
    tags: String,
    owner: String,
    status: String,
    created_at: String,
    updated_at: String,
    dir_path: String,
    scope: String,
    priority: String,
    category: String,
    component: String,
    assigned_to: String,
    intent_anchor: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct LegacyEventRow {
    event_id: String,
    ts: String,
    event_type: String,
    task_id: Option<String>,
    payload: String,
    actor: String,
}

#[derive(Clone, Debug, Default)]
struct LogicalInspection {
    has_tasks_table: bool,
    legacy_tasks: Vec<LegacyTaskRow>,
    legacy_events: Vec<LegacyEventRow>,
    matched_task_ids: Vec<String>,
    legacy_only_task_ids: Vec<String>,
    conflicting_task_ids: Vec<String>,
    conflict_details: Vec<Value>,
}

impl LogicalInspection {
    fn summary_json(&self) -> Value {
        json!({
            "has_tasks_table": self.has_tasks_table,
            "legacy_task_count": self.legacy_tasks.len(),
            "legacy_event_count": self.legacy_events.len(),
            "matched_task_ids": self.matched_task_ids,
            "legacy_only_task_ids": self.legacy_only_task_ids,
            "conflicting_task_ids": self.conflicting_task_ids,
            "conflicts": self.conflict_details,
        })
    }
}

fn sha256(bytes: &[u8]) -> String {
    format!("sha256:{:x}", Sha256::digest(bytes))
}

fn open_source(path: &Path, archival_link: bool) -> Result<File, DecapodError> {
    if archival_link {
        // Only the receipt-bound source/archive pair may temporarily have two
        // links. Do not relax the shared state-file policy for other callers.
        fs_permissions::check_directory(
            path.parent()
                .ok_or_else(|| invalid("source parent is absent"))?,
            false,
        )?;
    } else {
        fs_permissions::check_file(path, false)?;
    }
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
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let metadata = file.metadata()?;
        let uid = unsafe { libc::geteuid() };
        if (metadata.uid() != uid && metadata.uid() != 0)
            || metadata.mode() & 0o022 != 0
            || (metadata.nlink() != 1 && !(archival_link && metadata.nlink() == 2))
        {
            return Err(invalid(
                "source ownership, write boundary or link count is unsafe",
            ));
        }
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
    archival_link: bool,
) -> Result<Snapshot, DecapodError> {
    let mut file = open_source(path, archival_link)?;
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

fn table_columns(conn: &Connection, table: &str) -> Result<BTreeSet<String>, DecapodError> {
    let mut cols = BTreeSet::new();
    let sql = format!("PRAGMA table_info({table})");
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(1))?;
    for row in rows {
        cols.insert(row?);
    }
    Ok(cols)
}

fn read_legacy_tasks(conn: &Connection) -> Result<Vec<LegacyTaskRow>, DecapodError> {
    let cols = table_columns(conn, "tasks")?;
    if !cols.contains("id") || !cols.contains("title") {
        return Ok(Vec::new());
    }
    let col_expr = |name: &str, default_sql: &str| -> String {
        if cols.contains(name) {
            format!("COALESCE(CAST({name} AS TEXT), {default_sql})")
        } else {
            default_sql.to_string()
        }
    };
    let sql = format!(
        "SELECT id, {}, title, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {} FROM tasks ORDER BY id LIMIT {}",
        col_expr("hash", "''"),
        col_expr("description", "''"),
        col_expr("tags", "''"),
        col_expr("owner", "''"),
        col_expr("status", "'open'"),
        col_expr("created_at", "''"),
        col_expr("updated_at", "''"),
        col_expr("dir_path", "'.'"),
        col_expr("scope", "'root'"),
        col_expr("priority", "'medium'"),
        col_expr("category", "''"),
        col_expr("component", "''"),
        col_expr("assigned_to", "''"),
        col_expr("intent_anchor", "''"),
        MAX_LOGICAL_ROWS + 1
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map([], |row| {
        Ok(LegacyTaskRow {
            id: row.get(0)?,
            hash: row.get(1)?,
            title: row.get(2)?,
            description: row.get(3)?,
            tags: row.get(4)?,
            owner: row.get(5)?,
            status: row.get(6)?,
            created_at: row.get(7)?,
            updated_at: row.get(8)?,
            dir_path: row.get(9)?,
            scope: row.get(10)?,
            priority: row.get(11)?,
            category: row.get(12)?,
            component: row.get(13)?,
            assigned_to: row.get(14)?,
            intent_anchor: row.get(15)?,
        })
    })?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
        if out.len() > MAX_LOGICAL_ROWS {
            return Err(invalid(
                "legacy tasks table exceeds bounded logical review limit (10,000 rows)",
            ));
        }
    }
    Ok(out)
}

fn read_legacy_events(conn: &Connection) -> Result<Vec<LegacyEventRow>, DecapodError> {
    if conn.has_table("task_events")? {
        let mut stmt = conn.prepare(&format!(
            "SELECT event_id, ts, event_type, task_id, payload, actor FROM task_events ORDER BY rowid LIMIT {}",
            MAX_LOGICAL_ROWS + 1
        ))?;
        let rows = stmt.query_map([], |row| {
            Ok(LegacyEventRow {
                event_id: row.get(0)?,
                ts: row.get(1)?,
                event_type: row.get(2)?,
                task_id: row.get(3)?,
                payload: row.get(4)?,
                actor: row.get(5)?,
            })
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
            if out.len() > MAX_LOGICAL_ROWS {
                break;
            }
        }
        return Ok(out);
    }
    if conn.has_table("events")? {
        let mut stmt = conn.prepare(&format!(
            "SELECT event_id, ts, event_type, subject_id, payload, actor FROM events WHERE stream = 'todo' ORDER BY seq LIMIT {}",
            MAX_LOGICAL_ROWS + 1
        ))?;
        let rows = stmt.query_map([], |row| {
            Ok(LegacyEventRow {
                event_id: row.get(0)?,
                ts: row.get(1)?,
                event_type: row.get(2)?,
                task_id: row.get(3)?,
                payload: row.get(4)?,
                actor: row.get(5)?,
            })
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
            if out.len() > MAX_LOGICAL_ROWS {
                break;
            }
        }
        return Ok(out);
    }
    Ok(Vec::new())
}

fn inspect_logical_state(
    source_conn: &Connection,
    canonical_conn: &Connection,
) -> Result<LogicalInspection, DecapodError> {
    if !source_conn.has_table("tasks")? {
        return Ok(LogicalInspection::default());
    }
    let legacy_tasks = read_legacy_tasks(source_conn)?;
    let legacy_events = read_legacy_events(source_conn)?;

    let mut canonical_tasks: BTreeMap<String, (String, String, String)> = BTreeMap::new();
    if canonical_conn.has_table("tasks")? {
        let mut stmt = canonical_conn.prepare(&format!(
            "SELECT id, title, status, COALESCE(assigned_to, '') FROM tasks ORDER BY id LIMIT {}",
            MAX_LOGICAL_ROWS + 1
        ))?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        })?;
        for row in rows {
            let (id, title, status, assigned_to) = row?;
            canonical_tasks.insert(id, (title, status, assigned_to));
        }
    }

    let mut matched = Vec::new();
    let mut legacy_only = Vec::new();
    let mut conflicting = Vec::new();
    let mut conflict_details = Vec::new();

    for task in &legacy_tasks {
        match canonical_tasks.get(&task.id) {
            None => legacy_only.push(task.id.clone()),
            Some((c_title, c_status, c_assigned)) => {
                if c_title == &task.title
                    && c_status == &task.status
                    && c_assigned == &task.assigned_to
                {
                    matched.push(task.id.clone());
                } else {
                    conflicting.push(task.id.clone());
                    conflict_details.push(json!({
                        "task_id": task.id,
                        "legacy": {"title": task.title, "status": task.status, "assigned_to": task.assigned_to},
                        "canonical": {"title": c_title, "status": c_status, "assigned_to": c_assigned},
                    }));
                }
            }
        }
    }

    Ok(LogicalInspection {
        has_tasks_table: true,
        legacy_tasks,
        legacy_events,
        matched_task_ids: matched,
        legacy_only_task_ids: legacy_only,
        conflicting_task_ids: conflicting,
        conflict_details,
    })
}

fn classify(
    source: &Snapshot,
    canonical: &Snapshot,
    canonical_conn: &Connection,
) -> Result<(&'static str, LogicalInspection), DecapodError> {
    if source.bytes == 0 {
        return Ok(("empty", LogicalInspection::default()));
    }
    let bridge = match DactylBridge::open_local(&source.path, AccessMode::ReadOnly) {
        Ok(bridge) => bridge,
        Err(error) => {
            return Ok((
                if error.storage_failure_kind() == StorageFailureKind::Corrupt {
                    "corrupt"
                } else {
                    "unverifiable"
                },
                LogicalInspection::default(),
            ));
        }
    };
    if let Err(error) = bridge.verify_integrity() {
        return Ok((
            if error.storage_failure_kind() == StorageFailureKind::Corrupt {
                "corrupt"
            } else {
                "unverifiable"
            },
            LogicalInspection::default(),
        ));
    }
    drop(bridge);
    if source.digest == canonical.digest {
        return Ok(("canonical_copy", LogicalInspection::default()));
    }
    let conn =
        Connection::open_with_options(&source.path, AccessMode::ReadOnly, Duration::from_secs(2))?;
    let objects: u64 = conn.query_row(
        "SELECT COUNT(*) FROM sqlite_schema WHERE name NOT LIKE 'sqlite_%'",
        [],
        |row| row.get(0),
    )?;
    if objects == 0 {
        return Ok(("empty", LogicalInspection::default()));
    }
    let logical = inspect_logical_state(&conn, canonical_conn)?;
    if !logical.has_tasks_table {
        return Ok(("unmatched", logical));
    }
    if logical.legacy_only_task_ids.is_empty() && logical.conflicting_task_ids.is_empty() {
        Ok(("logically_equivalent", logical))
    } else {
        Ok(("divergent", logical))
    }
}

struct ReviewOutcome {
    report: Value,
    binding: Option<Binding>,
    physical_supported: bool,
    logical: LogicalInspection,
    canonical_journal_digest: Value,
}

fn review_locked(
    root: &Path,
    actor: &str,
    source_path: &Path,
    allow_archive_link: bool,
) -> Result<ReviewOutcome, DecapodError> {
    quiescent(root)?;
    let temporary = tempfile::tempdir()?;
    let deadline = Instant::now() + Duration::from_secs(30);
    let canonical = snapshot(
        &root.join("decapod.db"),
        &temporary.path().join("canonical.db"),
        MAX_CANONICAL_BYTES,
        deadline,
        false,
    )?;
    let source = snapshot(
        source_path,
        &temporary.path().join("legacy.db"),
        MAX_SOURCE_BYTES,
        deadline,
        allow_archive_link,
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
    let (classification, logical) = classify(&source, &canonical, &conn)?;
    let authority = policy::authorize_rbac_exception(root, actor, "todo.legacy.reconcile");
    let authority_status = match &authority {
        Ok(_) => "authorized",
        Err(DecapodError::ValidationError(message))
            if message.starts_with("RBAC_PERMISSION_DENIED:") =>
        {
            "permission_denied"
        }
        Err(_) => "policy_unavailable",
    };
    let physical_supported = matches!(classification, "empty" | "canonical_copy");
    let store_id = store_identity(root)?;
    let binding = authority.ok().map(|policy_sha256| Binding {
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
    let binding_digest = binding.as_ref().map(Binding::digest).transpose()?;
    let review = physical_supported
        .then_some(binding_digest.clone())
        .flatten();
    let can_reconcile = physical_supported && binding.is_some();
    let can_quarantine = binding.is_some();
    let can_import_missing = binding.is_some()
        && matches!(classification, "divergent" | "logically_equivalent")
        && logical.conflicting_task_ids.is_empty()
        && !logical.legacy_only_task_ids.is_empty();
    let next_action = if can_reconcile {
        "review these exact inputs before requesting reconciliation"
    } else if binding.is_none() {
        "obtain the explicit canonical todo.legacy.reconcile grant, then review again"
    } else if can_import_missing {
        "export with --export <path>, then run --import-missing --expected-review <quarantine_review_sha256> or --quarantine --expected-review <quarantine_review_sha256>"
    } else if classification == "divergent" && !logical.conflicting_task_ids.is_empty() {
        "conflicting task identities require human review; export with --export <path> or archive via --quarantine --expected-review <quarantine_review_sha256>"
    } else {
        "preserve inputs; export with --export <path> or archive non-destructively via --quarantine --expected-review <quarantine_review_sha256>"
    };
    let report = json!({
        "schema": SCHEMA,
        "status": if can_reconcile { "ok" } else { "blocked" },
        "operation": "review",
        "classification": classification,
        "review_sha256": review,
        "quarantine_review_sha256": binding_digest,
        "source_sha256": source.digest,
        "canonical_sha256": canonical.digest,
        "source_bytes": source.bytes,
        "canonical_journal_verified": true,
        "source_preserved": true,
        "canonical_journal_digest": journal["current_digest"],
        "source_integrity": match classification {
            "empty" if source.bytes == 0 => "not_applicable",
            "corrupt" => "failed",
            "unverifiable" => "unknown",
            _ => "ok",
        },
        "logical_summary": logical.summary_json(),
        "can_reconcile": can_reconcile,
        "can_quarantine": can_quarantine,
        "can_import_missing": can_import_missing,
        "required_permission": "todo.legacy.reconcile",
        "authority_status": authority_status,
        "identity_assurance": "cooperative_not_authenticated",
        "mutations_replayed": 0,
        "publication_ready": false,
        "completion_proven": false,
        "next_action": next_action,
    });
    Ok(ReviewOutcome {
        report,
        binding,
        physical_supported,
        logical,
        canonical_journal_digest: journal["current_digest"].clone(),
    })
}

fn write_export_bundle(
    export_path: &Path,
    outcome: &ReviewOutcome,
) -> Result<(String, String), DecapodError> {
    if fs::symlink_metadata(export_path).is_ok() {
        return Err(invalid(
            "export destination already exists; refusing to overwrite existing file",
        ));
    }
    if let Some(parent) = export_path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent)?;
    }
    let payload = json!({
        "schema": SCHEMA,
        "operation": "export",
        "classification": outcome.report["classification"],
        "source_sha256": outcome.report["source_sha256"],
        "canonical_sha256": outcome.report["canonical_sha256"],
        "source_bytes": outcome.report["source_bytes"],
        "canonical_journal_digest": outcome.canonical_journal_digest,
        "logical_summary": outcome.logical.summary_json(),
        "legacy_tasks": outcome.logical.legacy_tasks,
        "legacy_events": outcome.logical.legacy_events,
        "source_preserved": true,
        "canonical_mutated": false,
    });
    let bytes = serde_json::to_vec_pretty(&payload)
        .map_err(|_| invalid("export payload encoding failed"))?;
    if bytes.len() as u64 > MAX_SOURCE_BYTES {
        return Err(invalid("export payload exceeds bounded size limit"));
    }
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    let mut file = fs_permissions::open_private_file(export_path, &mut options)?;
    file.write_all(&bytes)?;
    file.sync_all()?;
    Ok((export_path.display().to_string(), sha256(&bytes)))
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
    open_source(&path, false)?
        .take(MAX_RECEIPT_BYTES + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_RECEIPT_BYTES {
        return Err(invalid("recovery receipt exceeds its byte budget"));
    }
    let receipt: Receipt = serde_json::from_slice(&bytes)
        .map_err(|_| invalid("recovery receipt is corrupt or unsupported"))?;
    let valid_classification = match receipt.mode.as_deref() {
        None => matches!(
            receipt.binding.classification.as_str(),
            "empty" | "canonical_copy"
        ),
        Some("quarantine") => matches!(
            receipt.binding.classification.as_str(),
            "empty"
                | "canonical_copy"
                | "logically_equivalent"
                | "divergent"
                | "unmatched"
                | "corrupt"
                | "unverifiable"
        ),
        Some("import_missing") => matches!(
            receipt.binding.classification.as_str(),
            "logically_equivalent" | "divergent"
        ),
        Some(_) => false,
    };
    if receipt.schema != SCHEMA
        || receipt.binding.schema != SCHEMA
        || !matches!(receipt.phase.as_str(), "prepared" | "committed")
        || !valid_classification
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
        true,
    )?;
    let expected_links = if exists(&root.join("todo.db"))?
        && identity(&fs::symlink_metadata(root.join("todo.db"))?)? == archive.identity
    {
        2
    } else {
        1
    };
    if archive.links != expected_links {
        return Err(invalid("archive has an unrecognized hard-link alias"));
    }
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
        false,
    )?;
    if current.digest != receipt.binding.canonical_sha256
        || current.identity != receipt.binding.canonical_identity
    {
        return Err(invalid(
            "canonical state changed after review; prepared outcome remains unresolved",
        ));
    }
    Ok(())
}

fn committed_report(root: &Path, receipt: &Receipt) -> Result<Value, DecapodError> {
    check_archive(root, receipt)?;
    let reappeared = exists(&root.join("todo.db"))?;
    let mode = receipt.mode.as_deref().unwrap_or("reconcile");
    Ok(json!({
        "schema": SCHEMA,
        "status": if reappeared { "blocked" } else { "ok" },
        "operation": mode,
        "operation_phase": "committed",
        "already_committed": true,
        "review_sha256": receipt.review_sha256,
        "source_sha256": receipt.binding.source_sha256,
        "archive": ARCHIVE_NAME,
        "imported_task_ids": receipt.imported_task_ids,
        "source_reappeared": reappeared,
        "canonical_current_validation_performed": false,
        "mutations_replayed": 0,
        "identity_assurance": "cooperative_not_authenticated",
        "publication_ready": false,
        "completion_proven": false,
        "next_action": if reappeared {
            "preserve the new input; the historical receipt does not reconcile it"
        } else {
            "historical physical outcome retained; use ordinary current-state validation for task acceptance"
        }
    }))
}

fn import_missing_tasks_into_canonical(
    root: &Path,
    actor: &str,
    review_sha256: &str,
    logical: &LogicalInspection,
) -> Result<Vec<String>, DecapodError> {
    let missing_set: BTreeSet<&str> = logical
        .legacy_only_task_ids
        .iter()
        .map(String::as_str)
        .collect();
    let to_import: Vec<&LegacyTaskRow> = logical
        .legacy_tasks
        .iter()
        .filter(|t| missing_set.contains(t.id.as_str()))
        .collect();
    if to_import.is_empty() {
        return Ok(Vec::new());
    }

    let conn = Connection::open_with_options(
        root.join("decapod.db"),
        AccessMode::ReadWrite,
        Duration::from_secs(5),
    )?;
    conn.execute_batch("BEGIN IMMEDIATE")?;
    let import_res: Result<Vec<String>, DecapodError> = (|| {
        let before = super::task_journal::capture_selected(&conn, &[])?;
        let watermark = super::task_journal::watermark(&conn)?;
        let now = crate::core::time::now_epoch_z();
        let mut imported_ids = Vec::with_capacity(to_import.len());

        for task in &to_import {
            let exists_now: bool = conn.query_row(
                "SELECT EXISTS(SELECT 1 FROM tasks WHERE id = ?1)",
                [task.id.as_str()],
                |row| row.get(0),
            )?;
            if exists_now {
                return Err(DecapodError::ValidationError(format!(
                    "LEGACY_TODO_CONFLICT_BLOCKED: task '{}' appeared in canonical store during import",
                    task.id
                )));
            }
            let hash = if task.hash.is_empty() {
                task.id.chars().take(12).collect::<String>()
            } else {
                task.hash.clone()
            };
            let created_at = if task.created_at.is_empty() {
                now.clone()
            } else {
                task.created_at.clone()
            };
            let updated_at = if task.updated_at.is_empty() {
                now.clone()
            } else {
                task.updated_at.clone()
            };
            let dir_path = if task.dir_path.is_empty() {
                ".".to_string()
            } else {
                task.dir_path.clone()
            };
            let scope = if task.scope.is_empty() {
                "root".to_string()
            } else {
                task.scope.clone()
            };
            let priority = if task.priority.is_empty() {
                "medium".to_string()
            } else {
                task.priority.clone()
            };
            let status = if task.status.is_empty() {
                "open".to_string()
            } else {
                task.status.clone()
            };
            let intent_anchor = if task.intent_anchor.is_empty() {
                format!("legacy-import:{review_sha256}")
            } else {
                task.intent_anchor.clone()
            };

            conn.execute(
                "INSERT INTO tasks (id, hash, title, description, tags, owner, status, created_at, updated_at, dir_path, scope, priority, category, component, assigned_to, intent_anchor, revision)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, 0)",
                crate::core::db::params![
                    task.id,
                    hash,
                    task.title,
                    task.description,
                    task.tags,
                    task.owner,
                    status,
                    created_at,
                    updated_at,
                    dir_path,
                    scope,
                    priority,
                    task.category,
                    task.component,
                    task.assigned_to,
                    intent_anchor
                ],
            )?;
            crate::core::events::append_on_conn(
                &conn,
                crate::core::events::TODO,
                &json!({
                    "ts": now,
                    "event_id": crate::core::ulid::new_ulid(),
                    "event_type": "task.add",
                    "status": "success",
                    "actor": actor,
                    "task_id": task.id,
                    "payload": {
                        "id": task.id,
                        "title": task.title,
                        "status": status,
                        "source": "legacy_todo_import",
                        "review_sha256": review_sha256,
                    }
                }),
            )?;
            imported_ids.push(task.id.clone());
        }

        imported_ids.sort();
        super::task_journal::record_changes(
            &conn,
            &before,
            &imported_ids,
            watermark,
            "todo.legacy.import_missing",
            actor,
        )?;
        Ok(imported_ids)
    })();

    let imported_ids = match import_res {
        Ok(ids) => {
            conn.execute_batch("COMMIT")?;
            ids
        }
        Err(err) => {
            let _ = conn.execute_batch("ROLLBACK");
            return Err(err);
        }
    };

    let budget = super::task_journal::VerifyBudget {
        page_size: 128,
        max_checkpoint_rows: Some(10_000),
        max_task_rows: Some(10_000),
    };
    let verify = super::task_journal::verify_with_budget(&conn, &budget, None)?;
    if verify["status"] != "ok" || verify["complete"] != true || verify["journal_valid"] != true {
        return Err(invalid(
            "canonical task journal failed post-import verification",
        ));
    }
    Ok(imported_ids)
}

/// Review, export, quarantine, or explicitly import missing non-conflicting legacy tasks.
pub(super) fn run_with_options(
    root: &Path,
    actor: &str,
    options: LegacyRecoveryOptions<'_>,
) -> Result<Value, DecapodError> {
    if super::DbBroker::new(root).is_cloud() {
        return Err(invalid(
            "cloud authority never falls back to local recovery",
        ));
    }
    if !valid_actor(actor) {
        return Err(invalid("invalid local actor identity"));
    }
    if options
        .expected_review
        .is_some_and(|value| !is_digest(value))
        || ((options.prepare_only || options.quarantine || options.import_missing)
            && options.expected_review.is_none())
    {
        return Err(invalid(
            "expected review must be the exact SHA-256 from a supported review",
        ));
    }
    if (options.prepare_only as u8 + options.quarantine as u8 + options.import_missing as u8) > 1 {
        return Err(invalid(
            "choose at most one of --prepare-only, --quarantine, or --import-missing",
        ));
    }
    let authority = if options.expected_review.is_some() {
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
        if options
            .expected_review
            .is_some_and(|value| value != receipt.review_sha256)
        {
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
    let mut outcome = review_locked(
        root,
        binding_actor,
        &source_path,
        has_archive && source_path == root.join("todo.db"),
    )?;

    let export_meta = if let Some(export_path) = options.export_path {
        let (path_str, digest) = write_export_bundle(export_path, &outcome)?;
        outcome.report["export_path"] = json!(path_str);
        outcome.report["export_sha256"] = json!(digest);
        outcome.report["export_written"] = json!(true);
        Some((path_str, digest))
    } else {
        None
    };

    let Some(expected) = options.expected_review else {
        if policy::authorize_rbac_exception(root, actor, "todo.legacy.reconcile").is_err() {
            outcome.report["can_reconcile"] = json!(false);
            outcome.report["can_quarantine"] = json!(false);
            outcome.report["can_import_missing"] = json!(false);
            if export_meta.is_none() {
                outcome.report["status"] = json!("blocked");
            }
        }
        if export_meta.is_some() {
            outcome.report["status"] = json!("ok");
            outcome.report["operation"] = json!("export");
        }
        return Ok(outcome.report);
    };

    let Some(binding) = outcome.binding else {
        return Ok(outcome.report);
    };

    if !options.quarantine && !options.import_missing && !outcome.physical_supported {
        return Ok(outcome.report);
    }

    if options.import_missing {
        if !matches!(
            binding.classification.as_str(),
            "divergent" | "logically_equivalent"
        ) {
            return Err(DecapodError::ValidationError(format!(
                "LEGACY_TODO_IMPORT_UNSUPPORTED: classification '{}' cannot be imported; use --quarantine to archive safely",
                binding.classification
            )));
        }
        if !outcome.logical.conflicting_task_ids.is_empty() {
            return Err(DecapodError::ValidationError(format!(
                "LEGACY_TODO_CONFLICT_BLOCKED: {} conflicting task(s) detected ({}); refusing silent overwrite — export with --export or archive with --quarantine",
                outcome.logical.conflicting_task_ids.len(),
                outcome.logical.conflicting_task_ids.join(", ")
            )));
        }
    }

    let digest = binding.digest()?;
    if let Some(receipt) = prior.as_ref()
        && binding != receipt.binding
    {
        return Err(invalid("inputs or policy changed after preparation"));
    }
    if digest != expected || authority.as_ref() != Some(&binding.policy_sha256) {
        return Err(invalid(
            "review is stale or belongs to different inputs or policy",
        ));
    }

    let mode_label = if options.quarantine {
        Some("quarantine".to_string())
    } else if options.import_missing {
        Some("import_missing".to_string())
    } else {
        None
    };

    let mut receipt = prior.unwrap_or(Receipt {
        schema: SCHEMA.to_string(),
        phase: "prepared".to_string(),
        review_sha256: digest,
        binding,
        completed_by: None,
        mode: mode_label.clone(),
        imported_task_ids: Vec::new(),
    });
    save_receipt(root, &receipt)?;
    if options.prepare_only {
        return Ok(json!({
            "schema": SCHEMA,
            "status": "ok",
            "operation_phase": "prepared",
            "review_sha256": receipt.review_sha256,
            "source_preserved": true,
            "mutations_replayed": 0,
            "publication_ready": false,
            "completion_proven": false
        }));
    }
    // Recheck authority immediately before physical archival / import.
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
    open_source(&root.join(ARCHIVE_NAME), true)?.sync_all()?;
    crate::core::atomic::sync_parent_directory(root)?;

    let imported_ids = if options.import_missing {
        import_missing_tasks_into_canonical(root, actor, &receipt.review_sha256, &outcome.logical)?
    } else {
        Vec::new()
    };

    if exists(&root.join("todo.db"))? {
        let temporary = tempfile::tempdir()?;
        let current = snapshot(
            &root.join("todo.db"),
            &temporary.path().join("current.db"),
            MAX_SOURCE_BYTES,
            Instant::now() + Duration::from_secs(30),
            true,
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

    if !options.import_missing {
        check_canonical(root, &receipt)?;
    }
    receipt.phase = "committed".to_string();
    receipt.completed_by = Some(actor.to_string());
    receipt.imported_task_ids = imported_ids;
    save_receipt(root, &receipt)?;
    let mut result = committed_report(root, &receipt)?;
    result["already_committed"] = json!(false);
    result["canonical_bytes_preserved"] = json!(!options.import_missing);
    if let Some((path_str, exp_digest)) = export_meta {
        result["export_path"] = json!(path_str);
        result["export_sha256"] = json!(exp_digest);
    }
    Ok(result)
}
