//! Agent-facing inventory and integrity report for required publication artifacts.
//!
//! This is the authoritative read-only substrate consumed by later readiness
//! diagnostics. It deliberately keeps repository research claims separate from
//! Health Engine claims stored in `health.db`.

use crate::core::{dirty_classification, research_claims, trajectory, validate};
use crate::plan_governance;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::path::Path;
use std::process::Command;

pub const INVENTORY_SCHEMA_VERSION: &str = "1.1.0";
pub const INVENTORY_COMMAND: &str = "decapod govern artifacts inventory";

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceTargetState {
    Identical,
    WorkspaceOnly,
    TargetOnly,
    Divergent,
    Missing,
    Unknown,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SemanticFreshness {
    Current,
    Stale,
    Invalid,
    Unknown,
}

#[derive(Debug, Clone, Serialize)]
pub struct GovernanceArtifactEntry {
    pub path: String,
    pub role: String,
    pub present: bool,
    pub valid: bool,
    pub staged: bool,
    pub in_pr_diff: bool,
    pub workspace_sha: Option<String>,
    pub target_sha: Option<String>,
    pub workspace_target_state: WorkspaceTargetState,
    pub semantic_freshness: SemanticFreshness,
    pub freshness_reasons: Vec<String>,
    pub schema_error: Option<String>,
    pub remediation: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct GovernanceArtifactInventory {
    pub schema_version: String,
    pub kind: String,
    pub base_ref: Option<String>,
    pub artifacts: Vec<GovernanceArtifactEntry>,
    pub all_present: bool,
    pub all_valid: bool,
    pub all_staged: bool,
    pub all_in_pr_diff: bool,
    pub all_semantically_current: bool,
    pub workspace_branch: Option<String>,
    pub workspace_sha: Option<String>,
    pub dirty: dirty_classification::DirtyClassification,
    pub claims_source: String,
    pub repair_command: String,
}

struct ArtifactValidity {
    valid: bool,
    semantic_freshness: SemanticFreshness,
    schema_error: Option<String>,
}

pub fn inventory(
    repo_root: &Path,
    base_branch: Option<&str>,
    repair: bool,
) -> Result<GovernanceArtifactInventory, crate::core::error::DecapodError> {
    if repair {
        let _ = research_claims::ensure_template(repo_root, false)?;
    }

    let base_ref = resolve_base_ref(repo_root, base_branch);
    let pr_paths = base_ref
        .as_deref()
        .map(|base| {
            git_paths(
                repo_root,
                &["diff", "--name-only", &format!("{base}...HEAD")],
            )
        })
        .transpose()?
        .unwrap_or_default();
    let staged_paths = git_paths(repo_root, &["diff", "--cached", "--name-only"])?;

    let plan_result = plan_governance::load_plan(repo_root);
    let claims_result = research_claims::load_and_validate(repo_root);
    let trajectory_result = trajectory::load_trajectory_cookie(repo_root);
    let receipt_result = load_validation_receipt(repo_root);
    let plan = plan_result.as_ref().ok().and_then(Option::as_ref);
    let claims = claims_result.as_ref().ok().and_then(Option::as_ref);
    let trajectory = trajectory_result.as_ref().ok().and_then(Option::as_ref);
    let receipt = receipt_result.as_ref().ok().and_then(Option::as_ref);
    let subject_freshness = subject_freshness(repo_root, plan, trajectory);
    let receipt_freshness = receipt_freshness(repo_root, receipt, trajectory);
    let receipt_chain_valid = receipt.is_some_and(|item| {
        item.validate_integrity().is_ok()
            && trajectory.is_some_and(|run| {
                item.trajectory_artifact_hash.as_deref() == Some(run.artifact_hash.as_str())
            })
    });

    let mut artifacts = vec![
        entry(
            repo_root,
            plan_governance::PLAN_PATH,
            "governed intent and phase plan",
            ArtifactValidity {
                valid: plan.is_some(),
                semantic_freshness: subject_freshness,
                schema_error: plan_result.as_ref().err().map(ToString::to_string),
            },
            &staged_paths,
            &pr_paths,
        ),
        entry(
            repo_root,
            research_claims::CLAIMS_PATH,
            "repository research claims ledger; distinct from Health Engine claims in health.db",
            ArtifactValidity {
                valid: claims.is_some(),
                semantic_freshness: if claims.is_some() {
                    SemanticFreshness::Current
                } else {
                    SemanticFreshness::Invalid
                },
                schema_error: claims_result.as_ref().err().map(ToString::to_string),
            },
            &staged_paths,
            &pr_paths,
        ),
        entry(
            repo_root,
            trajectory::TRAJECTORY_PATH,
            "agent-run trajectory cookie and proof evidence",
            ArtifactValidity {
                valid: trajectory.is_some(),
                semantic_freshness: subject_freshness,
                schema_error: trajectory_result.as_ref().err().map(ToString::to_string),
            },
            &staged_paths,
            &pr_paths,
        ),
        entry(
            repo_root,
            validate::VALIDATION_RECEIPT_PATH,
            "successful Decapod validation receipt bound to the current trajectory",
            ArtifactValidity {
                valid: receipt_chain_valid,
                semantic_freshness: receipt_freshness,
                schema_error: receipt_result.as_ref().err().map(ToString::to_string),
            },
            &staged_paths,
            &pr_paths,
        ),
    ];
    artifacts.sort_by(|left, right| left.path.cmp(&right.path));

    let all_present = artifacts.iter().all(|item| item.present);
    let all_valid = artifacts.iter().all(|item| item.valid);
    let all_staged = artifacts.iter().all(|item| item.staged);
    let all_in_pr_diff = base_ref.is_some() && artifacts.iter().all(|item| item.in_pr_diff);
    let all_semantically_current = artifacts
        .iter()
        .all(|item| item.semantic_freshness == SemanticFreshness::Current);
    Ok(GovernanceArtifactInventory {
        schema_version: INVENTORY_SCHEMA_VERSION.to_string(),
        kind: "governance_artifact_inventory".to_string(),
        base_ref,
        artifacts,
        all_present,
        all_valid,
        all_staged,
        all_in_pr_diff,
        all_semantically_current,
        workspace_branch: current_branch(repo_root),
        workspace_sha: current_revision(repo_root),
        dirty: dirty_classification::classify(repo_root, commit_often_limit())
            .map_err(crate::core::error::DecapodError::IoError)?,
        claims_source: ".decapod/governance/claims.json; Health Engine claims remain in .decapod/data/health.db".to_string(),
        repair_command: format!("{INVENTORY_COMMAND} --repair"),
    })
}

pub fn run_inventory(
    repo_root: &Path,
    base_branch: Option<&str>,
    repair: bool,
) -> Result<(), crate::core::error::DecapodError> {
    let report = inventory(repo_root, base_branch, repair)?;
    println!(
        "{}",
        serde_json::to_string_pretty(&report).map_err(|error| {
            crate::core::error::DecapodError::ValidationError(format!(
                "governance artifact inventory serialization failed: {error}"
            ))
        })?
    );
    if !report.all_present
        || !report.all_valid
        || !report.all_in_pr_diff
        || !report.all_semantically_current
    {
        return Err(crate::core::error::DecapodError::ValidationError(format!(
            "governance artifact inventory is incomplete or stale; run `{INVENTORY_COMMAND} --repair`, then stage all four artifacts and rerun with `--base-branch <branch>`."
        )));
    }
    Ok(())
}

fn entry(
    repo_root: &Path,
    path: &str,
    role: &str,
    validity: ArtifactValidity,
    staged_paths: &BTreeSet<String>,
    pr_paths: &BTreeSet<String>,
) -> GovernanceArtifactEntry {
    let present = repo_root.join(path).is_file();
    let workspace_sha = present.then(|| file_sha(&repo_root.join(path)));
    let target_sha = git_blob_sha(repo_root, "HEAD", path);
    let workspace_target_state = match (workspace_sha.as_deref(), target_sha.as_deref()) {
        (Some(workspace), Some(target)) if workspace == target => WorkspaceTargetState::Identical,
        (Some(_), Some(_)) => WorkspaceTargetState::Divergent,
        (Some(_), None) => WorkspaceTargetState::WorkspaceOnly,
        (None, Some(_)) => WorkspaceTargetState::TargetOnly,
        (None, None) => WorkspaceTargetState::Missing,
    };
    let mut freshness_reasons = Vec::new();
    if !validity.valid {
        freshness_reasons.push("schema_or_integrity_invalid".to_string());
    }
    if !staged_paths.contains(path) {
        freshness_reasons.push("not_staged".to_string());
    }
    if !pr_paths.contains(path) {
        freshness_reasons.push("not_in_target_diff".to_string());
    }
    if workspace_target_state != WorkspaceTargetState::Identical {
        freshness_reasons.push("workspace_differs_from_target".to_string());
    }
    if path == validate::VALIDATION_RECEIPT_PATH {
        freshness_reasons.push("receipt_must_bind_current_trajectory".to_string());
    }
    GovernanceArtifactEntry {
        path: path.to_string(),
        role: role.to_string(),
        present,
        valid: present && validity.valid,
        staged: staged_paths.contains(path),
        in_pr_diff: pr_paths.contains(path),
        workspace_sha,
        target_sha,
        workspace_target_state,
        semantic_freshness: if !present {
            SemanticFreshness::Unknown
        } else {
            validity.semantic_freshness
        },
        freshness_reasons,
        schema_error: validity.schema_error,
        remediation: if path == research_claims::CLAIMS_PATH {
            format!("Run `{INVENTORY_COMMAND} --repair`; existing claims content is preserved.")
        } else {
            format!("Create or refresh `{path}` through the governed workflow.")
        },
    }
}

fn load_validation_receipt(
    repo_root: &Path,
) -> Result<Option<validate::ValidationReceipt>, String> {
    let path = repo_root.join(validate::VALIDATION_RECEIPT_PATH);
    if !path.is_file() {
        return Ok(None);
    }
    let raw = std::fs::read_to_string(path).map_err(|error| error.to_string())?;
    serde_json::from_str(&raw)
        .map(Some)
        .map_err(|error| format!("invalid validation receipt: {error}"))
}

fn commit_often_limit() -> usize {
    std::env::var("DECAPOD_COMMIT_OFTEN_MAX_DIRTY_FILES")
        .ok()
        .and_then(|value| value.parse().ok())
        .filter(|value| *value > 0)
        .unwrap_or(6)
}

fn subject_freshness(
    repo_root: &Path,
    plan: Option<&plan_governance::GovernedPlan>,
    trajectory: Option<&trajectory::TrajectoryArtifact>,
) -> SemanticFreshness {
    let (Some(plan), Some(trajectory), Some(branch)) =
        (plan, trajectory, current_branch(repo_root))
    else {
        return SemanticFreshness::Unknown;
    };
    let Some(task_id) = trajectory.task_id.as_deref() else {
        return SemanticFreshness::Unknown;
    };
    if plan.todo_ids.iter().any(|todo| todo == task_id) && branch.contains(task_id) {
        SemanticFreshness::Current
    } else {
        SemanticFreshness::Stale
    }
}

fn receipt_freshness(
    repo_root: &Path,
    receipt: Option<&validate::ValidationReceipt>,
    trajectory: Option<&trajectory::TrajectoryArtifact>,
) -> SemanticFreshness {
    let (Some(receipt), Some(trajectory), Some(head)) =
        (receipt, trajectory, current_revision(repo_root))
    else {
        return SemanticFreshness::Unknown;
    };
    if receipt.validate_integrity().is_err()
        || receipt.trajectory_artifact_hash.as_deref() != Some(trajectory.artifact_hash.as_str())
        || receipt.git_revision != head
    {
        SemanticFreshness::Stale
    } else {
        SemanticFreshness::Current
    }
}

fn current_branch(repo_root: &Path) -> Option<String> {
    git_text(repo_root, &["symbolic-ref", "--quiet", "--short", "HEAD"])
}

fn current_revision(repo_root: &Path) -> Option<String> {
    git_text(repo_root, &["rev-parse", "HEAD"])
}

fn git_text(repo_root: &Path, args: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .current_dir(repo_root)
        .args(args)
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_string())
        .filter(|value| !value.is_empty())
}

fn git_blob_sha(repo_root: &Path, reference: &str, path: &str) -> Option<String> {
    let spec = format!("{reference}:{path}");
    let output = Command::new("git")
        .current_dir(repo_root)
        .args(["rev-parse", &spec])
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_string())
        .filter(|value| !value.is_empty())
}

fn file_sha(path: &Path) -> String {
    match std::fs::read(path) {
        Ok(bytes) => {
            let mut hasher = Sha256::new();
            hasher.update(bytes);
            format!("sha256:{:x}", hasher.finalize())
        }
        Err(_) => "unavailable".to_string(),
    }
}

fn resolve_base_ref(repo_root: &Path, requested: Option<&str>) -> Option<String> {
    if let Some(branch) = requested {
        return git_ref_exists(repo_root, branch).then(|| branch.to_string());
    }
    ["master", "main"]
        .into_iter()
        .find(|candidate| git_ref_exists(repo_root, candidate))
        .map(str::to_string)
}

fn git_ref_exists(repo_root: &Path, reference: &str) -> bool {
    Command::new("git")
        .current_dir(repo_root)
        .args(["rev-parse", "--verify", reference])
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

fn git_paths(
    repo_root: &Path,
    args: &[&str],
) -> Result<BTreeSet<String>, crate::core::error::DecapodError> {
    let output = Command::new("git")
        .current_dir(repo_root)
        .args(args)
        .output()
        .map_err(crate::core::error::DecapodError::IoError)?;
    if !output.status.success() {
        return Err(crate::core::error::DecapodError::ValidationError(format!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .filter(|path| !path.is_empty())
        .map(ToString::to_string)
        .collect())
}
