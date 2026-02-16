use crate::core::broker::DbBroker;
use crate::core::error;
use crate::core::external_action::{self, ExternalCapability};
use crate::core::store::Store;
use crate::plugins::federation;
use crate::plugins::todo;
use clap::{Parser, Subcommand};
use regex::Regex;
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

#[derive(Subcommand, Debug)]
pub enum VerifyCommand {
    /// Verify a single TODO.
    Todo {
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
    verification_policy_days: i64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct VerificationArtifacts {
    completed_at: String,
    proof_plan_results: Vec<ProofPlanResult>,
    file_artifacts: Vec<FileArtifact>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct ProofPlanResult {
    proof_gate: String,
    status: String,
    command: String,
    output_hash: String,
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
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    format!("{}Z", secs)
}

fn epoch_secs(ts: &str) -> Option<i64> {
    ts.trim_end_matches('Z').parse::<i64>().ok()
}

fn normalize_validate_output(raw: &str) -> String {
    let ansi = Regex::new(r"\x1B\[[0-9;]*[A-Za-z]").expect("valid ANSI regex");
    let stripped = ansi.replace_all(raw, "");
    stripped
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(|line| {
            if line.contains("decapod_validate_user_") || line.contains("decapod_validate_repo_") {
                "<tmp_validate_path>".to_string()
            } else {
                line.to_string()
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
                normalized.insert(key.clone(), normalize_json_value(&map[key]));
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
) -> Result<(bool, String), error::DecapodError> {
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
    let source = format!("event:{}", todo_id);
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
    ) {
        if let Some(intent_or_commitment) = anchor {
            let _ = federation::add_edge(store, &intent_or_commitment, &node.id, "depends_on");
        }
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
                "SELECT t.id, t.status, v.proof_plan, v.verification_artifacts, v.last_verified_at, COALESCE(v.verification_policy_days, 90)\n                 FROM tasks t\n                 LEFT JOIN task_verification v ON v.todo_id = t.id\n                 WHERE t.id = ?1",
            )?;
            let rows = stmt.query_map(rusqlite::params![id], |row| {
                Ok(VerifyTarget {
                    todo_id: row.get(0)?,
                    status: row.get(1)?,
                    proof_plan: row.get(2)?,
                    artifacts: row.get(3)?,
                    last_verified_at: row.get(4)?,
                    verification_policy_days: row.get(5)?,
                })
            })?;

            for row in rows {
                out.push(row?);
            }
        } else {
            let mut stmt = conn.prepare(
                "SELECT t.id, t.status, v.proof_plan, v.verification_artifacts, v.last_verified_at, COALESCE(v.verification_policy_days, 90)\n                 FROM tasks t\n                 LEFT JOIN task_verification v ON v.todo_id = t.id\n                 WHERE t.status = 'done'\n                   AND v.proof_plan IS NOT NULL\n                   AND v.proof_plan <> ''\n                 ORDER BY t.updated_at DESC",
            )?;
            let rows = stmt.query_map([], |row| {
                Ok(VerifyTarget {
                    todo_id: row.get(0)?,
                    status: row.get(1)?,
                    proof_plan: row.get(2)?,
                    artifacts: row.get(3)?,
                    last_verified_at: row.get(4)?,
                    verification_policy_days: row.get(5)?,
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
        &format!("Verification Result: {}", todo_id),
        &format!("Verification status={} notes={}", status, notes),
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

    if proof_plan.len() != 1 || proof_plan.first().map(String::as_str) != Some("validate_passes") {
        result.status = "unknown".to_string();
        result.notes.push(
            "Unsupported proof_plan for MVP. Remediation: set proof_plan to [\"validate_passes\"] and capture artifacts.".to_string(),
        );
        return Ok(result);
    }

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

    let (validate_ok, actual_hash) = run_validate_and_hash(store_root, repo_root)?;
    let expected = expected_hash.unwrap_or_default();

    if !validate_ok {
        result.status = "fail".to_string();
        result.proofs.push(ProofCheckResult {
            gate: "validate_passes".to_string(),
            status: "fail".to_string(),
            expected_output_hash: Some(expected),
            actual_output_hash: Some(actual_hash),
            reason: Some("decapod validate did not pass".to_string()),
        });
    } else if actual_hash != expected {
        result.status = "fail".to_string();
        result.proofs.push(ProofCheckResult {
            gate: "validate_passes".to_string(),
            status: "fail".to_string(),
            expected_output_hash: Some(expected),
            actual_output_hash: Some(actual_hash),
            reason: Some("validate output hash changed".to_string()),
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
            "TODO not found: {}",
            todo_id
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
                "Verification artifact file not found: {}",
                disk_path.display()
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

    let (validate_ok, output_hash) = run_validate_and_hash(&store.root, repo_root)?;

    let ts = now_iso();
    let artifacts = VerificationArtifacts {
        completed_at: ts.clone(),
        proof_plan_results: vec![ProofPlanResult {
            proof_gate: "validate_passes".to_string(),
            status: if validate_ok {
                "pass".to_string()
            } else {
                "fail".to_string()
            },
            command: "decapod validate".to_string(),
            output_hash,
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

pub fn run_verify_cli(
    store: &Store,
    repo_root: &Path,
    cli: VerifyCli,
) -> Result<(), error::DecapodError> {
    let single_id = cli
        .command
        .as_ref()
        .map(|VerifyCommand::Todo { id }| id.as_str());

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

    let run_id = ulid::Ulid::new().to_string();
    let mut results = Vec::new();

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
                println!("  note: {}", n);
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
