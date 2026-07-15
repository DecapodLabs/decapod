use crate::core::broker::DbBroker;
use crate::core::error;
use crate::core::external_action::{self, ExternalCapability};
use crate::core::state_commit;
use crate::core::store::Store;
use crate::core::todo;
use crate::core::validation_epoch::{ValidationEpochMetadata, active_validation_epoch};
use crate::core::workspace;
use crate::plugins::federation;
use clap::{Parser, Subcommand};
use fancy_regex::Regex;
use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::{self, OpenOptions};
use std::io::Write;
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
}

#[derive(Debug)]
struct VerifyTarget {
    todo_id: String,
    status: String,
    proof_plan: Option<String>,
    artifacts: Option<String>,
    last_verified_at: Option<String>,
    last_verified_status: Option<String>,
    verification_policy_days: i64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct VerificationArtifacts {
    completed_at: String,
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
    let mut old_verifying_todo = None;
    if let Some(id) = todo_id {
        let existing = std::env::var("DECAPOD_VERIFYING_TODO").unwrap_or_default();
        old_verifying_todo = Some(existing.clone());
        let new_val = if existing.is_empty() {
            id.to_string()
        } else if existing.split(',').any(|x| x.trim() == id) {
            existing
        } else {
            format!("{},{}", existing, id)
        };
        unsafe {
            std::env::set_var("DECAPOD_VERIFYING_TODO", new_val);
        }
    }
    let exe = std::env::current_exe()?;
    let exe_str = exe.to_string_lossy().to_string();
    let output = external_action::execute(
        store_root,
        ExternalCapability::VerificationExec,
        "verify.validate_passes",
        &exe_str,
        &["validate", "--format", "json"],
        repo_root,
    );
    if let Some(old_val) = old_verifying_todo {
        unsafe {
            if old_val.is_empty() {
                std::env::remove_var("DECAPOD_VERIFYING_TODO");
            } else {
                std::env::set_var("DECAPOD_VERIFYING_TODO", old_val);
            }
        }
    }
    let output = output?;

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

fn verification_events_path(store: &Store) -> PathBuf {
    store.root.join("verification_events.jsonl")
}

fn append_jsonl(path: &Path, value: &serde_json::Value) -> Result<(), error::DecapodError> {
    let mut f = OpenOptions::new().create(true).append(true).open(path)?;
    writeln!(f, "{}", serde_json::to_string(value).unwrap())?;
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
                "SELECT t.id, t.status, v.proof_plan, v.verification_artifacts, v.last_verified_at, v.last_verified_status, COALESCE(v.verification_policy_days, 90)\n                 FROM tasks t\n                 LEFT JOIN task_verification v ON v.todo_id = t.id\n                 WHERE t.id = ?1",
            )?;
            let rows = stmt.query_map(rusqlite::params![id], |row| {
                Ok(VerifyTarget {
                    todo_id: row.get(0)?,
                    status: row.get(1)?,
                    proof_plan: row.get(2)?,
                    artifacts: row.get(3)?,
                    last_verified_at: row.get(4)?,
                    last_verified_status: row.get(5)?,
                    verification_policy_days: row.get(6)?,
                })
            })?;

            for row in rows {
                out.push(row?);
            }
        } else {
            let mut stmt = conn.prepare(
                "SELECT t.id, t.status, v.proof_plan, v.verification_artifacts, v.last_verified_at, v.last_verified_status, COALESCE(v.verification_policy_days, 90)\n                 FROM tasks t\n                 LEFT JOIN task_verification v ON v.todo_id = t.id\n                 WHERE t.status = 'done'\n                   AND v.proof_plan IS NOT NULL\n                   AND v.proof_plan <> ''\n                 ORDER BY t.updated_at DESC",
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
                })
            })?;

            for row in rows {
                out.push(row?);
            }
        }
        Ok(out)
    })
}

fn persist_result(
    store: &Store,
    todo_id: &str,
    status: &str,
    notes: &str,
) -> Result<(), error::DecapodError> {
    let ts = now_iso();
    let broker = DbBroker::new(&store.root);
    let db_path = todo::todo_db_path(&store.root);
    let (proof_plan, verification_artifacts, verification_policy_days) =
        broker.with_conn(&db_path, "decapod", None, "verify.persist", |conn| {
            let existing: Option<(String, Option<String>, i64)> = conn
                .query_row(
                    "SELECT proof_plan, verification_artifacts, verification_policy_days
                     FROM task_verification
                     WHERE todo_id = ?1",
                    rusqlite::params![todo_id],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )
                .optional()?;
            let (proof_plan, verification_artifacts, verification_policy_days) =
                existing.unwrap_or_else(|| ("[]".to_string(), None, 90));

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
                rusqlite::params![
                    todo_id,
                    proof_plan,
                    verification_artifacts,
                    ts,
                    status,
                    notes,
                    verification_policy_days,
                ],
            )?;
            Ok((proof_plan, verification_artifacts, verification_policy_days))
        })?;

    todo::record_task_event(
        &store.root,
        "task.verify.result",
        Some(todo_id),
        serde_json::json!({
            "proof_plan": serde_json::from_str::<serde_json::Value>(&proof_plan).unwrap_or_else(|_| serde_json::json!([])),
            "verification_artifacts": verification_artifacts
                .as_ref()
                .and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok())
                .unwrap_or(serde_json::Value::Null),
            "last_verified_status": status,
            "last_verified_notes": notes,
            "verification_policy_days": verification_policy_days
        }),
    )?;
    mirror_verification_to_federation(
        store,
        todo_id,
        &format!("Verification Result: {todo_id}"),
        &format!("Verification status={status} notes={notes}"),
        "proof,verification,result",
    );
    Ok(())
}

fn is_stale(target: &VerifyTarget, now_secs: i64) -> bool {
    match target.last_verified_at.as_deref().and_then(epoch_secs) {
        None => true,
        Some(last) => now_secs.saturating_sub(last) > target.verification_policy_days * 86_400,
    }
}

fn resolve_artifact_path(repo_root: &Path, stored: &str) -> PathBuf {
    let path = Path::new(stored);
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        repo_root.join(path)
    }
}

fn verify_target(
    target: &VerifyTarget,
    store_root: &Path,
    repo_root: &Path,
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
                    reason: Some(format!(
                        "validation epoch changed: captured {} but active {}. Re-run `decapod todo done --validated` or `decapod qa verify capture --id {}` to recapture current proof evidence.",
                        captured_epoch.epoch_id, active_epoch.epoch_id, target.todo_id
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
        let (validate_ok, actual_hash) =
            run_validate_and_hash(store_root, repo_root, Some(&target.todo_id))?;
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

pub fn capture_baseline_for_todo(
    store: &Store,
    repo_root: &Path,
    todo_id: &str,
    artifact_paths: Vec<String>,
) -> Result<(), error::DecapodError> {
    todo::initialize_todo_db(&store.root)?;

    let broker = DbBroker::new(&store.root);
    let db_path = todo::todo_db_path(&store.root);
    let status: Option<String> =
        broker.with_conn(&db_path, "decapod", None, "verify.capture.read", |conn| {
            let status = conn
                .query_row(
                    "SELECT status FROM tasks WHERE id = ?1",
                    rusqlite::params![todo_id],
                    |row| row.get(0),
                )
                .optional()?;
            Ok(status)
        })?;

    let Some(task_status) = status else {
        return Err(error::DecapodError::NotFound(format!(
            "TODO not found: {todo_id}"
        )));
    };

    if task_status != "done" {
        return Err(error::DecapodError::ValidationError(
            "Task must be in done state before capturing verification artifacts".to_string(),
        ));
    }

    let paths = if artifact_paths.is_empty() {
        vec!["AGENTS.md".to_string()]
    } else {
        artifact_paths
    };

    let mut file_artifacts = Vec::new();
    for path in paths {
        let disk_path = resolve_artifact_path(repo_root, &path);
        if !disk_path.exists() {
            return Err(error::DecapodError::NotFound(format!(
                "Verification artifact file not found: {}. `--artifact` expects a file path relative to the repo root, for example `--artifact README.md`. To attach notes, use `decapod todo comment --id {}`.",
                disk_path.display(),
                todo_id
            )));
        }
        let (hash, size, mtime) = hash_file(&disk_path)?;
        file_artifacts.push(FileArtifact {
            path,
            hash,
            size,
            mtime,
        });
    }

    let (validate_ok, output_hash) = run_validate_and_hash(&store.root, repo_root, Some(todo_id))?;
    let validation_epoch = active_validation_epoch(repo_root)?;

    let ts = now_iso();
    let artifacts = VerificationArtifacts {
        completed_at: ts.clone(),
        evaluator_epoch: Some(validation_epoch.epoch_id.clone()),
        validation_epoch: Some(validation_epoch.clone()),
        proof_plan_results: vec![ProofPlanResult {
            proof_gate: "validate_passes".to_string(),
            status: if validate_ok {
                "pass".to_string()
            } else {
                "fail".to_string()
            },
            command: "decapod validate".to_string(),
            output_hash,
            evaluator_epoch: Some(validation_epoch.epoch_id.clone()),
            validation_epoch: Some(validation_epoch.clone()),
        }],
        file_artifacts,
    };

    let artifacts_json = serde_json::to_string(&artifacts).unwrap();
    let proof_plan_json = serde_json::to_string(&vec!["validate_passes"]).unwrap();
    let baseline_status = if validate_ok { "pass" } else { "fail" };
    let baseline_notes = if validate_ok {
        "baseline captured"
    } else {
        "baseline captured while validate was failing"
    };

    broker.with_conn(&db_path, "decapod", None, "verify.capture.write", |conn| {
        conn.execute(
            "INSERT INTO task_verification(todo_id, proof_plan, verification_artifacts, last_verified_at, last_verified_status, last_verified_notes, verification_policy_days, updated_at)\n             VALUES(?1, ?2, ?3, ?4, ?5, ?6, 90, ?4)\n             ON CONFLICT(todo_id) DO UPDATE SET\n               proof_plan=excluded.proof_plan,\n               verification_artifacts=excluded.verification_artifacts,\n               last_verified_at=excluded.last_verified_at,\n               last_verified_status=excluded.last_verified_status,\n               last_verified_notes=excluded.last_verified_notes,\n               verification_policy_days=excluded.verification_policy_days,\n               updated_at=excluded.updated_at",
            rusqlite::params![
                todo_id,
                proof_plan_json,
                artifacts_json,
                ts,
                baseline_status,
                baseline_notes
            ],
        )?;
        Ok(())
    })?;

    todo::record_task_event(
        &store.root,
        "task.verify.capture",
        Some(todo_id),
        serde_json::json!({
            "proof_plan": ["validate_passes"],
            "verification_artifacts": artifacts,
            "last_verified_status": baseline_status,
            "last_verified_notes": baseline_notes,
            "verification_policy_days": 90
        }),
    )?;
    Ok(())
}

pub fn prune_verification_for_todo(
    store: &Store,
    todo_id: &str,
) -> Result<(), error::DecapodError> {
    let ts = now_iso();
    let db_path = todo::todo_db_path(&store.root);
    let broker = DbBroker::new(&store.root);

    broker.with_conn(&db_path, "decapod", None, "verify.prune", |conn| {
        conn.execute(
            "DELETE FROM task_verification WHERE todo_id = ?1",
            [todo_id],
        )?;
        conn.execute(
            "UPDATE tasks SET status = 'open', completed_at = NULL, updated_at = ?1 WHERE id = ?2",
            rusqlite::params![ts, todo_id],
        )?;
        Ok(())
    })?;

    todo::record_task_event(
        &store.root,
        "task.verify.prune",
        Some(todo_id),
        serde_json::json!({}),
    )?;
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

    let old_verifying = std::env::var("DECAPOD_VERIFYING_TODO").ok();
    let target_ids: Vec<String> = targets.iter().map(|t| t.todo_id.clone()).collect();
    let target_ids_str = target_ids.join(",");
    unsafe {
        std::env::set_var("DECAPOD_VERIFYING_TODO", &target_ids_str);
    }

    for target in &targets {
        let result = verify_target(target, &store.root, repo_root)?;
        persist_result(
            store,
            &result.todo_id,
            &result.status,
            &result.notes.join("; "),
        )?;

        append_jsonl(
            &verification_events_path(store),
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

    append_jsonl(
        &verification_events_path(store),
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

    unsafe {
        if let Some(val) = old_verifying {
            std::env::set_var("DECAPOD_VERIFYING_TODO", val);
        } else {
            std::env::remove_var("DECAPOD_VERIFYING_TODO");
        }
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
    let proof_status = target
        .and_then(|target| target.last_verified_status.as_deref())
        .unwrap_or("missing");
    let proof_status = match proof_status {
        "pass" => "pass",
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
mod tests {
    use super::validation_proof_reason;

    #[test]
    fn stable_failed_validation_remains_blocked_with_explicit_reason() {
        assert_eq!(
            validation_proof_reason(false, true, "bugs_01test"),
            "decapod validate did not pass; output hash is unchanged from the baseline, so verification remains blocked. Next: fix the reported validation gate, then run `decapod qa verify todo bugs_01test`"
        );
    }

    #[test]
    fn failed_validation_with_changed_output_reports_validation_failure() {
        assert_eq!(
            validation_proof_reason(false, false, "bugs_01test"),
            "decapod validate did not pass. Next: fix the reported validation gate, then run `decapod qa verify todo bugs_01test`"
        );
    }

    #[test]
    fn passing_validation_with_changed_output_reports_hash_drift() {
        assert_eq!(
            validation_proof_reason(true, false, "bugs_01test"),
            "validate output hash changed. Next: review the drift, then recapture with `decapod qa verify regen bugs_01test` if the change is intentional"
        );
    }
}
