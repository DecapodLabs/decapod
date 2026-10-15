use crate::core::broker::DbBroker;
use crate::core::completion_evidence;
use crate::core::db::OptionalExtension;
use crate::core::error;
use crate::core::events;
use crate::core::external_action::{self, ExternalCapability};
use crate::core::state_commit;
use crate::core::store::Store;
use crate::core::todo;
use crate::core::validation_epoch::{ValidationEpochMetadata, active_validation_epoch};
use crate::core::workspace;
use crate::plugins::federation;
use clap::{Parser, Subcommand};
use fancy_regex::Regex;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Parser, Debug)]
#[clap(name = "verify", about = "Replay verification proofs and detect drift")]
pub struct VerifyCli {
    /// Output machine-readable JSON.
    #[clap(long, global = true)]
    json: bool,
    /// List stale items only; do not run verification.
    #[clap(long, global = true)]
    stale: bool,
    #[clap(subcommand)]
    command: Option<VerifyCommand>,
}

#[derive(Parser, Debug)]
#[clap(
    name = "diagnose",
    about = "Inspect validation recovery state without mutation"
)]
pub struct DiagnoseCli {
    /// Diagnose one TODO; without this flag, recent proof-bearing done TODOs are shown.
    #[clap(long)]
    pub todo: Option<String>,
    /// Output machine-readable JSON.
    #[clap(long)]
    pub json: bool,
}

#[derive(Subcommand, Debug)]
pub enum VerifyCommand {
    /// Verify a single TODO.
    Todo {
        #[clap(value_name = "ID")]
        id: String,
    },
    /// Regenerate/re-capture the verification baseline for a completed TODO.
    Regen {
        #[clap(value_name = "ID")]
        id: String,
        /// File path(s) to hash for drift detection. Defaults to AGENTS.md.
        #[clap(long = "artifact")]
        artifact: Vec<String>,
    },
    /// Prune/reset verification record for a TODO, returning its status to 'open'.
    Prune {
        #[clap(value_name = "ID")]
        id: String,
    },
    /// Recover a batch of done TODOs from the aggregate-proof deadlock.
    Recover,
    /// Generate or independently verify a completion evidence record.
    Completion {
        #[clap(value_name = "ID")]
        id: String,
        /// Generate the record instead of verifying the existing record.
        #[clap(long)]
        write: bool,
        /// Verify a specific record path instead of the default repository path.
        #[clap(long)]
        path: Option<PathBuf>,
        /// Export the current local record as portable evidence to this path.
        #[clap(long)]
        export: Option<PathBuf>,
        /// Import portable evidence from this path into receiving-repository custody.
        #[clap(long)]
        import: Option<PathBuf>,
    },
}

#[derive(Debug, Clone)]
struct VerifyTarget {
    todo_id: String,
    revision: i64,
    status: String,
    proof_plan: Option<String>,
    artifacts: Option<String>,
    last_verified_at: Option<String>,
    last_verified_status: Option<String>,
    verification_policy_days: i64,
}

#[derive(Debug, Clone)]
struct ValidationSnapshot {
    passed: bool,
    output_hash: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct VerificationArtifacts {
    completed_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    task_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    task_revision: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    evaluator_epoch: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    validation_epoch: Option<ValidationEpochMetadata>,
    proof_plan_results: Vec<ProofPlanResult>,
    file_artifacts: Vec<FileArtifact>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct ProofPlanResult {
    proof_gate: String,
    status: String,
    command: String,
    output_hash: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    evaluator_epoch: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    validation_epoch: Option<ValidationEpochMetadata>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct FileArtifact {
    path: String,
    hash: String,
    size: u64,
    mtime: Option<u64>,
}

#[derive(Debug, Serialize)]
struct VerifySummary {
    total: usize,
    passed: usize,
    failed: usize,
    unknown: usize,
    stale: usize,
}

#[derive(Debug, Serialize)]
struct ProofCheckResult {
    gate: String,
    status: String,
    expected_output_hash: Option<String>,
    actual_output_hash: Option<String>,
    reason: Option<String>,
}

#[derive(Debug, Serialize)]
struct ArtifactCheckResult {
    path: String,
    status: String,
    expected_hash: Option<String>,
    actual_hash: Option<String>,
    reason: Option<String>,
}

#[derive(Debug, Serialize)]
struct VerifyTodoResult {
    todo_id: String,
    status: String,
    proofs: Vec<ProofCheckResult>,
    artifacts: Vec<ArtifactCheckResult>,
    notes: Vec<String>,
}

#[derive(Debug, Serialize)]
struct VerifyReport {
    verified_at: String,
    summary: VerifySummary,
    results: Vec<VerifyTodoResult>,
}

#[derive(Debug, Serialize)]
struct StaleItem {
    todo_id: String,
    last_verified_at: Option<String>,
    verification_policy_days: i64,
}

fn now_iso() -> String {
    crate::core::time::now_epoch_z()
}

fn epoch_secs(ts: &str) -> Option<i64> {
    ts.trim_end_matches('Z').parse::<i64>().ok()
}

fn validation_proof_reason(validate_ok: bool, hashes_match: bool, todo_id: &str) -> String {
    if !validate_ok && hashes_match {
        format!(
            "decapod validate did not pass; output hash is unchanged from the baseline, so verification remains blocked. Next: fix the reported validation gate, then run `decapod qa verify todo {todo_id}`"
        )
    } else if !validate_ok {
        format!(
            "decapod validate did not pass. Next: fix the reported validation gate, then run `decapod qa verify todo {todo_id}`"
        )
    } else {
        format!(
            "validate output hash changed. Next: review the drift, then recapture with `decapod qa verify regen {todo_id}` if the change is intentional"
        )
    }
}

fn validation_epoch_changed_reason(
    captured_epoch: &str,
    active_epoch: &str,
    todo_id: &str,
) -> String {
    format!(
        "validation epoch changed: captured {captured_epoch} but active {active_epoch}. To recover from an aggregate-proof deadlock, run `decapod qa verify prune {todo_id}`, obtain a passing `decapod validate` in the governed container workspace, run `decapod todo claim --id {todo_id}`, then run `decapod todo done --id {todo_id} --validated` there."
    )
}

fn normalize_validate_output(raw: &str) -> String {
    let ansi = Regex::new(r"\x1B\[[0-9;]*[A-Za-z]").expect("valid ANSI regex");
    let elapsed_re = Regex::new(r" elapsed=\S+").expect("valid elapsed regex");
    let stripped = ansi.replace_all(raw, "");
    stripped
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(|line| {
            if line.contains("decapod_validate_user_") || line.contains("decapod_validate_repo_") {
                "<tmp_validate_path>".to_string()
            } else {
                // Strip non-deterministic elapsed timing from summary line
                elapsed_re.replace_all(line, "").to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn normalize_json_value(value: &serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Object(map) => {
            let mut normalized = serde_json::Map::new();
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            for key in keys {
                if key == "elapsed_ms" {
                    normalized.insert(
                        key.clone(),
                        serde_json::Value::Number(serde_json::Number::from(0)),
                    );
                } else if key == "self_heal" {
                    normalized.insert(key.clone(), serde_json::Value::Array(Vec::new()));
                } else if key == "failures" || key == "warnings" {
                    if let Some(arr) = map[key].as_array() {
                        let mut sorted_arr = arr.clone();
                        sorted_arr.sort_by_key(|a| a.to_string());
                        normalized.insert(
                            key.clone(),
                            serde_json::Value::Array(
                                sorted_arr.iter().map(normalize_json_value).collect(),
                            ),
                        );
                    } else {
                        normalized.insert(key.clone(), normalize_json_value(&map[key]));
                    }
                } else if key == "gate_timings" {
                    if let Some(arr) = map[key].as_array() {
                        let mut sorted_arr = arr.clone();
                        sorted_arr.sort_by(|a, b| {
                            let a_name = a.get("name").and_then(|v| v.as_str()).unwrap_or("");
                            let b_name = b.get("name").and_then(|v| v.as_str()).unwrap_or("");
                            a_name.cmp(b_name)
                        });
                        normalized.insert(
                            key.clone(),
                            serde_json::Value::Array(
                                sorted_arr.iter().map(normalize_json_value).collect(),
                            ),
                        );
                    } else {
                        normalized.insert(key.clone(), normalize_json_value(&map[key]));
                    }
                } else {
                    normalized.insert(key.clone(), normalize_json_value(&map[key]));
                }
            }
            serde_json::Value::Object(normalized)
        }
        serde_json::Value::Array(items) => {
            serde_json::Value::Array(items.iter().map(normalize_json_value).collect())
        }
        _ => value.clone(),
    }
}

fn sha256_hex(input: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(input);
    format!("sha256:{:x}", hasher.finalize())
}

fn hash_file(path: &Path) -> Result<(String, u64, Option<u64>), error::DecapodError> {
    let bytes = fs::read(path)?;
    let meta = fs::metadata(path)?;
    let mtime = meta
        .modified()
        .ok()
        .and_then(|m| m.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs());
    Ok((sha256_hex(&bytes), meta.len(), mtime))
}

fn run_validate_and_hash(
    store_root: &Path,
    repo_root: &Path,
    todo_id: Option<&str>,
) -> Result<(bool, String), error::DecapodError> {
    let _verifying = todo_id.map(|id| {
        let existing = std::env::var("DECAPOD_VERIFYING_TODO").unwrap_or_default();
        let ids = if existing.is_empty() {
            id.to_string()
        } else if existing.split(',').any(|value| value.trim() == id) {
            existing
        } else {
            format!("{existing},{id}")
        };
        VerifyingTodosGuard::enter(&ids)
    });
    let exe = std::env::current_exe()?;
    let exe_str = exe.to_string_lossy().to_string();
    let output = external_action::execute(
        store_root,
        ExternalCapability::VerificationExec,
        "verify.validate_passes",
        &exe_str,
        &["validate", "--format", "json"],
        repo_root,
    )?;
    // A successful validation persists its trajectory and receipt after the
    // worker report is produced. Run the same read-only proof once more before
    // hashing so the baseline represents the settled post-validation state;
    // otherwise the first capture hashes two artifact gates as skips and every
    // replay observes them as passes.
    let output = if output.status.success() {
        external_action::execute(
            store_root,
            ExternalCapability::VerificationExec,
            "verify.validate_passes",
            &exe_str,
            &["validate", "--format", "json"],
            repo_root,
        )?
    } else {
        output
    };
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(&stdout) {
        let normalized = normalize_json_value(&value);
        let canonical = serde_json::to_string(&normalized).unwrap();
        return Ok((output.status.success(), sha256_hex(canonical.as_bytes())));
    }

    let mut merged = stdout;
    if !output.stderr.is_empty() {
        merged.push('\n');
        merged.push_str(&String::from_utf8_lossy(&output.stderr));
    }

    let normalized = normalize_validate_output(&merged);
    Ok((output.status.success(), sha256_hex(normalized.as_bytes())))
}

fn append_event(store: &Store, value: &serde_json::Value) -> Result<(), error::DecapodError> {
    events::append(&store.root, events::VERIFICATION, value)?;
    Ok(())
}

fn mirror_verification_to_federation(
    store: &Store,
    todo_id: &str,
    title: &str,
    body: &str,
    tags: &str,
) {
    let source = format!("event:{todo_id}");
    let anchor = federation::find_node_by_source(store, &source)
        .ok()
        .flatten();
    if let Ok(node) = federation::add_node(
        store,
        title,
        "decision",
        "notable",
        "agent_inferred",
        body,
        &source,
        tags,
        "repo",
        None,
        "decapod",
    ) && let Some(intent_or_commitment) = anchor
    {
        let _ = federation::add_edge(store, &intent_or_commitment, &node.id, "depends_on");
    }
    let _ = federation::refresh_derived_files(store);
}

fn load_targets(
    store: &Store,
    single_id: Option<&str>,
) -> Result<Vec<VerifyTarget>, error::DecapodError> {
    todo::initialize_todo_db(&store.root)?;
    let broker = DbBroker::new(&store.root);
    let db_path = todo::todo_db_path(&store.root);

    broker.with_conn(&db_path, "decapod", None, "verify.targets", |conn| {
        let mut out = Vec::new();
        if let Some(id) = single_id {
            let mut stmt = conn.prepare(
                "SELECT t.id, t.status, v.proof_plan, v.verification_artifacts, v.last_verified_at, v.last_verified_status, COALESCE(v.verification_policy_days, 90), t.revision\n                 FROM tasks t\n                 LEFT JOIN task_verification v ON v.todo_id = t.id\n                 WHERE t.id = ?1",
            )?;
            let rows = stmt.query_map(crate::core::db::params![id], |row| {
                Ok(VerifyTarget {
                    todo_id: row.get(0)?,
                    status: row.get(1)?,
                    proof_plan: row.get(2)?,
                    artifacts: row.get(3)?,
                    last_verified_at: row.get(4)?,
                    last_verified_status: row.get(5)?,
                    verification_policy_days: row.get(6)?,
                    revision: row.get(7)?,
                })
            })?;

            for row in rows {
                out.push(row?);
            }
        } else {
            let mut stmt = conn.prepare(
                "SELECT t.id, t.status, v.proof_plan, v.verification_artifacts, v.last_verified_at, v.last_verified_status, COALESCE(v.verification_policy_days, 90), t.revision\n                 FROM tasks t\n                 LEFT JOIN task_verification v ON v.todo_id = t.id\n                 WHERE t.status = 'done'\n                 ORDER BY t.updated_at DESC",
            )?;
            let rows = stmt.query_map([], |row| {
                Ok(VerifyTarget {
                    todo_id: row.get(0)?,
                    status: row.get(1)?,
                    proof_plan: row.get(2)?,
                    artifacts: row.get(3)?,
                    last_verified_at: row.get(4)?,
                    last_verified_status: row.get(5)?,
                    verification_policy_days: row.get(6)?,
                    revision: row.get(7)?,
                })
            })?;

            for row in rows {
                let target = row?;
                let artifacts = target.artifacts.as_deref().and_then(|raw| serde_json::from_str::<serde_json::Value>(raw).ok());
                let effective = todo::effective_verification_status(target.last_verified_status.as_deref(), artifacts.as_ref(), &target.todo_id, target.revision);
                if !effective.is_some_and(|value| matches!(value.to_ascii_lowercase().as_str(), "pass" | "passed" | "verified")) {
                    out.push(target);
                }
            }
        }
        Ok(out)
    })
}

/// Check the exact task and verification baseline that the caller inspected.
/// The caller holds the write transaction through the result/event mutation.
fn ensure_target_unchanged(
    conn: &crate::core::db::Connection,
    target: &VerifyTarget,
) -> Result<(), error::DecapodError> {
    let current = conn.query_row(
        "SELECT t.status, t.revision, v.proof_plan, v.verification_artifacts, v.last_verified_at, v.last_verified_status, COALESCE(v.verification_policy_days, 90)
         FROM tasks t LEFT JOIN task_verification v ON v.todo_id = t.id WHERE t.id = ?1",
        [&target.todo_id],
        |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?, row.get::<_, Option<String>>(2)?, row.get::<_, Option<String>>(3)?, row.get::<_, Option<String>>(4)?, row.get::<_, Option<String>>(5)?, row.get::<_, i64>(6)?)),
    ).optional()?;
    let expected = (
        target.status.clone(),
        target.revision,
        target.proof_plan.clone(),
        target.artifacts.clone(),
        target.last_verified_at.clone(),
        target.last_verified_status.clone(),
        target.verification_policy_days,
    );
    if current.as_ref() != Some(&expected) {
        return Err(error::DecapodError::ValidationError(format!(
            "STALE_TASK_VERIFICATION: task {} or its proof baseline changed during verification; inspect the current task and rerun verification or recapture with `decapod qa verify regen {}`",
            target.todo_id, target.todo_id,
        )));
    }
    Ok(())
}

fn append_verification_event_on_connection(
    conn: &crate::core::db::Connection,
    todo_id: &str,
    event_type: &str,
    ts: &str,
    payload: serde_json::Value,
) -> Result<(), error::DecapodError> {
    events::append_on_conn(
        conn,
        events::TODO,
        &serde_json::json!({
            "ts": ts,
            "event_id": crate::core::ulid::new_ulid(),
            "event_type": event_type,
            "status": "success",
            "task_id": todo_id,
            "actor": "decapod",
            "payload": payload,
        }),
    )?;
    Ok(())
}

fn persist_result_on_connection(
    conn: &crate::core::db::Connection,
    target: &VerifyTarget,
    status: &str,
    notes: &str,
    ts: &str,
) -> Result<(), error::DecapodError> {
    if conn.is_autocommit() {
        return Err(error::DecapodError::ValidationError(
            "Verification result requires a transaction".into(),
        ));
    }
    ensure_target_unchanged(conn, target)?;
    let artifacts = target
        .artifacts
        .as_deref()
        .and_then(|raw| serde_json::from_str::<serde_json::Value>(raw).ok());
    if status.eq_ignore_ascii_case("pass")
        && (target.status != "done"
            || !todo::verification_binding_matches(
                artifacts.as_ref(),
                &target.todo_id,
                target.revision,
            ))
    {
        return Err(error::DecapodError::ValidationError(
                "STALE_TASK_VERIFICATION: passing proof must bind the current completed task; recapture verification".into(),
            ));
    }
    let proof_plan = target.proof_plan.as_deref().unwrap_or("[]");
    conn.execute(
            "INSERT INTO task_verification(todo_id, proof_plan, verification_artifacts, last_verified_at, last_verified_status, last_verified_notes, verification_policy_days, updated_at)
             VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?4)
             ON CONFLICT(todo_id) DO UPDATE SET
               proof_plan=excluded.proof_plan,
               verification_artifacts=excluded.verification_artifacts,
               last_verified_at=excluded.last_verified_at,
               last_verified_status=excluded.last_verified_status,
               last_verified_notes=excluded.last_verified_notes,
               verification_policy_days=excluded.verification_policy_days,
               updated_at=excluded.updated_at",
            crate::core::db::params![target.todo_id, proof_plan, target.artifacts, ts, status, notes, target.verification_policy_days],
        )?;
    append_verification_event_on_connection(
        conn,
        &target.todo_id,
        "task.verify.result",
        ts,
        serde_json::json!({
            "proof_plan": serde_json::from_str::<serde_json::Value>(proof_plan).unwrap_or_else(|_| serde_json::json!([])),
            "verification_artifacts": artifacts,
            "last_verified_status": status,
            "last_verified_notes": notes,
            "verification_policy_days": target.verification_policy_days,
        }),
    )
}

fn persist_result(
    store: &Store,
    target: &VerifyTarget,
    status: &str,
    notes: &str,
) -> Result<VerifyTarget, error::DecapodError> {
    let ts = now_iso();
    let broker = DbBroker::new(&store.root);
    let db_path = todo::todo_db_path(&store.root);
    todo::with_task_journal(
        &broker,
        &db_path,
        "decapod",
        None,
        "verify.persist",
        todo::TaskJournalSelection::Ids(vec![target.todo_id.clone()]),
        |conn| persist_result_on_connection(conn, target, status, notes, &ts),
    )?;
    mirror_verification_to_federation(
        store,
        &target.todo_id,
        &format!("Verification Result: {}", target.todo_id),
        &format!("Verification status={status} notes={notes}"),
        "proof,verification,result",
    );
    let mut written = target.clone();
    written.proof_plan = Some(
        target
            .proof_plan
            .clone()
            .unwrap_or_else(|| "[]".to_string()),
    );
    written.last_verified_at = Some(ts);
    written.last_verified_status = Some(status.to_string());
    Ok(written)
}

fn is_stale(target: &VerifyTarget, now_secs: i64) -> bool {
    let artifacts = target
        .artifacts
        .as_deref()
        .and_then(|raw| serde_json::from_str::<serde_json::Value>(raw).ok());
    if !todo::verification_binding_matches(artifacts.as_ref(), &target.todo_id, target.revision) {
        return true;
    }
    match target.last_verified_at.as_deref().and_then(epoch_secs) {
        None => true,
        Some(last) => now_secs.saturating_sub(last) > target.verification_policy_days * 86_400,
    }
}

fn resolve_artifact_path(repo_root: &Path, stored: &str) -> PathBuf {
    resolve_artifact_path_for_todo(repo_root, None, stored)
}

/// Resolve `--artifact` against the workspace that owns the claim.
///
/// Local-clone worktrees can hold files that do not exist in the parent
/// checkout. Prefer a path that exists in `repo_root`, then the newest
/// `.decapod/workspaces/*` directory for `todo_id` (GitHub #1259).
fn resolve_artifact_path_for_todo(
    repo_root: &Path,
    todo_id: Option<&str>,
    stored: &str,
) -> PathBuf {
    let path = Path::new(stored);
    if path.is_absolute() {
        return path.to_path_buf();
    }
    let local = repo_root.join(path);
    if local.exists() {
        return local;
    }
    if let Some(todo_id) = todo_id {
        let host =
            workspace::get_main_repo_root(repo_root).unwrap_or_else(|_| repo_root.to_path_buf());
        if let Some(workspace_path) = workspace::find_workspace_for_task(&host, todo_id) {
            let candidate = workspace_path.join(path);
            if candidate.exists() {
                return candidate;
            }
        }
    }
    local
}

fn verify_target(
    target: &VerifyTarget,
    store_root: &Path,
    repo_root: &Path,
    validation_snapshot: Option<&ValidationSnapshot>,
) -> Result<VerifyTodoResult, error::DecapodError> {
    let mut result = VerifyTodoResult {
        todo_id: target.todo_id.clone(),
        status: "pass".to_string(),
        proofs: Vec::new(),
        artifacts: Vec::new(),
        notes: Vec::new(),
    };

    if target.status != "done" {
        result.status = "unknown".to_string();
        result
            .notes
            .push("TODO is not in done state; only done tasks are verifiable".to_string());
        return Ok(result);
    }

    let plan_raw = match target.proof_plan.as_deref() {
        Some(v) if !v.trim().is_empty() => v,
        _ => {
            result.status = "unknown".to_string();
            result.notes.push(
                "Missing verification metadata. Remediation: mark task done with `--validated` or capture verification artifacts for this TODO.".to_string(),
            );
            return Ok(result);
        }
    };

    let proof_plan: Vec<String> = match serde_json::from_str(plan_raw) {
        Ok(v) => v,
        Err(_) => {
            result.status = "unknown".to_string();
            result.notes.push(
                "Invalid proof_plan format. Remediation: recapture verification artifacts for this TODO.".to_string(),
            );
            return Ok(result);
        }
    };

    let supported_proofs = ["validate_passes", "state_commit"];
    if proof_plan
        .iter()
        .any(|p| !supported_proofs.iter().any(|sp| p == *sp))
    {
        result.status = "unknown".to_string();
        result.notes.push(
            "Unsupported proof_plan. Supported: validate_passes, state_commit. Remediation: set proof_plan to [\"validate_passes\"] or [\"state_commit\"].".to_string(),
        );
        return Ok(result);
    }

    // Check validate_passes if in plan
    let validate_check_needed = proof_plan.iter().any(|p| p == "validate_passes");

    let artifacts_raw = match target.artifacts.as_deref() {
        Some(v) if !v.trim().is_empty() => v,
        _ => {
            result.status = "unknown".to_string();
            result.notes.push(
                "Missing verification_artifacts. Remediation: capture verification artifacts for this TODO.".to_string(),
            );
            return Ok(result);
        }
    };

    let artifacts: VerificationArtifacts = match serde_json::from_str(artifacts_raw) {
        Ok(v) => v,
        Err(_) => {
            result.status = "unknown".to_string();
            result.notes.push(
                "Malformed verification_artifacts JSON. Remediation: recapture verification artifacts for this TODO.".to_string(),
            );
            return Ok(result);
        }
    };

    if artifacts.task_id.as_deref() != Some(target.todo_id.as_str())
        || artifacts.task_revision != Some(target.revision)
        || target.revision < 0
    {
        result.status = "fail".to_string();
        result.notes.push(format!(
            "STALE_TASK_VERIFICATION: proof does not bind current task {} revision {}. Recapture with `decapod qa verify regen {}`; historical proof is not reassigned to changed intent.",
            target.todo_id, target.revision, target.todo_id,
        ));
        return Ok(result);
    }

    let expected_proof = artifacts
        .proof_plan_results
        .iter()
        .find(|p| p.proof_gate == "validate_passes");
    let expected_hash = expected_proof.map(|p| p.output_hash.clone());

    if expected_hash.is_none() {
        result.status = "unknown".to_string();
        result.notes.push(
            "Missing baseline validate_passes output hash. Remediation: capture verification artifacts for this TODO.".to_string(),
        );
        return Ok(result);
    }

    // Only check validate_passes if it's in the proof plan
    if validate_check_needed {
        let active_epoch = active_validation_epoch(repo_root)?;
        match artifacts.validation_epoch.as_ref() {
            Some(captured_epoch) if captured_epoch.epoch_id == active_epoch.epoch_id => {}
            Some(captured_epoch) => {
                result.status = "fail".to_string();
                result.proofs.push(ProofCheckResult {
                    gate: "validate_passes".to_string(),
                    status: "fail".to_string(),
                    expected_output_hash: expected_hash.clone(),
                    actual_output_hash: None,
                    reason: Some(validation_epoch_changed_reason(
                        &captured_epoch.epoch_id,
                        &active_epoch.epoch_id,
                        &target.todo_id,
                    )),
                });
                return Ok(result);
            }
            None => {
                result.status = "unknown".to_string();
                result.notes.push(
                    "Missing validation_epoch provenance. Remediation: recapture verification artifacts so proof evidence is tied to the active validation epoch.".to_string(),
                );
                return Ok(result);
            }
        }
        let (validate_ok, actual_hash) = if let Some(snapshot) = validation_snapshot {
            (snapshot.passed, snapshot.output_hash.clone())
        } else {
            run_validate_and_hash(store_root, repo_root, Some(&target.todo_id))?
        };
        let expected = expected_hash.unwrap_or_default();
        let hashes_match = actual_hash == expected;

        if !validate_ok || !hashes_match {
            result.status = "fail".to_string();
            result.proofs.push(ProofCheckResult {
                gate: "validate_passes".to_string(),
                status: "fail".to_string(),
                expected_output_hash: Some(expected),
                actual_output_hash: Some(actual_hash),
                reason: Some(validation_proof_reason(
                    validate_ok,
                    hashes_match,
                    &target.todo_id,
                )),
            });
        } else {
            result.proofs.push(ProofCheckResult {
                gate: "validate_passes".to_string(),
                status: "pass".to_string(),
                expected_output_hash: Some(expected),
                actual_output_hash: Some(actual_hash),
                reason: None,
            });
        }
    }

    // Check state_commit if in plan
    if proof_plan.iter().any(|p| p == "state_commit") {
        let state_commit_proof = artifacts
            .proof_plan_results
            .iter()
            .find(|p| p.proof_gate == "state_commit");

        let expected_root = state_commit_proof.map(|p| p.output_hash.clone());

        let expected = match expected_root {
            Some(exp) => exp,
            None => {
                result.status = "fail".to_string();
                result.notes.push(
                    "Missing baseline state_commit output hash. Remediation: run `decapod state-commit prove --base <sha> --head <sha> --output scope_record.cbor` and capture as proof artifact.".to_string(),
                );
                String::new() // Skip the rest of state_commit verification
            }
        };

        // Only verify if we have an expected_root
        if !expected.is_empty() {
            // Full state_commit verification using canonical prove logic:
            // 1. scope_record.cbor must exist
            // 2. Recompute Merkle root from git objects at current HEAD
            // 3. Compare recomputed root to expected_root
            let scope_record_path = repo_root.join("scope_record.cbor");
            if !scope_record_path.exists() {
                result.status = "fail".to_string();
                result.proofs.push(ProofCheckResult {
                gate: "state_commit".to_string(),
                status: "fail".to_string(),
                expected_output_hash: Some(expected.clone()),
                actual_output_hash: None,
                reason: Some("scope_record.cbor not found in repo root. Run `decapod state-commit prove` first.".to_string()),
            });
            } else {
                // Get current HEAD
                let head_output = std::process::Command::new("git")
                    .args(["rev-parse", "HEAD"])
                    .current_dir(repo_root)
                    .output();

                // Use library function to recompute root from git objects (hermetic, no subprocess)
                // This verifies the full contract: git objects -> entries -> Merkle root
                let current_head = match head_output {
                    Ok(o) if o.status.success() => {
                        String::from_utf8_lossy(&o.stdout).trim().to_string()
                    }
                    _ => String::new(),
                };

                let recomputed_root = if !current_head.is_empty() {
                    // Get base_sha (previous commit)
                    let base_output = std::process::Command::new("git")
                        .args(["rev-parse", "HEAD~1"])
                        .current_dir(repo_root)
                        .output();

                    let base_sha = match base_output {
                        Ok(o) if o.status.success() => {
                            String::from_utf8_lossy(&o.stdout).trim().to_string()
                        }
                        _ => String::new(),
                    };

                    if !base_sha.is_empty() {
                        let input = state_commit::StateCommitInput {
                            base_sha,
                            head_sha: current_head.clone(),
                            ignore_policy_hash: "da39a3ee5e6b4b0d3255bfef95601890afd80709"
                                .to_string(),
                        };
                        match state_commit::prove(&input, repo_root) {
                            Ok(result) => Some(result.state_commit_root),
                            Err(_) => None,
                        }
                    } else {
                        None
                    }
                } else {
                    None
                };

                match recomputed_root {
                    Some(root) if root == expected => {
                        // Verify HEAD binding - check scope_record contains current HEAD
                        let scope_bytes = std::fs::read(&scope_record_path).unwrap_or_default();
                        let head_in_record = !current_head.is_empty()
                            && scope_bytes
                                .windows(current_head.len())
                                .any(|w| w == current_head.as_bytes());

                        if !head_in_record {
                            result.status = "fail".to_string();
                            result.proofs.push(ProofCheckResult {
                                gate: "state_commit".to_string(),
                                status: "fail".to_string(),
                                expected_output_hash: Some(expected.clone()),
                                actual_output_hash: Some(root.clone()),
                                reason: Some(format!("STATE_COMMIT head_sha mismatch. Current HEAD: {current_head} not in scope_record. Run `decapod state-commit prove` to regenerate.")),
                            });
                        } else {
                            result.proofs.push(ProofCheckResult {
                                gate: "state_commit".to_string(),
                                status: "pass".to_string(),
                                expected_output_hash: Some(expected.clone()),
                                actual_output_hash: Some(root),
                                reason: Some("STATE_COMMIT verified: root recomputed from git objects matches expected, bound to current HEAD".to_string()),
                            });
                        }
                    }
                    Some(root) => {
                        result.status = "fail".to_string();
                        result.proofs.push(ProofCheckResult {
                            gate: "state_commit".to_string(),
                            status: "fail".to_string(),
                            expected_output_hash: Some(expected.clone()),
                            actual_output_hash: Some(root.clone()),
                            reason: Some(format!("STATE_COMMIT root mismatch. Expected: {expected}, Recomputed: {root}. Files changed since scope recorded. Run `decapod state-commit prove` to regenerate.")),
                        });
                    }
                    None => {
                        result.status = "fail".to_string();
                        result.proofs.push(ProofCheckResult {
                            gate: "state_commit".to_string(),
                            status: "fail".to_string(),
                            expected_output_hash: Some(expected.clone()),
                            actual_output_hash: None,
                            reason: Some("Failed to recompute STATE_COMMIT root. Verify decapod binary is available.".to_string()),
                        });
                    }
                }
            }
        }
    }

    if artifacts.file_artifacts.is_empty() {
        result.status = "unknown".to_string();
        result.notes.push(
            "Missing file_artifacts. Remediation: capture file hash artifacts (for MVP include AGENTS.md)."
                .to_string(),
        );
        return Ok(result);
    }

    for expected in artifacts.file_artifacts {
        let disk_path = resolve_artifact_path(repo_root, &expected.path);
        if !disk_path.exists() {
            result.status = "fail".to_string();
            result.artifacts.push(ArtifactCheckResult {
                path: expected.path,
                status: "fail".to_string(),
                expected_hash: Some(expected.hash),
                actual_hash: Some("<missing>".to_string()),
                reason: Some("artifact missing".to_string()),
            });
            continue;
        }

        let (actual_hash, _, _) = hash_file(&disk_path)?;
        if actual_hash != expected.hash {
            result.status = "fail".to_string();
            result.artifacts.push(ArtifactCheckResult {
                path: expected.path,
                status: "fail".to_string(),
                expected_hash: Some(expected.hash),
                actual_hash: Some(actual_hash),
                reason: Some("hash mismatch".to_string()),
            });
        } else {
            result.artifacts.push(ArtifactCheckResult {
                path: expected.path,
                status: "pass".to_string(),
                expected_hash: Some(expected.hash),
                actual_hash: Some(actual_hash),
                reason: None,
            });
        }
    }

    Ok(result)
}

/// A passing validation bound to one task revision and the material it checked.
/// Fields and construction stay private: a caller's `--validated` assertion is
/// never sufficient to manufacture completion evidence.
#[derive(Debug)]
pub(crate) struct PreparedTodoVerification {
    todo_id: String,
    expected_revision: i64,
    expected_status: String,
    repo_root: PathBuf,
    artifact_paths: Vec<PathBuf>,
    artifacts: VerificationArtifacts,
}

impl PreparedTodoVerification {
    pub(crate) fn expected_revision(&self) -> i64 {
        self.expected_revision
    }

    pub(crate) fn expected_status(&self) -> &str {
        &self.expected_status
    }

    pub(crate) fn matches_task(&self, id: &str, revision: i64, status: &str) -> bool {
        self.todo_id == id && self.expected_revision == revision && self.expected_status == status
    }

    fn check_material(&self) -> Result<(), error::DecapodError> {
        let epoch = active_validation_epoch(&self.repo_root)?;
        if self.artifacts.validation_epoch.as_ref() != Some(&epoch) {
            return Err(error::DecapodError::ValidationError(
                "Validation material changed after verification; rerun completion validation"
                    .into(),
            ));
        }
        for (path, artifact) in self
            .artifact_paths
            .iter()
            .zip(&self.artifacts.file_artifacts)
        {
            let (hash, size, _) = hash_file(path)?;
            if hash != artifact.hash || size != artifact.size {
                return Err(error::DecapodError::ValidationError(format!(
                    "Verification artifact changed after validation: {}",
                    path.display()
                )));
            }
        }
        Ok(())
    }

    /// Persist only inside the caller's existing transaction, after its CAS and
    /// completion gates and status update. Any error must abort that transaction.
    pub(crate) fn commit_on_connection(
        &self,
        conn: &crate::core::db::Connection,
    ) -> Result<(), error::DecapodError> {
        self.persist_on_connection(conn, self.expected_revision + 1)
    }

    fn commit_recapture_on_connection(
        &self,
        conn: &crate::core::db::Connection,
    ) -> Result<(), error::DecapodError> {
        if self.expected_status != "done" {
            return Err(error::DecapodError::ValidationError(
                "Only completed tasks can recapture a verification baseline".into(),
            ));
        }
        self.persist_on_connection(conn, self.expected_revision)
    }

    fn persist_on_connection(
        &self,
        conn: &crate::core::db::Connection,
        expected_revision: i64,
    ) -> Result<(), error::DecapodError> {
        if conn.is_autocommit() {
            return Err(error::DecapodError::ValidationError(
                "Verified completion requires an existing transaction".into(),
            ));
        }
        let (status, revision): (String, i64) = conn.query_row(
            "SELECT status, revision FROM tasks WHERE id = ?1",
            [&self.todo_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        if status != "done" || revision != expected_revision {
            return Err(error::DecapodError::ValidationError(
                "Task changed after verification; rerun completion validation".into(),
            ));
        }
        self.check_material()?;
        let ts = &self.artifacts.completed_at;
        let mut artifacts = self.artifacts.clone();
        artifacts.task_id = Some(self.todo_id.clone());
        artifacts.task_revision = Some(expected_revision);
        let artifacts_json = serde_json::to_string(&artifacts)
            .map_err(|err| error::DecapodError::ValidationError(err.to_string()))?;
        conn.execute(
            "INSERT INTO task_verification(todo_id, proof_plan, verification_artifacts, last_verified_at, last_verified_status, last_verified_notes, verification_policy_days, updated_at)
             VALUES(?1, '[\"validate_passes\"]', ?2, ?3, 'pass', 'baseline captured', 90, ?3)
             ON CONFLICT(todo_id) DO UPDATE SET
               proof_plan=excluded.proof_plan,
               verification_artifacts=excluded.verification_artifacts,
               last_verified_at=excluded.last_verified_at,
               last_verified_status=excluded.last_verified_status,
               last_verified_notes=excluded.last_verified_notes,
               verification_policy_days=excluded.verification_policy_days,
               updated_at=excluded.updated_at",
            crate::core::db::params![self.todo_id, artifacts_json, ts],
        )?;
        events::append_on_conn(
            conn,
            events::TODO,
            &serde_json::json!({
                "ts": ts,
                "event_id": crate::core::ulid::new_ulid(),
                "event_type": "task.verify.capture",
                "status": "success",
                "task_id": self.todo_id,
                "actor": "decapod",
                "payload": {
                    "proof_plan": ["validate_passes"],
                    "verification_artifacts": artifacts,
                    "last_verified_status": "pass",
                    "last_verified_notes": "baseline captured",
                    "verification_policy_days": 90
                }
            }),
        )?;
        Ok(())
    }
}

pub(crate) fn prepare_verified_completion(
    store: &Store,
    repo_root: &Path,
    todo_id: &str,
    artifact_paths: Vec<String>,
) -> Result<PreparedTodoVerification, error::DecapodError> {
    prepare_verified_completion_with(store, repo_root, todo_id, artifact_paths, || {
        run_validate_and_hash(&store.root, repo_root, Some(todo_id))
    })
}

// Keep the outcome collector private. Tests can exercise failures without
// executing a subprocess; production has only the real validator entrypoint.
fn prepare_verified_completion_with(
    store: &Store,
    repo_root: &Path,
    todo_id: &str,
    artifact_paths: Vec<String>,
    collect: impl FnOnce() -> Result<(bool, String), error::DecapodError>,
) -> Result<PreparedTodoVerification, error::DecapodError> {
    todo::initialize_todo_db(&store.root)?;
    let task = todo::get_task(&store.root, todo_id)?
        .ok_or_else(|| error::DecapodError::NotFound(format!("TODO not found: {todo_id}")))?;
    let result = (|| {
        let paths = if artifact_paths.is_empty() {
            vec!["AGENTS.md".to_string()]
        } else {
            artifact_paths
        };
        let validation_epoch = active_validation_epoch(repo_root)?;
        let mut file_artifacts = Vec::new();
        let mut resolved_paths = Vec::new();
        for path in paths {
            let disk_path = resolve_artifact_path_for_todo(repo_root, Some(todo_id), &path);
            if !disk_path.exists() {
                return Err(error::DecapodError::NotFound(format!(
                    "Verification artifact file not found: {}. `--artifact` expects a file path relative to the workspace worktree that owns the claim, for example `--artifact README.md`. To attach notes, use `decapod todo comment --id {}`.",
                    disk_path.display(),
                    todo_id
                )));
            }
            let (hash, size, mtime) = hash_file(&disk_path)?;
            resolved_paths.push(disk_path);
            file_artifacts.push(FileArtifact {
                path,
                hash,
                size,
                mtime,
            });
        }
        let (passed, output_hash) = collect()?;
        if !passed {
            return Err(error::DecapodError::ValidationError(format!(
                "decapod validate did not pass; completion verification failed (output_hash={output_hash})"
            )));
        }
        let prepared = PreparedTodoVerification {
            todo_id: todo_id.to_string(),
            expected_revision: task.revision,
            expected_status: task.status,
            repo_root: repo_root.to_path_buf(),
            artifact_paths: resolved_paths,
            artifacts: VerificationArtifacts {
                completed_at: now_iso(),
                task_id: Some(todo_id.to_string()),
                task_revision: Some(task.revision),
                evaluator_epoch: Some(validation_epoch.epoch_id.clone()),
                validation_epoch: Some(validation_epoch.clone()),
                proof_plan_results: vec![ProofPlanResult {
                    proof_gate: "validate_passes".to_string(),
                    status: "pass".to_string(),
                    command: "decapod validate".to_string(),
                    output_hash,
                    evaluator_epoch: Some(validation_epoch.epoch_id.clone()),
                    validation_epoch: Some(validation_epoch),
                }],
                file_artifacts,
            },
        };
        prepared.check_material()?;
        Ok(prepared)
    })();
    if let Err(err) = &result {
        // Failed attempts are durable history, never replacement baselines.
        events::append(
            &store.root,
            events::TODO,
            &serde_json::json!({
                "ts": now_iso(),
                "event_id": crate::core::ulid::new_ulid(),
                "event_type": "task.verify.capture_failed",
                "status": "failed",
                "task_id": todo_id,
                "actor": "decapod",
                "payload": { "last_verified_status": "fail", "reason": err.to_string() }
            }),
        )?;
    }
    result
}

pub fn capture_baseline_for_todo(
    store: &Store,
    repo_root: &Path,
    todo_id: &str,
    artifact_paths: Vec<String>,
) -> Result<(), error::DecapodError> {
    todo::initialize_todo_db(&store.root)?;
    let task = todo::get_task(&store.root, todo_id)?
        .ok_or_else(|| error::DecapodError::NotFound(format!("TODO not found: {todo_id}")))?;
    if task.status != "done" {
        return Err(error::DecapodError::ValidationError(
            "Task must be in done state before capturing verification artifacts".to_string(),
        ));
    }
    let prepared = prepare_verified_completion(store, repo_root, todo_id, artifact_paths)?;
    todo::with_task_journal(
        &DbBroker::new(&store.root),
        &todo::todo_db_path(&store.root),
        "decapod",
        None,
        "verify.capture.write",
        todo::TaskJournalSelection::Ids(vec![todo_id.to_owned()]),
        |conn| prepared.commit_recapture_on_connection(conn),
    )
}

fn refresh_batch_baselines(
    store: &Store,
    targets: &[VerifyTarget],
    snapshot: &ValidationSnapshot,
    validation_epoch: &ValidationEpochMetadata,
) -> Result<Vec<VerifyTarget>, error::DecapodError> {
    if !snapshot.passed {
        return Err(error::DecapodError::ValidationError(
            "cannot recover verification baselines from a failing validation snapshot".to_string(),
        ));
    }

    let completed_at = now_iso();
    let mut updates = Vec::new();
    for target in targets {
        let proof_plan_raw = target.proof_plan.as_deref().ok_or_else(|| {
            error::DecapodError::ValidationError(format!(
                "TODO {} has no proof plan; it must be reopened or completed with --validated before recovery",
                target.todo_id
            ))
        })?;
        let proof_plan: Vec<String> = serde_json::from_str(proof_plan_raw).map_err(|_| {
            error::DecapodError::ValidationError(format!(
                "TODO {} has malformed proof_plan; it must be recaptured before recovery",
                target.todo_id
            ))
        })?;
        if !proof_plan.iter().any(|gate| gate == "validate_passes") {
            return Err(error::DecapodError::ValidationError(format!(
                "TODO {} has no validate_passes proof gate; recovery refuses to invent proof",
                target.todo_id
            )));
        }

        let artifacts_raw = target.artifacts.as_deref().ok_or_else(|| {
            error::DecapodError::ValidationError(format!(
                "TODO {} has no verification artifacts; reopen it or capture artifacts before recovery",
                target.todo_id
            ))
        })?;
        let mut artifacts: VerificationArtifacts =
            serde_json::from_str(artifacts_raw).map_err(|_| {
                error::DecapodError::ValidationError(format!(
                    "TODO {} has malformed verification artifacts; recapture before recovery",
                    target.todo_id
                ))
            })?;
        if artifacts.file_artifacts.is_empty() {
            return Err(error::DecapodError::ValidationError(format!(
                "TODO {} has no file artifacts; recovery refuses to accept unverifiable evidence",
                target.todo_id
            )));
        }

        let proof = artifacts
            .proof_plan_results
            .iter_mut()
            .find(|proof| proof.proof_gate == "validate_passes")
            .ok_or_else(|| {
                error::DecapodError::ValidationError(format!(
                    "TODO {} has no validate_passes artifact; recovery refuses to invent proof",
                    target.todo_id
                ))
            })?;
        proof.status = "pass".to_string();
        proof.command = "decapod validate --format json".to_string();
        proof.output_hash = snapshot.output_hash.clone();
        proof.evaluator_epoch = Some(validation_epoch.epoch_id.clone());
        proof.validation_epoch = Some(validation_epoch.clone());
        artifacts.completed_at = completed_at.clone();
        artifacts.task_id = Some(target.todo_id.clone());
        artifacts.task_revision = Some(target.revision);
        artifacts.evaluator_epoch = Some(validation_epoch.epoch_id.clone());
        artifacts.validation_epoch = Some(validation_epoch.clone());

        updates.push((
            target,
            serde_json::to_string(&artifacts).unwrap(),
            serde_json::to_string(&proof_plan).unwrap(),
            artifacts,
        ));
    }

    let db_path = todo::todo_db_path(&store.root);
    let broker = DbBroker::new(&store.root);
    todo::with_task_journal(
        &broker,
        &db_path,
        "decapod",
        None,
        "verify.recover.baselines",
        todo::TaskJournalSelection::Ids(
            targets
                .iter()
                .map(|target| target.todo_id.clone())
                .collect(),
        ),
        |conn| {
            for (target, _, _, _) in &updates {
                ensure_target_unchanged(conn, target)?;
                if target.status != "done" || target.revision < 0 {
                    return Err(error::DecapodError::ValidationError(
                        "Recovery requires a current completed task revision".into(),
                    ));
                }
            }
            for (target, artifacts_json, proof_plan_json, artifacts) in &updates {
                conn.execute(
                "UPDATE task_verification
                 SET proof_plan = ?1,
                     verification_artifacts = ?2,
                     last_verified_status = 'CLAIMED',
                     last_verified_notes = 'baseline recovered from stable batch validation snapshot; awaiting final aggregate verification',
                     updated_at = ?3
                 WHERE todo_id = ?4",
                crate::core::db::params![proof_plan_json, artifacts_json, completed_at, target.todo_id],
            )?;
                append_verification_event_on_connection(
                    conn,
                    &target.todo_id,
                    "task.verify.recover",
                    &completed_at,
                    serde_json::json!({
                        "proof_plan": serde_json::from_str::<serde_json::Value>(proof_plan_json).map_err(|err| error::DecapodError::ValidationError(err.to_string()))?,
                        "verification_artifacts": artifacts,
                        "validation_epoch": validation_epoch,
                        "snapshot_output_hash": snapshot.output_hash,
                        "last_verified_status": "CLAIMED",
                        "last_verified_notes": "baseline recovered; awaiting final aggregate verification",
                        "verification_policy_days": target.verification_policy_days,
                    }),
                )?;
            }
            Ok(())
        },
    )?;
    Ok(updates
        .into_iter()
        .map(|(target, artifacts_json, proof_plan_json, _)| {
            let mut refreshed = target.clone();
            refreshed.artifacts = Some(artifacts_json);
            refreshed.proof_plan = Some(proof_plan_json);
            refreshed.last_verified_status = Some("CLAIMED".to_string());
            refreshed
        })
        .collect())
}

pub fn prune_verification_for_todo(
    store: &Store,
    todo_id: &str,
) -> Result<(), error::DecapodError> {
    let ts = now_iso();
    let db_path = todo::todo_db_path(&store.root);
    let broker = DbBroker::new(&store.root);

    todo::with_task_journal(
        &broker,
        &db_path,
        "decapod",
        None,
        "verify.prune",
        todo::TaskJournalSelection::Ids(vec![todo_id.to_owned()]),
        |conn| {
            conn.execute(
                "DELETE FROM task_verification WHERE todo_id = ?1",
                [todo_id],
            )?;
            conn.execute(
            "UPDATE tasks SET status = 'open', completed_at = NULL, updated_at = ?1, revision = revision + 1 WHERE id = ?2",
            crate::core::db::params![ts, todo_id],
        )?;
            append_verification_event_on_connection(
                conn,
                todo_id,
                "task.verify.prune",
                &ts,
                serde_json::json!({}),
            )
        },
    )
}

struct VerifyingTodosGuard {
    previous: Option<std::ffi::OsString>,
}

impl VerifyingTodosGuard {
    fn enter(ids: &str) -> Self {
        let previous = std::env::var_os("DECAPOD_VERIFYING_TODO");
        unsafe {
            std::env::set_var("DECAPOD_VERIFYING_TODO", ids);
        }
        Self { previous }
    }
}

impl Drop for VerifyingTodosGuard {
    fn drop(&mut self) {
        unsafe {
            if let Some(previous) = &self.previous {
                std::env::set_var("DECAPOD_VERIFYING_TODO", previous);
            } else {
                std::env::remove_var("DECAPOD_VERIFYING_TODO");
            }
        }
    }
}

/// Recovery has no provisional passing state. Recheck the exact batch and its
/// material, then promote all proof rows and events in a single transaction.
fn finalize_recovered_batch(
    store: &Store,
    repo_root: &Path,
    targets: &[VerifyTarget],
    snapshot: &ValidationSnapshot,
) -> Result<Vec<VerifyTodoResult>, error::DecapodError> {
    if !snapshot.passed {
        return Err(error::DecapodError::ValidationError(
            "Final aggregate validation did not pass; recovered proof remains pending".into(),
        ));
    }
    let ts = now_iso();
    todo::with_task_journal(
        &DbBroker::new(&store.root),
        &todo::todo_db_path(&store.root),
        "decapod",
        None,
        "verify.recover.finalize",
        todo::TaskJournalSelection::Ids(
            targets
                .iter()
                .map(|target| target.todo_id.clone())
                .collect(),
        ),
        |conn| {
            for target in targets {
                ensure_target_unchanged(conn, target)?;
            }
            let mut results = Vec::with_capacity(targets.len());
            for target in targets {
                let result = verify_target(target, &store.root, repo_root, Some(snapshot))?;
                if result.status != "pass" {
                    return Err(error::DecapodError::ValidationError(format!(
                        "Recovered task {} did not pass current proof; the complete batch remains pending: {}",
                        target.todo_id,
                        serde_json::to_string(&result)
                            .map_err(|err| error::DecapodError::ValidationError(err.to_string()))?,
                    )));
                }
                results.push(result);
            }
            for (target, result) in targets.iter().zip(&results) {
                persist_result_on_connection(
                    conn,
                    target,
                    &result.status,
                    &result.notes.join("; "),
                    &ts,
                )?;
            }
            Ok(results)
        },
    )
}

fn run_recover_cli(store: &Store, repo_root: &Path, json: bool) -> Result<(), error::DecapodError> {
    let targets = load_targets(store, None)?;
    if targets.is_empty() {
        return Err(error::DecapodError::ValidationError(
            "no done TODOs are available for proof recovery".to_string(),
        ));
    }

    let target_ids = targets
        .iter()
        .map(|target| target.todo_id.as_str())
        .collect::<Vec<_>>()
        .join(",");
    let _verifying = VerifyingTodosGuard::enter(&target_ids);

    let (passed, output_hash) = run_validate_and_hash(&store.root, repo_root, None)?;
    let snapshot = ValidationSnapshot {
        passed,
        output_hash,
    };
    let validation_epoch = active_validation_epoch(repo_root)?;
    let recovered_targets = refresh_batch_baselines(store, &targets, &snapshot, &validation_epoch)?;

    // These exact tasks remain CLAIMED while their own aggregate admission
    // check is scoped out. All other final validation gates still execute.
    // Finalization rechecks every task/artifact and commits all passes at once.
    let (final_passed, final_hash) = run_validate_and_hash(&store.root, repo_root, None)?;
    let results = finalize_recovered_batch(
        store,
        repo_root,
        &recovered_targets,
        &ValidationSnapshot {
            passed: final_passed,
            output_hash: final_hash.clone(),
        },
    )?;

    if json {
        println!(
            "{}",
            serde_json::json!({
                "status": "ok",
                "recovered": results.len(),
                "validation_epoch": validation_epoch.epoch_id,
                "snapshot_output_hash": snapshot.output_hash,
                "final_output_hash": final_hash
            })
        );
    } else {
        println!(
            "Recovered and verified {} TODO(s) with final aggregate validation passing.",
            results.len()
        );
    }
    Ok(())
}

pub fn run_verify_cli(
    store: &Store,
    repo_root: &Path,
    cli: VerifyCli,
) -> Result<(), error::DecapodError> {
    if let Some(cmd) = &cli.command {
        match cmd {
            VerifyCommand::Regen { id, artifact } => {
                capture_baseline_for_todo(store, repo_root, id, artifact.clone())?;
                if !cli.json {
                    println!("Successfully regenerated verification baseline for task {id}.");
                } else {
                    println!("{}", serde_json::json!({ "status": "ok", "todo_id": id }));
                }
                return Ok(());
            }
            VerifyCommand::Prune { id } => {
                prune_verification_for_todo(store, id)?;
                if !cli.json {
                    println!(
                        "Successfully pruned verification record and reverted task {id} to status 'open'."
                    );
                } else {
                    println!("{}", serde_json::json!({ "status": "ok", "todo_id": id }));
                }
                return Ok(());
            }
            VerifyCommand::Recover => {
                return run_recover_cli(store, repo_root, cli.json);
            }
            VerifyCommand::Completion {
                id,
                write,
                path,
                export,
                import,
            } => {
                if *write && (export.is_some() || import.is_some()) {
                    return Err(error::DecapodError::ValidationError(
                        "completion evidence --write cannot be combined with --export or --import"
                            .to_string(),
                    ));
                }
                if export.is_some() && import.is_some() {
                    return Err(error::DecapodError::ValidationError(
                        "completion evidence --export cannot be combined with --import".to_string(),
                    ));
                }
                if import.is_some() && path.is_some() {
                    return Err(error::DecapodError::ValidationError(
                        "completion evidence --import cannot be combined with --path".to_string(),
                    ));
                }
                if let Some(output_path) = export {
                    let record_path = path
                        .clone()
                        .unwrap_or(completion_evidence::default_record_path(repo_root, id)?);
                    let envelope = completion_evidence::export_record(
                        repo_root,
                        id,
                        &record_path,
                        output_path,
                    )?;
                    if cli.json {
                        println!(
                            "{}",
                            serde_json::json!({
                                "status": "ok",
                                "task_id": id,
                                "export_path": output_path,
                                "envelope_hash": envelope.envelope_hash,
                                "evidence_hash": envelope.record.evidence_hash,
                            })
                        );
                    } else {
                        println!(
                            "portable completion evidence exported: {} ({})",
                            output_path.display(),
                            envelope.envelope_hash
                        );
                    }
                    return Ok(());
                }
                if let Some(input_path) = import {
                    let (stored_path, report) =
                        completion_evidence::import_record(repo_root, id, input_path)?;
                    if cli.json {
                        println!("{}", serde_json::to_string_pretty(&report).unwrap());
                    } else {
                        println!(
                            "portable completion evidence imported: {} [{} / {}]",
                            stored_path.display(),
                            report.structural_status,
                            report.local_decision
                        );
                        for check in &report.checks {
                            let detail = check
                                .detail
                                .as_deref()
                                .map(|detail| format!(" ({detail})"))
                                .unwrap_or_default();
                            println!("- {}: {}{}", check.name, check.status, detail);
                        }
                    }
                    if report.local_decision != "accepted" {
                        return Err(error::DecapodError::ValidationError(format!(
                            "portable completion evidence local decision: {}",
                            report.local_decision
                        )));
                    }
                    return Ok(());
                }
                let record_path = path
                    .clone()
                    .unwrap_or(completion_evidence::default_record_path(repo_root, id)?);
                if *write {
                    let record = completion_evidence::build_record(repo_root, id)?;
                    let written = completion_evidence::write_record(repo_root, &record)?;
                    if cli.json {
                        println!(
                            "{}",
                            serde_json::json!({
                                "status": "ok",
                                "task_id": id,
                                "record_path": written,
                                "evidence_hash": record.evidence_hash,
                            })
                        );
                    } else {
                        println!(
                            "completion evidence written: {} ({})",
                            written.display(),
                            record.evidence_hash
                        );
                    }
                } else {
                    let report = completion_evidence::verify_record(repo_root, id, &record_path)?;
                    if cli.json {
                        println!("{}", serde_json::to_string_pretty(&report).unwrap());
                    } else {
                        println!("completion evidence {id} [{}]", report.status);
                        for check in &report.checks {
                            let detail = check
                                .detail
                                .as_deref()
                                .map(|detail| format!(" ({detail})"))
                                .unwrap_or_default();
                            println!("- {}: {}{}", check.name, check.status, detail);
                        }
                    }
                    if report.status != "current" {
                        return Err(error::DecapodError::ValidationError(format!(
                            "completion evidence verification status: {}",
                            report.status
                        )));
                    }
                }
                return Ok(());
            }
            VerifyCommand::Todo { .. } => {}
        }
    }

    let single_id = cli.command.as_ref().map(|cmd| match cmd {
        VerifyCommand::Todo { id } => id.as_str(),
        _ => unreachable!(),
    });

    let targets = load_targets(store, single_id)?;
    if single_id.is_some() && targets.is_empty() {
        return Err(error::DecapodError::NotFound("TODO not found".to_string()));
    }

    let now = now_iso();
    let now_secs = epoch_secs(&now).unwrap_or(0);

    if cli.stale {
        let stale_items: Vec<StaleItem> = targets
            .iter()
            .filter(|t| t.status == "done" && is_stale(t, now_secs))
            .map(|t| StaleItem {
                todo_id: t.todo_id.clone(),
                last_verified_at: t.last_verified_at.clone(),
                verification_policy_days: t.verification_policy_days,
            })
            .collect();

        if cli.json {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "checked_at": now,
                    "stale": stale_items
                }))
                .unwrap()
            );
        } else if stale_items.is_empty() {
            println!("No stale TODOs found.");
        } else {
            println!("Stale TODOs:");
            for item in stale_items {
                println!(
                    "- {} (last_verified_at={}, policy_days={})",
                    item.todo_id,
                    item.last_verified_at.unwrap_or_else(|| "never".to_string()),
                    item.verification_policy_days
                );
            }
        }
        return Ok(());
    }

    let run_id = crate::core::ulid::new_ulid();
    let mut results = Vec::new();

    let target_ids: Vec<String> = targets.iter().map(|t| t.todo_id.clone()).collect();
    let target_ids_str = target_ids.join(",");
    let _verifying = VerifyingTodosGuard::enter(&target_ids_str);

    let validation_snapshot = if single_id.is_none() {
        let (passed, output_hash) = run_validate_and_hash(&store.root, repo_root, None)?;
        Some(ValidationSnapshot {
            passed,
            output_hash,
        })
    } else {
        None
    };

    for target in &targets {
        let result = verify_target(target, &store.root, repo_root, validation_snapshot.as_ref())?;
        persist_result(store, target, &result.status, &result.notes.join("; "))?;

        append_event(
            store,
            &serde_json::json!({
                "event_type": "verification.todo_result",
                "ts": now,
                "run_id": run_id,
                "todo_id": result.todo_id,
                "status": result.status,
                "proofs": result.proofs,
                "artifacts": result.artifacts,
                "notes": result.notes,
            }),
        )?;

        results.push(result);
    }

    let summary = VerifySummary {
        total: results.len(),
        passed: results.iter().filter(|r| r.status == "pass").count(),
        failed: results.iter().filter(|r| r.status == "fail").count(),
        unknown: results.iter().filter(|r| r.status == "unknown").count(),
        stale: targets.iter().filter(|t| is_stale(t, now_secs)).count(),
    };

    append_event(
        store,
        &serde_json::json!({
            "event_type": "verification.run",
            "ts": now,
            "run_id": run_id,
            "summary": {
                "total": summary.total,
                "passed": summary.passed,
                "failed": summary.failed,
                "unknown": summary.unknown,
                "stale": summary.stale,
            }
        }),
    )?;

    let report = VerifyReport {
        verified_at: now,
        summary,
        results,
    };

    if cli.json {
        println!("{}", serde_json::to_string_pretty(&report).unwrap());
    } else {
        println!("Verification report at {}", report.verified_at);
        for r in &report.results {
            println!("- {} [{}]", r.todo_id, r.status);
            for p in &r.proofs {
                if p.status == "fail" {
                    println!(
                        "  proof {} failed (expected={}, actual={})",
                        p.gate,
                        p.expected_output_hash.as_deref().unwrap_or("n/a"),
                        p.actual_output_hash.as_deref().unwrap_or("n/a")
                    );
                    if let Some(reason) = &p.reason {
                        println!("    reason: {reason}");
                    }
                }
            }
            for a in &r.artifacts {
                if a.status == "fail" {
                    println!(
                        "  artifact {} failed (expected={}, actual={})",
                        a.path,
                        a.expected_hash.as_deref().unwrap_or("n/a"),
                        a.actual_hash.as_deref().unwrap_or("n/a")
                    );
                }
            }
            for n in &r.notes {
                println!("  note: {n}");
            }
        }
        println!(
            "Summary: total={} passed={} failed={} unknown={} stale={}",
            report.summary.total,
            report.summary.passed,
            report.summary.failed,
            report.summary.unknown,
            report.summary.stale
        );
    }

    if report.summary.failed > 0 {
        return Err(error::DecapodError::ValidationError(format!(
            "verification failed for {} TODO(s)",
            report.summary.failed
        )));
    }

    Ok(())
}

/// Emit a read-only recovery report for validation, proof, task, and workspace state.
pub fn run_diagnose_cli(
    store: &Store,
    repo_root: &Path,
    cli: DiagnoseCli,
) -> Result<(), error::DecapodError> {
    let main_root = workspace::get_main_repo_root(repo_root)?;
    let workspace_status = workspace::get_workspace_status(repo_root)?;
    let config = crate::cli::DecapodProjectConfig::load(&main_root).ok();
    let expected_container = config
        .as_ref()
        .map(|config| config.repo.container_workspaces)
        .unwrap_or(false);
    let base_branch = workspace::resolve_base_branch(&main_root, None);

    let tasks = if let Some(id) = cli.todo.as_deref() {
        let task = todo::get_task(&store.root, id)?
            .ok_or_else(|| error::DecapodError::NotFound(format!("TODO not found: {id}")))?;
        vec![task]
    } else {
        todo::list_tasks(
            &store.root,
            Some("done".to_string()),
            None,
            None,
            None,
            None,
        )?
        .into_iter()
        .filter(|task| !task.assigned_to.is_empty())
        .take(12)
        .collect()
    };

    let task_reports = tasks
        .iter()
        .map(|task| {
            let target = load_targets(store, Some(&task.id))?.into_iter().next();
            Ok(diagnose_task(
                task,
                target.as_ref(),
                &workspace_status,
                expected_container,
                &base_branch,
            ))
        })
        .collect::<Result<Vec<_>, error::DecapodError>>()?;

    let report = serde_json::json!({
        "read_only": true,
        "repo_root": main_root.to_string_lossy().to_string(),
        "workspace": {
            "path": repo_root.to_string_lossy().to_string(),
            "branch": workspace_status.git.current_branch.clone(),
            "in_worktree": workspace_status.git.in_worktree,
            "is_main_repo": workspace_status.git.is_main_repo,
            "is_protected": workspace_status.git.is_protected,
            "worktree_path": workspace_status.git.worktree_path.clone(),
            "base_branch": base_branch.clone(),
        },
        "execution_context": {
            "expected": if expected_container { "container workspace" } else { "isolated host worktree" },
            "actual": if workspace_status.container.in_container { "container workspace" } else { "host process" },
            "validation_executed": false,
        },
        "tasks": task_reports,
        "next": if workspace_status.git.is_protected || !workspace_status.git.in_worktree {
            "decapod workspace ensure"
        } else if expected_container && !workspace_status.container.in_container {
            "decapod workspace ensure --container"
        } else {
            "inspect the task-specific next_command"
        },
    });

    if cli.json {
        println!("{}", serde_json::to_string_pretty(&report).unwrap());
    } else {
        println!("Decapod QA diagnostic (read-only)");
        println!("Repository: {}", main_root.display());
        println!(
            "Workspace: branch={} in_worktree={} main_repo={} protected={} path={}",
            workspace_status.git.current_branch,
            workspace_status.git.in_worktree,
            workspace_status.git.is_main_repo,
            workspace_status.git.is_protected,
            repo_root.display()
        );
        println!(
            "Execution context: expected={}, actual={}",
            report["execution_context"]["expected"]
                .as_str()
                .unwrap_or("unknown"),
            report["execution_context"]["actual"]
                .as_str()
                .unwrap_or("unknown")
        );
        if task_reports.is_empty() {
            println!("Tasks: none selected (pass --todo <id> to inspect a specific task)");
        } else {
            for task in &task_reports {
                println!(
                    "Task {}: state={} claim={} proof={} next={}",
                    task["todo_id"].as_str().unwrap_or("unknown"),
                    task["state"].as_str().unwrap_or("unknown"),
                    task["claim"].as_str().unwrap_or("unknown"),
                    task["proof"]["status"].as_str().unwrap_or("unknown"),
                    task["next_command"].as_str().unwrap_or("unknown")
                );
                if let Some(reason) = task["proof"]["notes"].as_str()
                    && !reason.is_empty()
                {
                    println!("  proof note: {reason}");
                }
            }
        }
        println!(
            "Next: {}",
            report["next"].as_str().unwrap_or("inspect task")
        );
    }
    Ok(())
}

fn diagnose_task(
    task: &todo::Task,
    target: Option<&VerifyTarget>,
    workspace_status: &workspace::WorkspaceStatus,
    expected_container: bool,
    base_branch: &str,
) -> serde_json::Value {
    let artifacts = target
        .and_then(|target| target.artifacts.as_deref())
        .and_then(|raw| serde_json::from_str::<serde_json::Value>(raw).ok());
    let proof_status = todo::effective_verification_status(
        target.and_then(|target| target.last_verified_status.as_deref()),
        artifacts.as_ref(),
        &task.id,
        task.revision,
    )
    .unwrap_or("missing");
    let proof_status = match proof_status {
        "pass" | "passed" | "verified" => "pass",
        "stale" => "stale",
        "fail" => "blocked",
        "missing" => "missing",
        _ => "unknown",
    };
    let proof_notes = target
        .and_then(|target| target.artifacts.as_deref())
        .and_then(|raw| serde_json::from_str::<VerificationArtifacts>(raw).ok())
        .and_then(|artifacts| {
            artifacts
                .proof_plan_results
                .into_iter()
                .find(|proof| proof.proof_gate == "validate_passes")
        })
        .map(|proof| {
            format!(
                "baseline_status={} expected_hash={}",
                proof.status, proof.output_hash
            )
        })
        .or_else(|| target.and_then(|target| target.last_verified_at.clone()))
        .unwrap_or_else(|| "verification baseline is missing".to_string());

    let branch_matches = workspace_status.git.current_branch.contains(&task.hash)
        || workspace_status.git.current_branch.contains(&task.id);
    let claim = if task.assigned_to.is_empty() {
        "unclaimed"
    } else {
        "claimed"
    };
    let next_command = if workspace_status.git.is_protected || !workspace_status.git.in_worktree {
        "decapod workspace ensure".to_string()
    } else if expected_container && !workspace_status.container.in_container {
        "decapod workspace ensure --container".to_string()
    } else if !branch_matches {
        format!("switch to the todo-scoped workspace for {}", task.id)
    } else if proof_status == "missing" {
        format!("decapod todo done --id {} --validated", task.id)
    } else if proof_status == "stale" {
        format!("decapod qa verify regen {}", task.id)
    } else if proof_status == "blocked" || proof_status == "unknown" {
        format!("decapod validate && decapod qa verify todo {}", task.id)
    } else {
        "decapod workspace publish".to_string()
    };

    serde_json::json!({
        "todo_id": task.id,
        "state": task.status,
        "claim": claim,
        "assigned_to": task.assigned_to,
        "branch_matches_todo": branch_matches,
        "expected_base_branch": base_branch,
        "proof": {
            "status": proof_status,
            "last_verified_at": target.and_then(|target| target.last_verified_at.clone()),
            "notes": proof_notes,
            "validation_hash_current": "not executed by read-only diagnose",
        },
        "next_command": next_command,
    })
}
#[cfg(test)]
#[path = "../../../tests/unit/plugins/verify_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "../../../tests/unit/plugins/verify_completion_tests.rs"]
mod completion_tests;
