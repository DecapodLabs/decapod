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
use crate::core::validation_history::{self, HistoricalValidationProof, IntegrationWitness};
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
    /// Inspect recorded full validation for an immutable source commit; never publishes.
    Commit {
        #[clap(value_name = "SHA")]
        sha: String,
    },
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
    Recover {
        /// Fresh evidence for a completed TODO, as TODO_ID=FILE; repeat per file/task.
        /// Required for tasks without a usable baseline; never imports an old CI pass.
        #[clap(long = "artifact", value_name = "TODO_ID=FILE")]
        artifact: Vec<String>,
    },
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

/// The application routes this before migrations, presence, and session writes.
pub fn is_commit_lookup(cli: &VerifyCli) -> bool {
    matches!(cli.command, Some(VerifyCommand::Commit { .. }))
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    historical_validation: Option<HistoricalCompletionEvidence>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct HistoricalCompletionEvidence {
    candidate_sha: String,
    record_id: String,
    record_digest: String,
    integration: IntegrationWitness,
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
            "decapod validate did not pass; output hash is unchanged from the baseline, so verification remains blocked. Agent: run `decapod validate --format json` to inspect the current blockers, fix the reported validation gate, then run `decapod qa verify todo {todo_id}`"
        )
    } else if !validate_ok {
        format!(
            "decapod validate did not pass. Agent: run `decapod validate --format json` to inspect the current blockers, fix the reported validation gate, then run `decapod qa verify todo {todo_id}`"
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
        "validation epoch changed: captured {captured_epoch} but active {active_epoch}. Agent: review the material drift, then run `decapod qa verify recover` to revalidate the completed batch without reopening it. If current evidence is missing, supply `decapod qa verify recover --artifact {todo_id}=<current-file>` and repeat --artifact for each task requiring evidence. Recovery still requires actual passing validation; repair unrelated gates rather than pruning completion history."
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

/// Compare validation semantics across executions, not the location or identity
/// of one execution's evidence. The complete receipts remain intact in the
/// validation output and are verified by readiness before success is returned.
/// Only these exact report fields are execution-specific; material, epoch,
/// decisions, scopes, blockers and unknown fields must still affect replay.
fn normalize_validation_document(value: &serde_json::Value) -> serde_json::Value {
    let mut normalized = normalize_json_value(value);
    if let Some(field) = normalized.pointer_mut("/readiness/evidence_path")
        && field.is_string()
    {
        *field = serde_json::Value::String("<execution-evidence>".into());
    }
    // Mask only execution identity fields in named readiness receipts.
    // New receipt implementations get the same replay contract without
    // hiding their material, mode, decision, blockers, or unknown fields.
    if let Some(receipts) = normalized
        .get_mut("readiness")
        .and_then(serde_json::Value::as_object_mut)
    {
        for (name, receipt) in receipts {
            if !name.ends_with("_readiness") || !receipt.is_object() {
                continue;
            }
            for pointer in ["/request_id", "/binding/evidence_digest"] {
                if let Some(field) = receipt.pointer_mut(pointer)
                    && field.is_string()
                {
                    *field = serde_json::Value::String("<execution-evidence>".into());
                }
            }
        }
    }
    normalized
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

const MAX_DIAGNOSTIC_TEXT_SCAN_BYTES: usize = 1024 * 1024;
const MAX_DIAGNOSTIC_JSON_BYTES: usize = 16 * 1024 * 1024;
const MAX_DIAGNOSTIC_LABELS: usize = 8;
const MAX_DIAGNOSTIC_MESSAGE_BYTES: usize = 1023;

const SAFE_VALIDATION_SCOPES: &[&str] = &[
    "publication_readiness",
    "source_pr_readiness",
    "repository_local_readiness",
    "repository_local_git_readiness",
    "ci_local",
    "project_prerequisites",
];

// Compile-time allowlist of canonical blocker labels. Only entries from this
// table (never arbitrary child stdout/stderr text, paths, URLs, credentials, or
// task titles) may be emitted in recovery diagnostics.
const SAFE_BLOCKER_LABELS: &[&str] = &[
    "READINESS_GATE_OVERRIDE_UNSUPPORTED",
    "NEEDS_PLAN_APPROVAL",
    "PROOF_HOOK_FAILED",
    "SNAPSHOT_NOT_READY",
    "REQUIRED_PROOF_NOT_AUTHENTICATED",
    "CONTROLLER_CONTRACT_UNSUPPORTED",
    "SPEC_REVIEW_REQUIRED",
    "STALE_SOURCE_INVENTORY",
    "SNAPSHOT_UPLOAD_PENDING",
    "WORKSPACE_PROTECTION",
    "MISSING_PROOF_PROFILE",
    "UNKNOWN_PROOF_PROFILE",
    "PROOF_PROFILE_RECONCILIATION_REQUIRED",
    "PROOF_PROFILE_INVALID",
    "MISSING_REQUIRED_PROOF_ADAPTER",
    "READINESS_PROFILE_REQUIRED",
    "READINESS_PROVIDER_MISMATCH",
    "REPOSITORY_PROFILE_PROVIDER_UNSUPPORTED",
    "ACCEPTED_REPOSITORY_CONTRACT_CHANGED",
    "REPOSITORY_CONTRACT_INVALID",
    "REPOSITORY_CONTRACT_REQUIRED",
    "REPOSITORY_COMMAND_CONTRACT",
    "REPOSITORY_PROOF_SEAL",
    "READINESS_SNAPSHOT_ADOPTED_PROOF_PENDING",
    "READINESS_INVOCATION_INVALID",
    "READINESS_TOOL_MISSING",
    "READINESS_BASE_MISSING",
    "READINESS_HEAD_MISSING",
    "READINESS_TOOLCHAIN_CONTRACT",
    "READINESS_DEADLINE_EXCEEDED",
    "SNAPSHOT_UPLOAD_INCOMPLETE",
    "INCOMPLETE_AUTHENTICATED_CATALOG",
    "LEGACY_TEMPORARY_SNAPSHOT_UNSUPPORTED",
    "SOURCE_CHANGED_DURING_SNAPSHOT_PROOF",
    "DERIVED_RELEASE_CATALOG_MISSING",
    "DERIVED_RELEASE_CUSTODY_BLOCKED",
    "MISSING_SOURCE_EXECUTOR",
    "SOURCE_EXECUTION_CUSTODY_BLOCKED",
    "LOCAL_TARGET_HOST_MISMATCH",
    "LOCAL_TARGET_IDENTITY_CHANGED",
    "LOCAL_TARGET_PUBLICATION_UNSUPPORTED",
    "PROJECT_PREREQUISITES_FAILED",
    "PROJECT_PREREQUISITES_PROVIDER_MISMATCH",
    "LAYA_READINESS_BLOCKED",
    "LAYA_POLICY_CHANGED_DURING_INFERENCE",
    "LAYA_POLICY_CHANGED_DURING_VALIDATION",
    "NATIVE_EXECUTION_BLOCKED",
    "NATIVE_SELECTION_DEADLINE",
    "NATIVE_SLICE_FAILED",
    "UNSUPPORTED_NATIVE_SLICE",
    "PARTIAL_NATIVE_EVIDENCE",
    "PARTIAL_NATIVE_PROOF",
    "SOURCE_CHANGED_DURING_NATIVE_SLICE",
    "ACCEPTED_BASE_NOT_ANCESTOR",
    "STALE_ACCEPTED_BASE",
    "MISSING_PROOF_SOURCE",
    "BUILDKITE_SOURCE_CUSTODY_BLOCKED",
    "MISMATCHED_CI_CONTRACT",
    "UNKNOWN_CI_CONTRACT",
    "NO_BUILD_OR_CI_CONTRACT",
    "COMMIT_AND_RERUN",
    "ENTRYPOINT_COMMIT_DISCIPLINE",
    "FINGERPRINT_ONLY_SPECS",
    "REMOTE_FINGERPRINT_ONLY_SPECS",
    "STALE_SPEC_REVIEW",
    "STALE_SPECS_FINGERPRINT",
    "STALE_SPECS_TEMPLATE",
    "STALE_CONFIG_INPUT_HASH",
    "STALE_ENTRYPOINT_RELEASE",
    "OUT_OF_SYNC_SPECS",
    "MALFORMED_PROJECT_SPEC_LAYOUT",
    "PROOF_DUAL_AUTHORITY",
    "MANAGED_SPEC_PROJECTIONS_MISSING",
    "GOVERNANCE_CHECKPOINT_MISSING",
    "GOVERNANCE_CHECKPOINT_ID_CONFLICT",
    "GOVERNANCE_CHECKPOINT_HISTORY_CHANGED",
    "GOVERNANCE_HISTORY_INVALID",
    "GOVERNANCE_HISTORY_INPUT_UNAVAILABLE",
    "GOVERNANCE_ACCEPTED_BASE_MISSING",
    "GOVERNANCE_ACCEPTED_CHECKPOINT_CHANGED",
    "GOVERNANCE_ACCEPTED_BASELINE_CHANGED",
    "GOVERNANCE_ACCEPTED_IDENTITY_CONFLICT",
    "GOVERNANCE_PR_IDENTITY_MISSING",
    "GOVERNANCE_PR_IDENTITY_CONFLICT",
    "GOVERNANCE_PR_NOT_ACCEPTED",
    "GOVERNANCE_PR_TARGET_CHANGED",
    "GOVERNANCE_PR_TARGET_MISMATCH",
    "GOVERNANCE_PR_BASE_MISMATCH",
    "GOVERNANCE_PR_UPDATES",
    "GOVERNANCE_PR_DIFF_MISSING",
    "GOVERNANCE_PR_DIFF_UNAVAILABLE",
    "GOVERNANCE_GIT_PROOF_UNAVAILABLE",
    "GOVERNANCE_EMPTY_CHANGE",
    "GOVERNANCE_PROTECTED_BRANCH",
    "GOVERNANCE_UNSTAGED_MATERIAL",
    "GOVERNANCE_UNARCHIVED_WORK",
    "GOVERNANCE_WORK_CONTEXT_REQUIRED",
    "GOVERNANCE_BRANCH_CREATION_PROOF_REQUIRED",
    "GOVERNANCE_WORKSPACE_ALIAS",
    "GOVERNANCE_AUTHORITY_CONFLICT",
    "GOVERNANCE_SCHEMA_UNSUPPORTED",
    "GOVERNANCE_LOCK_BUSY",
    "GOVERNANCE_ARTIFACTS_INVALID",
    "GOVERNANCE_VALIDATION_MISSING",
    "GOVERNANCE_VALIDATION_INVALID",
    "VALIDATION_ARTIFACTS_NOT_STAGED",
    "VALIDATION_RECEIPT_MISMATCH",
    "VALIDATION_HISTORY_PERSIST_FAILED",
    "STALE_VALIDATION_EVIDENCE",
    "VALIDATION_NOT_READY",
    "VALIDATE_TIMEOUT_OR_LOCK",
    "VALIDATE_WORKER_FAILED",
    "PUBLICATION_NOT_READY",
    "PUBLICATION_NOT_PROVEN",
    "PUBLICATION_MATERIAL_BASE_MISSING",
    "PUBLICATION_MATERIAL_BINDING_REQUIRED",
    "PR_BASE_REF_MISSING",
    "PR_MERGE_CONFLICT",
    "PR_TRAJECTORY_MISSING",
    "PR_TARGET_MISMATCH",
    "PR_DIFF_MISMATCH",
    "PR_NUMBER_MISMATCH",
    "PR_PUBLICATION_RACE",
    "PR_CREATE_UNCONFIRMED",
    "PR_CREATED_NOT_ADMITTED",
    "PR_QUARANTINE_TARGET_UNVERIFIED",
    "PR_QUARANTINE_STATE_UNKNOWN",
    "PR_QUARANTINE_UNCONFIRMED",
    "PR_QUARANTINE_UNAVAILABLE_ALREADY_MERGED",
    "TRAJECTORY_COOKIE_MISSING",
    "TRAJECTORY_BLOCKERS_PRESENT",
    "TRAJECTORY_TASK_MISMATCH",
    "TRAJECTORY_PROOF_INSUFFICIENT",
    "WORKUNIT_MANIFEST_MISSING",
    "WORKUNIT_NOT_VERIFIED",
    "EVAL_AGGREGATE_MISSING",
    "EVAL_GATE_FAILED",
    "NO_NETWORK_GIT_REMOTE",
    "NEEDS_HUMAN_INPUT",
    "PLAN_CHANGED_REQUIRES_APPROVAL",
    "PLAN_REQUIRED",
    "PHASE_REQUIRED",
    "PHASES_INCOMPLETE",
    "PHASE_GATE_NOT_SATISFIED",
    "PHASE_LOCKED",
    "PHASE_NOT_FOUND",
    "INVALID_PHASE_CONTRACT",
    "INVALID_PHASE_TRANSITION",
    "TODO_PROJECTION_MISSING",
    "SCOPE_VIOLATION",
    "PROTECTED_PATH_VIOLATION",
    "TASK_FORBIDDEN_PATH_VIOLATION",
    "INVALID_WORKSPACE_CUSTODY",
    "STALE_OR_MISSING_EVIDENCE",
    "UNVERIFIED_COMPLETION",
    "FILE_TOUCH_BUDGET_EXCEEDED",
    "ROOT_REPOSITORY_MODIFIED",
    "WORKSPACE_REQUIRED",
    "WORKSPACE_BLOCKED",
    "WORKSPACE_TODO_CLAIM_CONFLICT",
    "WORKSPACE_BRANCH_TODO_CLAIM_CONFLICT",
    "WORKSPACE_BRANCH_NOT_TODO_SCOPED",
    "WORKSPACE_STORAGE_PREFLIGHT_FAILED",
    "WORKSPACE_SCHEMA_REPAIR_FAILED",
    "WORKSPACE_PATH_OWNERSHIP_CONFLICT",
    "WORKTREE_CUSTODY_MISMATCH",
    "WORKTREE_CUSTODY_REQUIRED",
    "VALIDATION_RULES_FAILED",
    "VALIDATION_RULE_VIOLATED",
    "VALIDATION_RULE_STALE",
    "VALIDATION_RULE_UNVERIFIABLE",
    "PROJECT_SPECS_MANIFEST_MISSING",
    "PROJECT_SPECS_MANIFEST_MISMATCH",
    "SNAPSHOT_CLEANUP_PENDING",
    "INVENTORY_INCOMPLETE",
    "AUTHORING_ACTOR_REQUIRED",
    "AUTHORING_BINDING_REQUIRED",
    "AUTHORING_BINDING_STALE",
    "AUTHORING_CHECKOUT_CHANGED",
    "AUTHORING_CHECKOUT_OCCUPIED",
    "AUTHORING_CHECKOUT_UNVERIFIED",
    "AUTHORING_CUSTODY_LIMIT",
    "AUTHORING_CUSTODY_STALE",
    "AUTHORING_DEPENDENCIES_BLOCKED",
    "AUTHORING_EVIDENCE_UNAVAILABLE",
    "AUTHORING_PROFILE_UNSUPPORTED",
    "AUTHORING_STORE_INVALID",
    "AUTHORING_TASK_ALREADY_BOUND",
    "AUTHORING_TASK_AMBIGUOUS",
    "ACCEPTED_TARGET_UNAVAILABLE",
    "ACCEPTED_POLICY_ABSENT",
    "CI_ATTESTED_POLICY_NOT_ACCEPTED",
    "CI_CHANGE_CONTEXT_INVALID",
    "INVALID_CONFIG_SCHEMA",
    "GOVERNANCE_HISTORY_NOT_PROVEN",
    "INVALID_PLAN",
    "INVALID_WORKTREE_CONFIG",
    "LAYA_ACCEPTED_POLICY_CHANGED",
    "LOCAL_TARGET_INVALID",
    "LOCAL_TARGET_POLICY_UNACCEPTED",
    "LOCAL_TARGET_UNAVAILABLE",
    "LOCAL_PUBLICATION_RACE",
    "NOT_READY_AFTER_RUST_WORKER",
    "OUT_OF_SYNC_ENTRYPOINTS",
    "PUBLICATION_BUNDLE_CURRENCY",
    "PUBLICATION_CUSTODY_CHANGED",
    "PUBLICATION_DECISION_REQUIRED",
    "PUBLICATION_INCOMPLETE",
    "PUBLICATION_LAYA_CHANGED",
    "PUBLICATION_MATERIAL_BASE_CHANGED",
    "PUBLICATION_OUTCOME_UNKNOWN",
    "PUBLICATION_PROOF_MISMATCH",
    "PUBLICATION_PROOF_MISSING",
    "PUBLICATION_PROOF_SUBJECT_CHANGED",
    "PUBLICATION_WORKBRANCH_REQUIRED",
    "RAW_PROVIDER_ARTIFACT_REQUIRED",
    "REMOTE_GOVERNANCE_DIFF_MISSING",
    "REMOTE_HEAD_MISMATCH",
    "REMOTE_PUBLICATION_RACE",
    "REMOTE_SPECS_DIFF_MISSING",
    "REPOSITORY_PROOF_SEAL_EXPIRED",
    "REPOSITORY_PROOF_SEAL_MISMATCH",
    "REPOSITORY_PROOF_SEAL_MISSING",
    "REPOSITORY_PROOF_SEAL_STALE",
    "REPOSITORY_PROOF_POLICY_DENIED",
    "REPOSITORY_PROOF_REJECTED",
    "READINESS_GATE_OVERRIDE_UNSUPPORTED",
    "READINESS_REPOSITORY_MISSING",
    "READINESS_REPOSITORY_UNSUPPORTED",
    "REQUIRED_PROFILE_DOWNGRADE",
    "RUST_WORKER_CONTRACT_INVALID",
    "RUST_WORKER_FAILED",
    "RUST_WORKER_INPUT_UNTRACKED",
    "RUST_WORKER_NOT_READY",
    "RUST_WORKER_POLICY_DOWNGRADE",
    "RUST_WORKER_POLICY_STALE",
    "RUST_WORKER_PROOF_OUTPUT_INVALID",
    "RUST_WORKER_STRICT_PROFILE_REQUIRED",
    "RUST_WORKER_TOOLCHAIN_CONFLICT",
    "RUST_WORKER_TOOLCHAIN_REQUIRED",
    "RUST_WORKER_WORKSPACE_REQUIRED",
    "SPEC_REVIEW_DECISION_REQUIRED",
    "SPEC_REVIEW_DISPOSITION_MISMATCH",
    "STALE_COMMAND_CONTRACT",
    "STALE_POLICY_AFTER_RUST_WORKER",
    "STALE_POST_VALIDATION_DECISION",
    "STALE_PUBLICATION_REVISION",
    "STALE_PUBLICATION_TASK",
    "STALE_PUBLICATION_VALIDATION",
    "STALE_TASK_VERIFICATION",
    "TERMINAL_VALIDATION_RULES_FAILED",
    "TERMINAL_WORKUNIT_STALE",
    "TERMINAL_WORKUNIT_UNPROVEN",
    "UNCOMMITTED_GOVERNANCE_ARTIFACT",
    "UNCOMMITTED_PUBLICATION_CONTENT",
    "UNSTAGED_GOVERNANCE_ARTIFACT",
    "UNSUPPORTED_SOURCE_CONTRACT",
    "UNKNOWN_REQUIRED_PROFILE",
    "MISSING_PROFILE_SOURCE",
    "MISSING_REQUIRED_PROFILE",
    "MISSING_NATIVE_CARGO",
    "MISSING_NATIVE_LINKER",
    "MISSING_NATIVE_MDBOOK_RESOLUTION",
    "MISSING_NATIVE_PLATFORM",
    "MISSING_NATIVE_RUST",
    "MISSING_NATIVE_RUST_RESOLUTION",
    "MISSING_NATIVE_SEQUENCE_ADAPTER",
    "MISSING_NATIVE_TARGET",
    "MISSING_NATIVE_TOOL",
    "MISSING_NATIVE_TOOLCHAIN_CLOSURE",
    "NATIVE_RUST_VERSION_MISMATCH",
    "VALIDATION_HISTORY_BACKEND_UNSUPPORTED",
    "VALIDATION_HISTORY_BINDING_MISMATCH",
    "VALIDATION_HISTORY_DEPENDENCIES_BLOCKED",
    "VALIDATION_HISTORY_INVALID",
    "VALIDATION_HISTORY_MISSING",
    "VALIDATION_HISTORY_OWNER_MISMATCH",
    "VALIDATION_HISTORY_TASK_MISMATCH",
    "VALIDATION_HISTORY_TASK_PROOF_UNSUPPORTED",
    "VALIDATION_HISTORY_TASK_STALE",
    "VALIDATION_HISTORY_TASK_UNBOUND",
    "WORKSPACE_AMBIGUOUS_TASK_HASH",
    "WORKSPACE_BASE_UNRESOLVED",
    "WORKSPACE_OWNERSHIP_UNVERIFIED",
    "WORKUNIT_PROOF_UNVERIFIABLE",
    "WORKUNIT_PROOF_STALE",
    "PROJECT_CONFIG_INVALID",
    "PROOF_CONFIG_MISMATCH",
    "LIVING_SPECS_INCOMPLETE",
    "GATEKEEPER_VIOLATION",
    "LINEAGE_GATE_VIOLATION",
    "CONTEXT_CAPSULE_INVALID",
    "MANAGED_DOCKERFILE_MISMATCH",
    "ROOT_DOCKERFILE_MISCONFIGURED",
    "ENTRYPOINT_CONTRACT_INVALID",
    "SCHEMA_NON_DETERMINISTIC",
    "REPOMAP_NON_DETERMINISTIC",
    "ASSURANCE_MIGRATION_REQUIRED",
    "CARGO_FMT_FAILED",
    "CARGO_CLIPPY_FAILED",
    "CARGO_FMT_EXECUTION_FAILED",
    "CARGO_CLIPPY_EXECUTION_FAILED",
    "RUSTFMT_UNAVAILABLE",
    "CLIPPY_UNAVAILABLE",
    "LINT_FAILED",
    // Keep generic umbrella last so specific unwrapped labels take priority when capping.
    "READINESS_BLOCKED",
];

#[derive(Debug, Clone, PartialEq, Eq)]
struct SafeValidationDiagnostic {
    stage: Option<&'static str>,
    scope: Option<&'static str>,
    labels: Vec<&'static str>,
    message: String,
}

impl SafeValidationDiagnostic {
    fn empty() -> Self {
        Self {
            stage: None,
            scope: None,
            labels: Vec::new(),
            message: String::new(),
        }
    }
}

fn canonical_safe_label(candidate: &str) -> Option<&'static str> {
    let normalized = candidate.trim();
    SAFE_BLOCKER_LABELS
        .iter()
        .copied()
        .find(|label| label.eq_ignore_ascii_case(normalized))
}

fn canonical_safe_scope(value: Option<&serde_json::Value>) -> Option<&'static str> {
    let raw = value.and_then(|v| v.as_str())?.trim();
    SAFE_VALIDATION_SCOPES
        .iter()
        .copied()
        .find(|scope| *scope == raw)
}

fn push_unique_label(labels: &mut Vec<&'static str>, label: &'static str) {
    if !labels.contains(&label) {
        labels.push(label);
    }
}

fn is_token_ident_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

fn contains_token_boundary(bytes: &[u8], token: &str) -> bool {
    let needle = token.as_bytes();
    if needle.is_empty() || bytes.len() < needle.len() {
        return false;
    }
    bytes
        .windows(needle.len())
        .enumerate()
        .any(|(idx, window)| {
            if window != needle {
                return false;
            }
            let left_ok = idx == 0 || !is_token_ident_byte(bytes[idx - 1]);
            let right_idx = idx + needle.len();
            let right_ok = right_idx == bytes.len() || !is_token_ident_byte(bytes[right_idx]);
            left_ok && right_ok
        })
}

fn collect_tokens_bounded(bytes: &[u8], labels: &mut Vec<&'static str>) {
    let slice = &bytes[..bytes.len().min(MAX_DIAGNOSTIC_TEXT_SCAN_BYTES)];
    for &label in SAFE_BLOCKER_LABELS {
        if contains_token_boundary(slice, label) {
            push_unique_label(labels, label);
        }
    }
}

fn strip_known_prefix<'a>(s: &'a str, prefix: &str) -> Option<&'a str> {
    s.strip_prefix(prefix)
}

fn extract_labels_from_cannot_publish(rest: &str, labels: &mut Vec<&'static str>) {
    let rest = rest.trim_start();
    if let Some((token, _)) = split_leading_code_token(rest)
        && let Some(label) = canonical_safe_label(token)
    {
        push_unique_label(labels, label);
        return;
    }
    let fixed_mappings: &[(&str, &'static str)] = &[
        (
            "no network-capable git remote is configured",
            "NO_NETWORK_GIT_REMOTE",
        ),
        ("not in a git worktree", "WORKSPACE_REQUIRED"),
        ("on protected branch", "GOVERNANCE_PROTECTED_BRANCH"),
        ("cannot resolve HEAD", "READINESS_HEAD_MISSING"),
        (
            "missing required workunit manifest",
            "WORKUNIT_MANIFEST_MISSING",
        ),
        ("workunit ", "WORKUNIT_NOT_VERIFIED"),
        ("missing trajectory cookie", "TRAJECTORY_COOKIE_MISSING"),
        (
            "missing governance validation section",
            "GOVERNANCE_VALIDATION_MISSING",
        ),
        (
            "invalid governance validation section",
            "GOVERNANCE_VALIDATION_INVALID",
        ),
        (
            "validation receipt is not bound",
            "VALIDATION_RECEIPT_MISMATCH",
        ),
        ("missing governed plan", "PLAN_REQUIRED"),
        (
            "required governance artifact is missing or invalid",
            "GOVERNANCE_ARTIFACTS_INVALID",
        ),
        (
            "required governance artifacts are missing or invalid",
            "GOVERNANCE_ARTIFACTS_INVALID",
        ),
        (
            "required validation artifact was not staged",
            "VALIDATION_ARTIFACTS_NOT_STAGED",
        ),
        (
            "failed to inspect the PR diff",
            "GOVERNANCE_PR_DIFF_UNAVAILABLE",
        ),
        (
            "required governance artifacts are not included in the PR diff",
            "GOVERNANCE_PR_DIFF_MISSING",
        ),
        ("required eval aggregate ", "EVAL_AGGREGATE_MISSING"),
        ("eval gate failed for aggregate ", "EVAL_GATE_FAILED"),
        ("base branch ", "GOVERNANCE_ACCEPTED_BASE_MISSING"),
        (
            "code-related changes versus ",
            "MANAGED_SPEC_PROJECTIONS_MISSING",
        ),
    ];
    for &(prefix, label) in fixed_mappings {
        if rest.starts_with(prefix) {
            push_unique_label(labels, label);
            return;
        }
    }
    if rest.starts_with("trajectory ") {
        if rest.contains(" has blockers:") {
            push_unique_label(labels, "TRAJECTORY_BLOCKERS_PRESENT");
        } else if rest.contains(" is bound to task ") {
            push_unique_label(labels, "TRAJECTORY_TASK_MISMATCH");
        } else if rest.contains(" has insufficient proof status ") {
            push_unique_label(labels, "TRAJECTORY_PROOF_INSUFFICIENT");
        }
    }
}

fn split_leading_code_token(s: &str) -> Option<(&str, &str)> {
    let trimmed = s.trim_start();
    let end = trimmed
        .find(|c: char| !(c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_'))
        .unwrap_or(trimmed.len());
    if end == 0 {
        return None;
    }
    let token = &trimmed[..end];
    let after = &trimmed[end..];
    if after.is_empty()
        || after.starts_with(':')
        || after.starts_with(" —")
        || after.starts_with(" -")
        || after.starts_with(" (")
        || after.starts_with(" or ")
        || after.starts_with('\n')
        || after.starts_with(' ')
    {
        Some((token, after))
    } else {
        None
    }
}

// Inspect only structured leading wrappers/error codes of a validation blocker
// or failure string so arbitrary untrusted suffixes (task titles, trajectory
// blocker details, rule descriptions, URLs, paths) never trigger label emission.
fn extract_labels_from_structured_text(raw: &str, labels: &mut Vec<&'static str>) {
    let mut current = raw.trim_start();
    for _ in 0..4 {
        let mut stripped = false;
        for scope in SAFE_VALIDATION_SCOPES {
            if let Some(after_scope) = current.strip_prefix(scope)
                && let Some(after_colon) = after_scope.strip_prefix(':')
            {
                current = after_colon.trim_start();
                stripped = true;
                break;
            }
        }
        if stripped {
            continue;
        }
        for prefix in ["Validation error:", "gate error:", "Error:"] {
            if let Some(rest) = strip_known_prefix(current, prefix) {
                current = rest.trim_start();
                stripped = true;
                break;
            }
        }
        if !stripped {
            break;
        }
    }

    if let Some(after_auto) = current.strip_prefix("AUTOREMEDIABLE_VALIDATION_ERROR ") {
        if let Some(after_code_kw) = after_auto.strip_prefix("code=") {
            let token_end = after_code_kw
                .find(|c: char| !is_token_ident_byte(c as u8))
                .unwrap_or(after_code_kw.len());
            if let Some(label) = canonical_safe_label(&after_code_kw[..token_end]) {
                push_unique_label(labels, label);
            }
        }
        if let Some((_, body)) = after_auto.split_once('\n') {
            extract_labels_from_structured_text(body, labels);
        }
        return;
    }

    if let Some(rest) = current.strip_prefix("Cannot publish:") {
        extract_labels_from_cannot_publish(rest, labels);
        return;
    }

    if let Some(rest) = current.strip_prefix("Rule ") {
        if let Some((_, body)) = rest.split_once(':') {
            let body_trimmed = body.trim_start();
            if body_trimmed.starts_with("Rule violated") {
                push_unique_label(labels, "VALIDATION_RULE_VIOLATED");
            } else if body_trimmed.starts_with("Rule stale") {
                push_unique_label(labels, "VALIDATION_RULE_STALE");
            } else if body_trimmed.starts_with("Rule unverifiable") {
                push_unique_label(labels, "VALIDATION_RULE_UNVERIFIABLE");
            } else if let Some((token, _)) = split_leading_code_token(body_trimmed)
                && let Some(label) = canonical_safe_label(token)
            {
                push_unique_label(labels, label);
            } else {
                push_unique_label(labels, "VALIDATION_RULES_FAILED");
            }
        } else {
            push_unique_label(labels, "VALIDATION_RULES_FAILED");
        }
        return;
    }

    if current.starts_with("Validation rules:") {
        push_unique_label(labels, "VALIDATION_RULES_FAILED");
        return;
    }

    if current.starts_with("Currently on protected branch")
        || current.starts_with("Protected branch")
    {
        push_unique_label(labels, "GOVERNANCE_PROTECTED_BRANCH");
        return;
    }

    if let Some(rest) = current.strip_prefix("TASK:") {
        let rest_trimmed = rest.trim_start();
        if rest_trimmed.starts_with("Project specs manifest missing") {
            push_unique_label(labels, "PROJECT_SPECS_MANIFEST_MISSING");
        } else if rest_trimmed.starts_with("Project specs manifest schema mismatch") {
            push_unique_label(labels, "PROJECT_SPECS_MANIFEST_MISMATCH");
        }
        return;
    }

    let fixed_gate_prefixes: &[(&str, &'static str)] = &[
        ("Project config ", "PROJECT_CONFIG_INVALID"),
        ("Guided init ", "PROJECT_CONFIG_INVALID"),
        ("Proof command", "PROOF_CONFIG_MISMATCH"),
        (
            "Missing required project specs file:",
            "LIVING_SPECS_INCOMPLETE",
        ),
        ("Architecture spec ", "LIVING_SPECS_INCOMPLETE"),
        ("Intent spec ", "LIVING_SPECS_INCOMPLETE"),
        ("Interfaces spec ", "LIVING_SPECS_INCOMPLETE"),
        ("Validation spec ", "LIVING_SPECS_INCOMPLETE"),
        ("Semantics spec ", "LIVING_SPECS_INCOMPLETE"),
        ("Operations spec ", "LIVING_SPECS_INCOMPLETE"),
        ("Security spec ", "LIVING_SPECS_INCOMPLETE"),
        ("Gatekeeper violations:", "GATEKEEPER_VIOLATION"),
        ("Lineage gate violations:", "LINEAGE_GATE_VIOLATION"),
        ("Context capsule ", "CONTEXT_CAPSULE_INVALID"),
        (
            "Managed Dockerfile release pin failure",
            "MANAGED_DOCKERFILE_MISMATCH",
        ),
        ("Root Dockerfile contains", "ROOT_DOCKERFILE_MISCONFIGURED"),
        ("AGENTS.md ", "ENTRYPOINT_CONTRACT_INVALID"),
        ("CLAUDE.md ", "ENTRYPOINT_CONTRACT_INVALID"),
        ("GEMINI.md ", "ENTRYPOINT_CONTRACT_INVALID"),
        ("CODEX.md ", "ENTRYPOINT_CONTRACT_INVALID"),
        (
            "Schema output is non-deterministic",
            "SCHEMA_NON_DETERMINISTIC",
        ),
        (
            "Repo map output is non-deterministic",
            "REPOMAP_NON_DETERMINISTIC",
        ),
        (
            "Live assurance JSONL still present",
            "ASSURANCE_MIGRATION_REQUIRED",
        ),
        ("Python linting failed", "LINT_FAILED"),
        ("Shell script linting failed", "LINT_FAILED"),
        ("YAML linting failed", "LINT_FAILED"),
        ("Dockerfile linting failed", "LINT_FAILED"),
    ];
    for &(prefix, label) in fixed_gate_prefixes {
        if current.starts_with(prefix) {
            push_unique_label(labels, label);
            return;
        }
    }

    // Peel repeatable `<ALLOWLISTED_CODE>: ` chains (for example `READINESS_BLOCKED:
    // FINGERPRINT_ONLY_SPECS: ...`) while checking non-code nested wrappers.
    for _ in 0..3 {
        let Some((token, after)) = split_leading_code_token(current) else {
            break;
        };
        let Some(label) = canonical_safe_label(token) else {
            break;
        };
        push_unique_label(labels, label);
        let next = after
            .trim_start()
            .trim_start_matches([':', '—', '-'])
            .trim_start();
        if next.starts_with("Cannot publish:") {
            if let Some(rest) = next.strip_prefix("Cannot publish:") {
                extract_labels_from_cannot_publish(rest, labels);
            }
            break;
        }
        if next == current || next.is_empty() {
            break;
        }
        current = next;
    }
}

fn extract_from_validation_json(
    value: &serde_json::Value,
    stderr: &[u8],
) -> Option<SafeValidationDiagnostic> {
    let obj = value.as_object()?;
    let looks_like_validate = obj.contains_key("gate")
        || obj.contains_key("readiness")
        || obj.contains_key("report")
        || obj.contains_key("status");
    if !looks_like_validate {
        return None;
    }

    let mut stage: Option<&'static str> = None;
    let top_scope = canonical_safe_scope(obj.get("scope"));
    let mut readiness_scope: Option<&'static str> = None;
    let mut labels: Vec<&'static str> = Vec::new();

    if obj.get("gate").and_then(|v| v.as_str()) == Some("workspace_protection")
        && obj.get("success").and_then(|v| v.as_bool()) == Some(false)
    {
        stage = Some("workspace_protection");
        push_unique_label(&mut labels, "WORKSPACE_PROTECTION");
        if obj.get("is_protected").and_then(|v| v.as_bool()) == Some(true) {
            push_unique_label(&mut labels, "GOVERNANCE_PROTECTED_BRANCH");
        }
        if let Some(err) = obj.get("error").and_then(|v| v.as_str()) {
            extract_labels_from_structured_text(err, &mut labels);
        }
    }

    let report_failed = obj
        .get("report")
        .and_then(|v| v.as_object())
        .is_some_and(|report| {
            report.get("status").and_then(|v| v.as_str()) == Some("fail")
                || report
                    .get("fail_count")
                    .and_then(|v| v.as_u64())
                    .is_some_and(|count| count > 0)
        });

    if report_failed {
        if stage.is_none() {
            stage = Some("project_gates");
        }
        if let Some(failures) = obj
            .get("report")
            .and_then(|v| v.as_object())
            .and_then(|r| r.get("failures"))
            .and_then(|v| v.as_array())
        {
            for failure in failures {
                if let Some(line) = failure.as_str() {
                    extract_labels_from_structured_text(line, &mut labels);
                }
            }
        }
    }

    if let Some(readiness) = obj.get("readiness").and_then(|v| v.as_object()) {
        let ready = readiness.get("ready").and_then(|v| v.as_bool());
        if ready == Some(false) {
            readiness_scope = canonical_safe_scope(readiness.get("scope"));
            if stage.is_none() {
                stage = if readiness.get("proof_ready").and_then(|v| v.as_bool()) == Some(true) {
                    Some("post_validation_readiness")
                } else {
                    Some("readiness")
                };
            }
            if let Some(blockers) = readiness.get("blockers").and_then(|v| v.as_array()) {
                for blocker in blockers {
                    let Some(blocker_obj) = blocker.as_object() else {
                        continue;
                    };
                    if let Some(detail) = blocker_obj.get("detail").and_then(|v| v.as_str()) {
                        extract_labels_from_structured_text(detail, &mut labels);
                    }
                    if let Some(code) = blocker_obj.get("code").and_then(|v| v.as_str())
                        && let Some(label) = canonical_safe_label(code)
                    {
                        push_unique_label(&mut labels, label);
                    }
                }
            }
        }
    }

    if stage.is_none() && obj.get("status").and_then(|v| v.as_str()) == Some("fail") {
        stage = Some("validation_report");
    }

    collect_tokens_bounded(stderr, &mut labels);

    let scope = match stage {
        Some("readiness") | Some("post_validation_readiness") => readiness_scope.or(top_scope),
        _ => top_scope.or(readiness_scope),
    };

    Some(build_safe_validation_diagnostic(stage, scope, labels))
}

fn format_diagnostic_header(stage: Option<&'static str>, scope: Option<&'static str>) -> String {
    let mut header = String::from("VALIDATION_CHILD_FAILED");
    if let Some(stage) = stage {
        header.push_str("; stage=");
        header.push_str(stage);
    }
    if let Some(scope) = scope {
        header.push_str("; scope=");
        header.push_str(scope);
    }
    header
}

fn build_safe_validation_diagnostic(
    stage: Option<&'static str>,
    scope: Option<&'static str>,
    mut labels: Vec<&'static str>,
) -> SafeValidationDiagnostic {
    // Keep non-generic specific labels ahead of READINESS_BLOCKED so bounded
    // output never drops the root blocker code behind the outer wrapper.
    labels.sort_by_key(|label| *label == "READINESS_BLOCKED");

    let header = format_diagnostic_header(stage, scope);
    if labels.is_empty() {
        let suffix = ": no recognized safe blocker label; inspect scoped recovery JSON (`--json`) or run `decapod validate --format json` (note: unscoped validate may stop earlier at `PROOF_HOOK_FAILED`), repair its blockers, then retry the original verification command. No child output was forwarded.";
        let message = format!("{header}{suffix}");
        return SafeValidationDiagnostic {
            stage,
            scope,
            labels,
            message,
        };
    }

    let mid = ": recognized labels observed in scoped validation output: ";
    let suffix = "; inspect scoped recovery JSON (`--json`) or run `decapod validate --format json` (note: unscoped validate may stop earlier at `PROOF_HOOK_FAILED`), repair the blockers, then retry the original verification command. Observed labels are not independently established causes or proof; no child output was forwarded.";

    let mut chosen: Vec<&'static str> = Vec::new();
    let mut labels_len = 0usize;
    let fixed_len = header.len() + mid.len() + suffix.len();
    for label in labels {
        if chosen.len() >= MAX_DIAGNOSTIC_LABELS {
            break;
        }
        let sep = if chosen.is_empty() { 0 } else { 2 };
        if fixed_len + labels_len + sep + label.len() > MAX_DIAGNOSTIC_MESSAGE_BYTES {
            break;
        }
        labels_len += sep + label.len();
        chosen.push(label);
    }

    let message = format!("{header}{mid}{}{suffix}", chosen.join(", "));
    SafeValidationDiagnostic {
        stage,
        scope,
        labels: chosen,
        message,
    }
}

// Return only fixed diagnostic labels and static stage/scope names, never
// child-controlled text, paths, environment values or provider URLs. These
// diagnostics survive scoped recovery validation, even when a separate
// ordinary validate would hit aggregate proof first.
fn extract_safe_validation_diagnostic(stdout: &[u8], stderr: &[u8]) -> SafeValidationDiagnostic {
    if stdout.len() <= MAX_DIAGNOSTIC_JSON_BYTES
        && let Ok(value) = serde_json::from_slice::<serde_json::Value>(stdout)
        && let Some(parsed) = extract_from_validation_json(&value, stderr)
    {
        return parsed;
    }

    let mut labels = Vec::new();
    collect_tokens_bounded(stdout, &mut labels);
    collect_tokens_bounded(stderr, &mut labels);
    build_safe_validation_diagnostic(None, None, labels)
}

#[cfg(test)]
fn safe_validation_failure_diagnostic(stdout: &[u8], stderr: &[u8]) -> String {
    extract_safe_validation_diagnostic(stdout, stderr).message
}

// Test-only seam (#1527): counts entries into the child-validation
// entrypoint so tests can prove that listing paths never launch the
// validation worker. Thread-local because each #[test] runs on its own
// thread; compiled out of production builds entirely.
#[cfg(test)]
thread_local! {
    static VALIDATION_SPAWN_COUNT: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

#[cfg(test)]
fn validation_spawn_count() -> usize {
    VALIDATION_SPAWN_COUNT.with(|count| count.get())
}

#[derive(Debug, Clone)]
struct ValidationRunOutcome {
    passed: bool,
    output_hash: String,
    diagnostic: SafeValidationDiagnostic,
}

fn emit_recovery_failure_json(
    phase: &'static str,
    todo_id: Option<&str>,
    outcome: &ValidationRunOutcome,
) {
    let mut payload = serde_json::json!({
        "status": "error",
        "error": "VALIDATION_CHILD_FAILED",
        "phase": phase,
        "stage": outcome.diagnostic.stage,
        "scope": outcome.diagnostic.scope,
        "labels": outcome.diagnostic.labels,
        "output_hash": outcome.output_hash,
        "diagnostic": outcome.diagnostic.message,
    });
    if let Some(id) = todo_id {
        payload["todo_id"] = serde_json::Value::String(id.to_string());
    }
    println!("{payload}");
}

fn run_validate_and_hash_detailed(
    store_root: &Path,
    repo_root: &Path,
    todo_id: Option<&str>,
) -> Result<ValidationRunOutcome, error::DecapodError> {
    crate::core::validation_runtime::require_full_readiness_environment()?;
    #[cfg(test)]
    VALIDATION_SPAWN_COUNT.with(|count| count.set(count.get() + 1));
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
    let diagnostic = if !output.status.success() {
        let diag = extract_safe_validation_diagnostic(&output.stdout, &output.stderr);
        eprintln!("{}", diag.message);
        let store = Store {
            kind: crate::core::store::StoreKind::Repo,
            root: store_root.to_path_buf(),
        };
        if let Some(diagnostic) =
            validation_history::current_target_only_diagnostic(repo_root, &store)
        {
            eprintln!("{diagnostic}");
        }
        diag
    } else {
        SafeValidationDiagnostic::empty()
    };
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(&stdout) {
        let normalized = normalize_validation_document(&value);
        let canonical = serde_json::to_string(&normalized).unwrap();
        return Ok(ValidationRunOutcome {
            passed: output.status.success(),
            output_hash: sha256_hex(canonical.as_bytes()),
            diagnostic,
        });
    }

    let mut merged = stdout;
    if !output.stderr.is_empty() {
        merged.push('\n');
        merged.push_str(&String::from_utf8_lossy(&output.stderr));
    }

    let normalized = normalize_validate_output(&merged);
    Ok(ValidationRunOutcome {
        passed: output.status.success(),
        output_hash: sha256_hex(normalized.as_bytes()),
        diagnostic,
    })
}

fn run_validate_and_hash(
    store_root: &Path,
    repo_root: &Path,
    todo_id: Option<&str>,
) -> Result<(bool, String), error::DecapodError> {
    let outcome = run_validate_and_hash_detailed(store_root, repo_root, todo_id)?;
    Ok((outcome.passed, outcome.output_hash))
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
    crate::core::storage_contracts::validate_verification_write(
        &target.todo_id,
        proof_plan,
        target.artifacts.as_deref(),
        Some(status),
        Some(notes),
    )?;
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

fn stale_items_from_targets(targets: &[VerifyTarget], now: &str) -> Vec<StaleItem> {
    let now_secs = epoch_secs(now).unwrap_or(0);
    targets
        .iter()
        .filter(|t| t.status == "done" && is_stale(t, now_secs))
        .map(|t| StaleItem {
            todo_id: t.todo_id.clone(),
            last_verified_at: t.last_verified_at.clone(),
            verification_policy_days: t.verification_policy_days,
        })
        .collect()
}

/// Stale listing for the whole done batch; read-only, never validates (#1527).
fn collect_stale_items(store: &Store) -> Result<(String, Vec<StaleItem>), error::DecapodError> {
    let targets = load_targets(store, None)?;
    let now = now_iso();
    let stale_items = stale_items_from_targets(&targets, &now);
    Ok((now, stale_items))
}

fn stale_listing_json(checked_at: &str, stale_items: &[StaleItem]) -> String {
    serde_json::to_string_pretty(&serde_json::json!({
        "checked_at": checked_at,
        "stale": stale_items,
    }))
    .unwrap()
}

fn print_stale_listing(json: bool, checked_at: &str, stale_items: &[StaleItem]) {
    if json {
        println!("{}", stale_listing_json(checked_at, stale_items));
    } else if stale_items.is_empty() {
        println!("No stale TODOs found.");
    } else {
        println!("Stale TODOs:");
        for item in stale_items {
            println!(
                "- {} (last_verified_at={}, policy_days={})",
                item.todo_id,
                item.last_verified_at
                    .clone()
                    .unwrap_or_else(|| "never".to_string()),
                item.verification_policy_days
            );
        }
    }
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
            result.notes.push(missing_artifacts_remediation(
                store_root,
                &target.todo_id,
                "Missing verification metadata.",
            ));
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

    let supported_proofs = ["validate_passes", "state_commit", "validated_commit"];
    if proof_plan
        .iter()
        .any(|p| !supported_proofs.iter().any(|sp| p == *sp))
    {
        result.status = "unknown".to_string();
        result.notes.push(
            "Unsupported proof_plan. Supported gates: validate_passes, state_commit, validated_commit. Recapture evidence through the corresponding governed completion command.".to_string(),
        );
        return Ok(result);
    }

    // Check validate_passes if in plan
    let validate_check_needed = proof_plan.iter().any(|p| p == "validate_passes");

    let artifacts_raw = match target.artifacts.as_deref() {
        Some(v) if !v.trim().is_empty() => v,
        _ => {
            result.status = "unknown".to_string();
            result.notes.push(missing_artifacts_remediation(
                store_root,
                &target.todo_id,
                "Missing verification_artifacts.",
            ));
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

    if artifacts.historical_validation.is_some()
        || proof_plan.iter().any(|p| p == "validated_commit")
    {
        return verify_historical_target(target, store_root, repo_root, &proof_plan, &artifacts);
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
        // Resolve through the same task workspace search used when this
        // baseline was captured. Otherwise a valid artifact that exists only
        // in the task worktree is recorded successfully but cannot verify.
        let disk_path =
            resolve_artifact_path_for_todo(repo_root, Some(&target.todo_id), &expected.path);
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

fn requires_current_validation(target: &VerifyTarget) -> bool {
    let historical_gate = target
        .proof_plan
        .as_deref()
        .and_then(|raw| serde_json::from_str::<Vec<String>>(raw).ok())
        .is_some_and(|gates| gates.iter().any(|gate| gate == "validated_commit"));
    let historical_artifacts = target
        .artifacts
        .as_deref()
        .and_then(|raw| serde_json::from_str::<VerificationArtifacts>(raw).ok())
        .is_some_and(|artifacts| artifacts.historical_validation.is_some());
    // A corrupt or incomplete historical baseline must fail on its own terms;
    // a fresh child run cannot repair or reinterpret the original source proof.
    !historical_gate && !historical_artifacts
}

fn verify_historical_target(
    target: &VerifyTarget,
    store_root: &Path,
    repo_root: &Path,
    proof_plan: &[String],
    artifacts: &VerificationArtifacts,
) -> Result<VerifyTodoResult, error::DecapodError> {
    let check = (|| -> Result<String, error::DecapodError> {
        let history = artifacts.historical_validation.as_ref().ok_or_else(|| {
            error::DecapodError::ValidationError(
                "VALIDATION_HISTORY_MISSING: historical proof reference is absent".into(),
            )
        })?;
        if proof_plan != ["validated_commit"]
            || artifacts.proof_plan_results.len() != 1
            || artifacts.proof_plan_results[0].proof_gate != "validated_commit"
            || artifacts.proof_plan_results[0].status != "pass"
            || artifacts.proof_plan_results[0].output_hash != history.record_digest
            || !artifacts.file_artifacts.is_empty()
        {
            return Err(error::DecapodError::ValidationError(
                "VALIDATION_HISTORY_INVALID: mixed or malformed historical proof".into(),
            ));
        }
        let store = Store {
            kind: crate::core::store::StoreKind::Repo,
            root: store_root.to_path_buf(),
        };
        let proof = validation_history::lookup_record(
            repo_root,
            &store,
            &history.candidate_sha,
            &history.record_id,
        )?;
        if proof.task_binding().task_id != target.todo_id
            || proof.record_id() != history.record_id
            || proof.record_digest() != history.record_digest
            || artifacts.validation_epoch.as_ref() != Some(proof.validation_epoch())
            || history.integration.candidate_sha != proof.candidate_sha()
            || history.integration.accepted_base != proof.accepted_base()
        {
            return Err(error::DecapodError::ValidationError(
                "VALIDATION_HISTORY_BINDING_MISMATCH: historical source, task or evidence changed"
                    .into(),
            ));
        }
        proof.check_integration_witness(repo_root, &history.integration)?;
        DbBroker::new(store_root).with_conn(
            &todo::todo_db_path(store_root), "decapod", None, "verify.history.replay", |conn| {
                ensure_target_unchanged(conn, target)?;
                proof.recheck_on_conn(conn)?;
                todo::check_validation_binding_on_conn(conn, proof.task_binding(), true)?;
                if todo::dependency_completion_gate_on_conn(conn, store_root, &target.todo_id)?.is_some() {
                    return Err(error::DecapodError::ValidationError("VALIDATION_HISTORY_DEPENDENCIES_BLOCKED: current task dependencies must be complete".into()));
                }
                Ok(())
            },
        )?;
        Ok(history.record_digest.clone())
    })();
    let (status, hash, notes) = match check {
        Ok(hash) => ("pass", Some(hash), Vec::new()),
        Err(error) => ("fail", None, vec![error.to_string()]),
    };
    Ok(VerifyTodoResult {
        todo_id: target.todo_id.clone(),
        status: status.into(),
        proofs: vec![ProofCheckResult {
            gate: "validated_commit".into(),
            status: status.into(),
            expected_output_hash: artifacts
                .historical_validation
                .as_ref()
                .map(|h| h.record_digest.clone()),
            actual_output_hash: hash,
            reason: notes.first().cloned(),
        }],
        artifacts: Vec::new(),
        notes,
    })
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
    historical: Option<HistoricalValidationProof>,
    inspected_target: Option<VerifyTarget>,
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

    /// Run while the original task projection still exists under the caller's
    /// completion transaction, before its status/revision update.
    pub(crate) fn check_task_binding_on_connection(
        &self,
        conn: &crate::core::db::Connection,
    ) -> Result<(), error::DecapodError> {
        if let Some(proof) = &self.historical {
            if proof.task_binding().assignee != crate::current_agent_id() {
                return Err(error::DecapodError::ValidationError(
                    "VALIDATION_HISTORY_OWNER_MISMATCH: historical completion requires the recorded task owner".into(),
                ));
            }
            if let Some(target) = &self.inspected_target {
                ensure_target_unchanged(conn, target)?;
                ensure_historical_proof_plan(target.proof_plan.as_deref())?;
            }
            proof.recheck_on_conn(conn)?;
            todo::check_validation_binding_on_conn(conn, proof.task_binding(), true)?;
        }
        Ok(())
    }

    fn check_material(&self) -> Result<(), error::DecapodError> {
        if let Some(history) = &self.artifacts.historical_validation {
            let proof = self.historical.as_ref().ok_or_else(|| {
                error::DecapodError::ValidationError(
                    "VALIDATION_HISTORY_MISSING: prepared historical proof is absent".into(),
                )
            })?;
            return proof.check_integration_witness(&self.repo_root, &history.integration);
        }
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
        self.check_task_binding_on_connection(conn)?;
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
        if let Some(proof) = &self.historical {
            proof.recheck_on_conn(conn)?;
        }
        let ts = &self.artifacts.completed_at;
        let mut artifacts = self.artifacts.clone();
        artifacts.task_id = Some(self.todo_id.clone());
        artifacts.task_revision = Some(expected_revision);
        let artifacts_json = serde_json::to_string(&artifacts)
            .map_err(|err| error::DecapodError::ValidationError(err.to_string()))?;
        let proof_plan = if self.historical.is_some() {
            "[\"validated_commit\"]"
        } else {
            "[\"validate_passes\"]"
        };
        crate::core::storage_contracts::validate_verification_write(
            &self.todo_id,
            proof_plan,
            Some(&artifacts_json),
            Some("pass"),
            Some("baseline captured"),
        )?;
        conn.execute(
            "INSERT INTO task_verification(todo_id, proof_plan, verification_artifacts, last_verified_at, last_verified_status, last_verified_notes, verification_policy_days, updated_at)
             VALUES(?1, ?4, ?2, ?3, 'pass', 'baseline captured', 90, ?3)
             ON CONFLICT(todo_id) DO UPDATE SET
               proof_plan=excluded.proof_plan,
               verification_artifacts=excluded.verification_artifacts,
               last_verified_at=excluded.last_verified_at,
               last_verified_status=excluded.last_verified_status,
               last_verified_notes=excluded.last_verified_notes,
               verification_policy_days=excluded.verification_policy_days,
               updated_at=excluded.updated_at",
            crate::core::db::params![self.todo_id, artifacts_json, ts, proof_plan],
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
                    "proof_plan": serde_json::from_str::<serde_json::Value>(proof_plan).map_err(|e| error::DecapodError::ValidationError(e.to_string()))?,
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

fn prepare_verified_completion_for_cli(
    store: &Store,
    repo_root: &Path,
    todo_id: &str,
    artifact_paths: Vec<String>,
    emit_json_failure: bool,
) -> Result<PreparedTodoVerification, error::DecapodError> {
    crate::core::validation_runtime::require_full_readiness_environment()?;
    let mut failed_outcome: Option<ValidationRunOutcome> = None;
    let res = prepare_verified_completion_with(store, repo_root, todo_id, artifact_paths, || {
        let outcome = run_validate_and_hash_detailed(&store.root, repo_root, Some(todo_id))?;
        let summary = (outcome.passed, outcome.output_hash.clone());
        if !outcome.passed {
            failed_outcome = Some(outcome);
        }
        Ok(summary)
    });
    if let Err(err) = res {
        if emit_json_failure && let Some(outcome) = failed_outcome.as_ref() {
            emit_recovery_failure_json("regen", Some(todo_id), outcome);
        }
        return Err(err);
    }
    res
}

pub(crate) fn prepare_verified_completion(
    store: &Store,
    repo_root: &Path,
    todo_id: &str,
    artifact_paths: Vec<String>,
) -> Result<PreparedTodoVerification, error::DecapodError> {
    prepare_verified_completion_for_cli(store, repo_root, todo_id, artifact_paths, false)
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PreparedDoneHandoff {
    schema_version: String,
    request_id: String,
    todo_id: String,
    expected_revision: i64,
    expected_status: String,
    repo_root: PathBuf,
    artifact_paths: Vec<PathBuf>,
    artifacts: VerificationArtifacts,
    historical_record_id: Option<String>,
    historical_candidate_sha: Option<String>,
    historical_record_digest: Option<String>,
}

fn prepared_done_handoff_path(
    store_root: &Path,
    request_id: &str,
) -> Result<PathBuf, error::DecapodError> {
    if request_id.is_empty()
        || !request_id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
    {
        return Err(error::DecapodError::ValidationError(
            "invalid broker request id for prepared completion handoff".into(),
        ));
    }
    Ok(store_root
        .join("broker-runtime")
        .join(format!("prepared-done-{request_id}.json")))
}

pub(crate) fn write_prepared_done_handoff(
    store: &Store,
    request_id: &str,
    prepared: &PreparedTodoVerification,
) -> Result<PathBuf, error::DecapodError> {
    let path = prepared_done_handoff_path(&store.root, request_id)?;
    if let Some(parent) = path.parent() {
        crate::core::fs_permissions::ensure_storage_dir(parent)
            .map_err(error::DecapodError::IoError)?;
    }
    let payload = PreparedDoneHandoff {
        schema_version: "decapod.prepared-done-handoff.v1".to_string(),
        request_id: request_id.to_string(),
        todo_id: prepared.todo_id.clone(),
        expected_revision: prepared.expected_revision,
        expected_status: prepared.expected_status.clone(),
        repo_root: prepared.repo_root.clone(),
        artifact_paths: prepared.artifact_paths.clone(),
        artifacts: prepared.artifacts.clone(),
        historical_record_id: prepared
            .historical
            .as_ref()
            .map(|proof| proof.record_id().to_string()),
        historical_candidate_sha: prepared
            .historical
            .as_ref()
            .map(|proof| proof.candidate_sha().to_string()),
        historical_record_digest: prepared
            .historical
            .as_ref()
            .map(|proof| proof.record_digest().to_string()),
    };
    let bytes = serde_json::to_vec(&payload)
        .map_err(|err| error::DecapodError::ValidationError(err.to_string()))?;
    use std::io::Write;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&path).map_err(error::DecapodError::IoError)?;
    file.write_all(&bytes)
        .map_err(error::DecapodError::IoError)?;
    file.sync_all().map_err(error::DecapodError::IoError)?;
    Ok(path)
}

pub(crate) fn remove_prepared_done_handoff(store_root: &Path, request_id: &str) {
    if let Ok(path) = prepared_done_handoff_path(store_root, request_id) {
        let _ = std::fs::remove_file(path);
    }
}

pub(crate) fn consume_prepared_done_handoff(
    store: &Store,
    repo_root: &Path,
    todo_id: &str,
    commit: Option<&str>,
    request_id: &str,
) -> Result<Option<PreparedTodoVerification>, error::DecapodError> {
    let path = prepared_done_handoff_path(&store.root, request_id)?;
    if !path.is_file() {
        return Ok(None);
    }
    let bytes = std::fs::read(&path).map_err(error::DecapodError::IoError)?;
    let _ = std::fs::remove_file(&path);
    let handoff: PreparedDoneHandoff = serde_json::from_slice(&bytes)
        .map_err(|err| error::DecapodError::ValidationError(err.to_string()))?;
    if handoff.schema_version != "decapod.prepared-done-handoff.v1"
        || handoff.request_id != request_id
        || handoff.todo_id != todo_id
        || handoff.repo_root != repo_root
    {
        return Err(error::DecapodError::ValidationError(
            "PREPARED_DONE_HANDOFF_MISMATCH: prepared completion handoff does not match this request".into(),
        ));
    }
    let prepared = if let Some(sha) = commit {
        let record_id = handoff.historical_record_id.as_deref().ok_or_else(|| {
            error::DecapodError::ValidationError(
                "PREPARED_DONE_HANDOFF_MISMATCH: missing historical record id".into(),
            )
        })?;
        let candidate_sha = handoff.historical_candidate_sha.as_deref().ok_or_else(|| {
            error::DecapodError::ValidationError(
                "PREPARED_DONE_HANDOFF_MISMATCH: missing historical candidate sha".into(),
            )
        })?;
        let expected_digest = handoff.historical_record_digest.as_deref().ok_or_else(|| {
            error::DecapodError::ValidationError(
                "PREPARED_DONE_HANDOFF_MISMATCH: missing historical record digest".into(),
            )
        })?;
        if candidate_sha != sha {
            return Err(error::DecapodError::ValidationError(
                "PREPARED_DONE_HANDOFF_MISMATCH: prepared historical commit differs".into(),
            ));
        }
        let history = handoff.artifacts.historical_validation.ok_or_else(|| {
            error::DecapodError::ValidationError(
                "PREPARED_DONE_HANDOFF_MISMATCH: missing integration witness".into(),
            )
        })?;
        let proof = validation_history::lookup_record(repo_root, store, sha, record_id)?;
        if proof.record_digest() != expected_digest {
            return Err(error::DecapodError::ValidationError(
                "PREPARED_DONE_HANDOFF_MISMATCH: canonical validation record digest changed".into(),
            ));
        }
        if proof.task_binding().task_id != todo_id {
            return Err(error::DecapodError::ValidationError(
                "VALIDATION_HISTORY_TASK_MISMATCH: commit validation belongs to another task"
                    .into(),
            ));
        }
        if proof.task_binding().assignee != crate::current_agent_id() {
            return Err(error::DecapodError::ValidationError(
                "VALIDATION_HISTORY_OWNER_MISMATCH: historical completion requires the recorded task owner".into(),
            ));
        }
        let targets = load_targets(store, Some(todo_id))?;
        let target = targets
            .into_iter()
            .next()
            .ok_or_else(|| error::DecapodError::NotFound(format!("TODO not found: {todo_id}")))?;
        prepare_historical_completion_from_evidence(
            store,
            repo_root,
            target,
            proof,
            history.integration,
        )?
    } else {
        if handoff.artifacts.historical_validation.is_some()
            || handoff.historical_record_id.is_some()
        {
            return Err(error::DecapodError::ValidationError(
                "PREPARED_DONE_HANDOFF_MISMATCH: unexpected historical evidence".into(),
            ));
        }
        let prepared = PreparedTodoVerification {
            todo_id: handoff.todo_id,
            expected_revision: handoff.expected_revision,
            expected_status: handoff.expected_status,
            repo_root: handoff.repo_root,
            artifact_paths: handoff.artifact_paths,
            artifacts: handoff.artifacts,
            historical: None,
            inspected_target: None,
        };
        prepared.check_material()?;
        prepared
    };
    Ok(Some(prepared))
}

fn missing_artifacts_remediation(store_root: &Path, todo_id: &str, prefix: &str) -> String {
    let store = Store {
        kind: crate::core::store::StoreKind::Repo,
        root: store_root.to_path_buf(),
    };
    if let Some(sha) = validation_history::latest_candidate_for_task(&store, todo_id) {
        format!(
            "{prefix} Remediation: repair proof with `decapod todo done {todo_id} --validated --commit {sha}` (if already integrated into the accepted branch), `decapod qa verify regen {todo_id}`, or `decapod qa verify recover --artifact {todo_id}=<current-file>`."
        )
    } else {
        format!(
            "{prefix} Remediation: capture proof with `decapod qa verify regen {todo_id}` or `decapod qa verify recover --artifact {todo_id}=<current-file>` (or `decapod todo done {todo_id} --validated --commit <sha>` if a historical validation commit exists)."
        )
    }
}

/// Admit a prior full result for the same task and a proven accepted integration.
/// This observes immutable evidence; it never executes current publication gates.
fn ensure_historical_proof_plan(plan: Option<&str>) -> Result<(), error::DecapodError> {
    let Some(raw) = plan else {
        return Ok(());
    };
    let gates: Vec<String> = serde_json::from_str(raw).map_err(|_| {
        error::DecapodError::ValidationError(
            "VALIDATION_HISTORY_TASK_PROOF_UNSUPPORTED: malformed task proof plan".into(),
        )
    })?;
    validation_history::ensure_full_validation_plan(&gates)
}

pub(crate) fn prepare_historical_completion(
    store: &Store,
    repo_root: &Path,
    todo_id: &str,
    candidate_sha: &str,
) -> Result<PreparedTodoVerification, error::DecapodError> {
    crate::core::validation_runtime::require_full_readiness_environment()?;
    let targets = load_targets(store, Some(todo_id))?;
    let target = targets
        .into_iter()
        .next()
        .ok_or_else(|| error::DecapodError::NotFound(format!("TODO not found: {todo_id}")))?;
    ensure_historical_proof_plan(target.proof_plan.as_deref())?;
    if !matches!(target.status.as_str(), "open" | "in_progress" | "done") {
        return Err(error::DecapodError::ValidationError(
            "VALIDATION_HISTORY_TASK_STALE: archived tasks require their existing completion history".into(),
        ));
    }
    let proof = validation_history::lookup(repo_root, store, candidate_sha)?;
    if proof.task_binding().task_id != todo_id {
        return Err(error::DecapodError::ValidationError(
            "VALIDATION_HISTORY_TASK_MISMATCH: commit validation belongs to another task".into(),
        ));
    }
    if proof.task_binding().assignee != crate::current_agent_id() {
        return Err(error::DecapodError::ValidationError(
            "VALIDATION_HISTORY_OWNER_MISMATCH: historical completion requires the recorded task owner".into(),
        ));
    }
    // The remote read is outside the completion transaction. The immutable
    // accepted revision is rechecked locally inside the transaction.
    let integration = proof.verify_integration(repo_root)?;
    prepare_historical_completion_from_evidence(store, repo_root, target, proof, integration)
}

/// The typed proof can only originate from the canonical producer journal.
/// Keeping immutable preparation separate makes the transactional rechecks
/// testable without substituting a network response or invoking validation.
fn prepare_historical_completion_from_evidence(
    store: &Store,
    repo_root: &Path,
    target: VerifyTarget,
    proof: HistoricalValidationProof,
    integration: IntegrationWitness,
) -> Result<PreparedTodoVerification, error::DecapodError> {
    ensure_historical_proof_plan(target.proof_plan.as_deref())?;
    if !matches!(target.status.as_str(), "open" | "in_progress" | "done")
        || proof.task_binding().task_id != target.todo_id
    {
        return Err(error::DecapodError::ValidationError(
            "VALIDATION_HISTORY_TASK_MISMATCH: recorded validation does not bind this completable task".into(),
        ));
    }
    let todo_id = target.todo_id.clone();
    let epoch = proof.validation_epoch().clone();
    let history = HistoricalCompletionEvidence {
        candidate_sha: proof.candidate_sha().to_owned(),
        record_id: proof.record_id().to_owned(),
        record_digest: proof.record_digest().to_owned(),
        integration,
    };
    let prepared = PreparedTodoVerification {
        todo_id: todo_id.clone(),
        expected_revision: target.revision,
        expected_status: target.status.clone(),
        repo_root: repo_root.to_path_buf(),
        artifact_paths: Vec::new(),
        artifacts: VerificationArtifacts {
            completed_at: now_iso(),
            task_id: Some(todo_id),
            task_revision: Some(target.revision),
            evaluator_epoch: Some(epoch.epoch_id.clone()),
            validation_epoch: Some(epoch.clone()),
            proof_plan_results: vec![ProofPlanResult {
                proof_gate: "validated_commit".into(),
                status: "pass".into(),
                command: format!("decapod qa verify commit {}", proof.candidate_sha()),
                output_hash: proof.record_digest().to_owned(),
                evaluator_epoch: Some(epoch.epoch_id.clone()),
                validation_epoch: Some(epoch),
            }],
            file_artifacts: Vec::new(),
            historical_validation: Some(history),
        },
        historical: Some(proof),
        inspected_target: Some(target),
    };
    DbBroker::new(&store.root).with_conn(
        &todo::todo_db_path(&store.root),
        "decapod",
        None,
        "verify.history.prepare",
        |conn| prepared.check_task_binding_on_connection(conn),
    )?;
    prepared.check_material()?;
    Ok(prepared)
}

/// Repair a completed task's proof without reopening it or changing completed_at.
pub(crate) fn complete_historical_done_task(
    store: &Store,
    prepared: &PreparedTodoVerification,
) -> Result<(), error::DecapodError> {
    if prepared.historical.is_none() {
        return Err(error::DecapodError::ValidationError(
            "Historical completion requires recorded full validation".into(),
        ));
    }
    todo::with_task_journal(
        &DbBroker::new(&store.root),
        &todo::todo_db_path(&store.root),
        "decapod",
        None,
        "verify.history.complete",
        todo::TaskJournalSelection::Ids(vec![prepared.todo_id.clone()]),
        |conn| {
            if todo::dependency_completion_gate_on_conn(conn, &store.root, &prepared.todo_id)?
                .is_some()
            {
                return Err(error::DecapodError::ValidationError(
                    "VALIDATION_HISTORY_DEPENDENCIES_BLOCKED: current task dependencies must be complete".into(),
                ));
            }
            prepared.commit_recapture_on_connection(conn)
        },
    )
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
                historical_validation: None,
            },
            historical: None,
            inspected_target: None,
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
    capture_baseline_for_todo_impl(store, repo_root, todo_id, artifact_paths, false)
}

fn capture_baseline_for_todo_impl(
    store: &Store,
    repo_root: &Path,
    todo_id: &str,
    artifact_paths: Vec<String>,
    emit_json_failure: bool,
) -> Result<(), error::DecapodError> {
    todo::initialize_todo_db(&store.root)?;
    let task = todo::get_task(&store.root, todo_id)?
        .ok_or_else(|| error::DecapodError::NotFound(format!("TODO not found: {todo_id}")))?;
    if task.status != "done" {
        return Err(error::DecapodError::ValidationError(
            "Task must be in done state before capturing verification artifacts".to_string(),
        ));
    }
    let _verifying = VerifyingTodosGuard::enter(todo_id);
    let prepared = prepare_verified_completion_for_cli(
        store,
        repo_root,
        todo_id,
        artifact_paths,
        emit_json_failure,
    )?;
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

/// Explicitly selected current material for recovery, captured before validation.
/// It is not proof until the real validation snapshot and final batch checks pass.
#[derive(Debug)]
struct RecoveryCapture {
    files: Vec<FileArtifact>,
    paths: Vec<PathBuf>,
    epoch: ValidationEpochMetadata,
}

impl RecoveryCapture {
    fn check_material(&self, repo_root: &Path) -> Result<(), error::DecapodError> {
        if active_validation_epoch(repo_root)? != self.epoch {
            return Err(error::DecapodError::ValidationError(
                "Recovery validation material changed; recapture current evidence".into(),
            ));
        }
        for (path, expected) in self.paths.iter().zip(&self.files) {
            let (hash, size, _) = hash_file(path)?;
            if hash != expected.hash || size != expected.size {
                return Err(error::DecapodError::ValidationError(format!(
                    "Recovery artifact changed during validation: {}",
                    path.display()
                )));
            }
        }
        Ok(())
    }
}

fn prepare_recovery_captures(
    repo_root: &Path,
    targets: &[VerifyTarget],
    inputs: &[String],
) -> Result<std::collections::BTreeMap<String, RecoveryCapture>, error::DecapodError> {
    let mut captures = std::collections::BTreeMap::<String, RecoveryCapture>::new();
    for input in inputs {
        let (id, path) = input
            .split_once('=')
            .filter(|(id, path)| !id.is_empty() && !path.is_empty())
            .ok_or_else(|| {
                error::DecapodError::ValidationError(
                    "Recovery --artifact requires TODO_ID=FILE".into(),
                )
            })?;
        let target = targets
            .iter()
            .find(|target| target.todo_id == id)
            .ok_or_else(|| {
                error::DecapodError::ValidationError(format!(
                    "Recovery artifact names a task outside the unverified completed batch: {id}"
                ))
            })?;
        // Capture may establish the existing validate_passes contract, but must
        // not replace a different or malformed task proof contract.
        if let Some(raw) = target.proof_plan.as_deref() {
            let plan: Vec<String> = serde_json::from_str(raw).map_err(|_| {
                error::DecapodError::ValidationError(format!("TODO {id} has malformed proof_plan"))
            })?;
            if plan.iter().any(|gate| gate != "validate_passes") {
                return Err(error::DecapodError::ValidationError(format!(
                    "TODO {id} has additional proof obligations; recovery cannot replace them"
                )));
            }
        }
        let disk_path = resolve_artifact_path_for_todo(repo_root, Some(id), path);
        let (hash, size, mtime) = hash_file(&disk_path)?;
        let capture = captures.entry(id.to_owned()).or_insert(RecoveryCapture {
            files: Vec::new(),
            paths: Vec::new(),
            epoch: active_validation_epoch(repo_root)?,
        });
        if !capture.paths.contains(&disk_path) {
            capture.paths.push(disk_path);
            capture.files.push(FileArtifact {
                path: path.to_owned(),
                hash,
                size,
                mtime,
            });
        }
    }
    // Discover all missing inputs before executing validation or writing state.
    for target in targets {
        if !captures.contains_key(&target.todo_id) {
            let usable = target
                .artifacts
                .as_deref()
                .and_then(|raw| serde_json::from_str::<VerificationArtifacts>(raw).ok())
                .is_some_and(|artifacts| !artifacts.file_artifacts.is_empty());
            if !usable {
                return Err(error::DecapodError::ValidationError(format!(
                    "TODO {} requires fresh evidence: rerun `decapod qa verify recover --artifact {}=FILE` with a current relevant file for each incomplete baseline; historical completion remains unverified",
                    target.todo_id, target.todo_id
                )));
            }
        }
    }
    Ok(captures)
}

fn refresh_batch_baselines(
    store: &Store,
    targets: &[VerifyTarget],
    snapshot: &ValidationSnapshot,
    validation_epoch: &ValidationEpochMetadata,
) -> Result<Vec<VerifyTarget>, error::DecapodError> {
    refresh_batch_baselines_with_captures(store, targets, snapshot, validation_epoch, None)
}

fn refresh_batch_baselines_with_captures(
    store: &Store,
    targets: &[VerifyTarget],
    snapshot: &ValidationSnapshot,
    validation_epoch: &ValidationEpochMetadata,
    fresh: Option<(&Path, &std::collections::BTreeMap<String, RecoveryCapture>)>,
) -> Result<Vec<VerifyTarget>, error::DecapodError> {
    if !snapshot.passed {
        return Err(error::DecapodError::ValidationError(
            "cannot recover verification baselines from a failing validation snapshot. Agent: run `decapod validate --format json` to inspect the current validation blockers, repair the reported gates, then retry `decapod qa verify recover` with the same explicit --artifact TODO_ID=FILE inputs where required. Historical completion remains unverified; do not prune or reopen tasks to bypass proof".to_string(),
        ));
    }

    let completed_at = now_iso();
    let mut updates = Vec::new();
    for target in targets {
        let capture = fresh.and_then(|(_, captures)| captures.get(&target.todo_id));
        let (proof_plan, mut artifacts) = if let Some(capture) = capture {
            capture.check_material(fresh.unwrap().0)?;
            if &capture.epoch != validation_epoch {
                return Err(error::DecapodError::ValidationError(
                    "Recovery epoch changed after evidence capture".into(),
                ));
            }
            (
                vec!["validate_passes".to_string()],
                VerificationArtifacts {
                    completed_at: completed_at.clone(),
                    task_id: Some(target.todo_id.clone()),
                    task_revision: Some(target.revision),
                    evaluator_epoch: Some(validation_epoch.epoch_id.clone()),
                    validation_epoch: Some(validation_epoch.clone()),
                    file_artifacts: capture.files.clone(),
                    historical_validation: None,
                    proof_plan_results: vec![ProofPlanResult {
                        proof_gate: "validate_passes".into(),
                        status: "pass".into(),
                        command: "decapod validate --format json".into(),
                        output_hash: snapshot.output_hash.clone(),
                        evaluator_epoch: Some(validation_epoch.epoch_id.clone()),
                        validation_epoch: Some(validation_epoch.clone()),
                    }],
                },
            )
        } else {
            let proof_plan_raw = target.proof_plan.as_deref().ok_or_else(|| {
                error::DecapodError::ValidationError(format!(
                    "TODO {} has no proof plan; supply explicit recovery --artifact {}=FILE",
                    target.todo_id, target.todo_id
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
                    "TODO {} has no verification artifacts; supply explicit recovery --artifact {}=FILE",
                    target.todo_id, target.todo_id
                ))
            })?;
            let artifacts: VerificationArtifacts =
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
            (proof_plan, artifacts)
        };

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
                if let Some((repo_root, captures)) = fresh
                    && let Some(capture) = captures.get(&target.todo_id)
                {
                    capture.check_material(repo_root)?;
                }
                if target.status != "done" || target.revision < 0 {
                    return Err(error::DecapodError::ValidationError(
                        "Recovery requires a current completed task revision".into(),
                    ));
                }
            }
            for (target, artifacts_json, proof_plan_json, artifacts) in &updates {
                crate::core::storage_contracts::validate_verification_write(
                    &target.todo_id,
                    proof_plan_json,
                    Some(artifacts_json),
                    Some("CLAIMED"),
                    Some(
                        "current recovery evidence captured; awaiting final aggregate verification",
                    ),
                )?;
                conn.execute(
                "INSERT INTO task_verification(todo_id, proof_plan, verification_artifacts,
                     last_verified_status, last_verified_notes, verification_policy_days, updated_at)
                 VALUES(?4, ?1, ?2, 'CLAIMED',
                     'current recovery evidence captured; awaiting final aggregate verification', 90, ?3)
                 ON CONFLICT(todo_id) DO UPDATE SET
                     proof_plan = excluded.proof_plan,
                     verification_artifacts = excluded.verification_artifacts,
                     last_verified_status = excluded.last_verified_status,
                     last_verified_notes = excluded.last_verified_notes,
                     updated_at = excluded.updated_at",
                crate::core::db::params![proof_plan_json, artifacts_json, completed_at, target.todo_id],
            )?;
                append_verification_event_on_connection(
                    conn,
                    &target.todo_id,
                    "task.verify.recover",
                    &completed_at,
                    serde_json::json!({
                        "previous_proof_plan": target.proof_plan,
                        "previous_verification_artifacts": target.artifacts,
                        "capture_kind": if fresh.is_some_and(|(_, captures)| captures.contains_key(&target.todo_id)) { "fresh_current_material" } else { "existing_material_revalidation" },
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
            "Final aggregate validation did not pass; recovered proof remains pending. Agent: run `decapod validate --format json` to inspect the current validation blockers, repair the reported gates, then retry `decapod qa verify recover`. If evidence must be recaptured, repeat --artifact TODO_ID=FILE for each affected task; pending proof is not acceptance".into(),
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

fn run_recover_cli(
    store: &Store,
    repo_root: &Path,
    json: bool,
    artifact_inputs: &[String],
) -> Result<(), error::DecapodError> {
    let targets = load_targets(store, None)?;
    if targets.is_empty() {
        return Err(error::DecapodError::ValidationError(
            "no done TODOs are available for proof recovery".to_string(),
        ));
    }

    let captures = prepare_recovery_captures(repo_root, &targets, artifact_inputs)?;
    let target_ids = targets
        .iter()
        .map(|target| target.todo_id.as_str())
        .collect::<Vec<_>>()
        .join(",");
    let _verifying = VerifyingTodosGuard::enter(&target_ids);

    let initial = run_validate_and_hash_detailed(&store.root, repo_root, None)?;
    let snapshot = ValidationSnapshot {
        passed: initial.passed,
        output_hash: initial.output_hash.clone(),
    };
    let validation_epoch = active_validation_epoch(repo_root)?;
    let refresh_res = if captures.is_empty() {
        refresh_batch_baselines(store, &targets, &snapshot, &validation_epoch)
    } else {
        refresh_batch_baselines_with_captures(
            store,
            &targets,
            &snapshot,
            &validation_epoch,
            Some((repo_root, &captures)),
        )
    };
    let recovered_targets = match refresh_res {
        Ok(recovered) => recovered,
        Err(err) => {
            if json && !initial.passed {
                emit_recovery_failure_json("initial", None, &initial);
            }
            return Err(err);
        }
    };

    // These exact tasks remain CLAIMED while their own aggregate admission
    // check is scoped out. All other final validation gates still execute.
    // Finalization rechecks every task/artifact and commits all passes at once.
    let final_run = run_validate_and_hash_detailed(&store.root, repo_root, None)?;
    let results = match finalize_recovered_batch(
        store,
        repo_root,
        &recovered_targets,
        &ValidationSnapshot {
            passed: final_run.passed,
            output_hash: final_run.output_hash.clone(),
        },
    ) {
        Ok(results) => results,
        Err(err) => {
            if json && !final_run.passed {
                emit_recovery_failure_json("final", None, &final_run);
            }
            return Err(err);
        }
    };

    if json {
        println!(
            "{}",
            serde_json::json!({
                "status": "ok",
                "recovered": results.len(),
                "validation_epoch": validation_epoch.epoch_id,
                "snapshot_output_hash": snapshot.output_hash,
                "final_output_hash": final_run.output_hash
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
            VerifyCommand::Commit { sha } => {
                if cli.stale {
                    return Err(error::DecapodError::ValidationError(
                        "--stale does not apply to immutable commit validation lookup".into(),
                    ));
                }
                let proof = validation_history::lookup(repo_root, store, sha)?;
                println!(
                    "{}",
                    serde_json::to_string_pretty(&serde_json::json!({
                        "schema": "decapod.validation-history.lookup.v1",
                        "scope": "historical_validation",
                        "status": "passed",
                        "publication_ready": false,
                        "candidate_sha": proof.candidate_sha(),
                        "accepted_base": proof.accepted_base(),
                        "task": proof.task_binding(),
                        "record_id": proof.record_id(),
                        "record_digest": proof.record_digest(),
                        "validation_epoch": proof.validation_epoch(),
                    }))
                    .map_err(|error| error::DecapodError::ValidationError(error.to_string()))?
                );
                return Ok(());
            }
            VerifyCommand::Regen { id, artifact } => {
                capture_baseline_for_todo_impl(store, repo_root, id, artifact.clone(), cli.json)?;
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
            VerifyCommand::Recover { artifact } => {
                if cli.stale {
                    // --stale lists stale items only and must not run
                    // verification, even for the recover command (#1527).
                    let (checked_at, stale_items) = collect_stale_items(store)?;
                    print_stale_listing(cli.json, &checked_at, &stale_items);
                    return Ok(());
                }
                return run_recover_cli(store, repo_root, cli.json, artifact);
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
        let stale_items = stale_items_from_targets(&targets, &now);
        print_stale_listing(cli.json, &now, &stale_items);
        return Ok(());
    }

    let run_id = crate::core::ulid::new_ulid();
    let mut results = Vec::new();

    let target_ids: Vec<String> = targets.iter().map(|t| t.todo_id.clone()).collect();
    let target_ids_str = target_ids.join(",");
    let _verifying = VerifyingTodosGuard::enter(&target_ids_str);

    let needs_current_validation = targets.iter().any(requires_current_validation);
    let validation_snapshot = if single_id.is_none() && needs_current_validation {
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
                if let Some(cleanup) = task["cleanup_guidance"].as_str()
                    && !cleanup.is_empty()
                {
                    println!("  cleanup: {cleanup}");
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

    let cleanup_guidance = (next_command == "decapod workspace publish")
        .then(|| crate::core::validate::todo_tmp_cleanup_instruction_for_task(&task.id, None));

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
        "cleanup_guidance": cleanup_guidance,
    })
}
#[cfg(test)]
#[path = "../../../tests/unit/plugins/verify_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "../../../tests/unit/plugins/verify_completion_tests.rs"]
mod completion_tests;

#[cfg(test)]
#[path = "../../../tests/unit/plugins/verify_historical_tests.rs"]
mod historical_tests;
