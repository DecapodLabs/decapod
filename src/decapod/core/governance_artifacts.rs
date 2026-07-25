//! Agent-facing inventory and repair for required publication artifacts.
//!
//! This surface deliberately distinguishes the repository research claims
//! ledger from Health Engine claims stored in `health.db`.

use crate::core::{research_claims, trajectory, validate};
use crate::plan_governance;
use serde::Serialize;
use std::collections::BTreeSet;
use std::path::Path;
use std::process::Command;

pub const INVENTORY_SCHEMA_VERSION: &str = "1.0.0";
pub const INVENTORY_COMMAND: &str = "decapod govern artifacts inventory";

#[derive(Debug, Clone, Serialize)]
pub struct GovernanceArtifactEntry {
    pub path: String,
    pub role: String,
    pub present: bool,
    pub valid: bool,
    pub staged: bool,
    pub in_pr_diff: bool,
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
    pub claims_source: String,
    pub repair_command: String,
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

    let mut artifacts = vec![
        entry(
            repo_root,
            plan_governance::PLAN_PATH,
            "governed intent and phase plan",
            plan_governance::load_plan(repo_root)?.is_some(),
            &staged_paths,
            &pr_paths,
        ),
        entry(
            repo_root,
            research_claims::CLAIMS_PATH,
            "repository research claims ledger; distinct from Health Engine claims in health.db",
            research_claims::load_and_validate(repo_root)?.is_some(),
            &staged_paths,
            &pr_paths,
        ),
        entry(
            repo_root,
            trajectory::TRAJECTORY_PATH,
            "agent-run trajectory cookie and proof evidence",
            trajectory::load_trajectory_cookie(repo_root)?.is_some(),
            &staged_paths,
            &pr_paths,
        ),
        entry(
            repo_root,
            validate::VALIDATION_RECEIPT_PATH,
            "successful Decapod validation receipt",
            validation_receipt_is_valid(repo_root)?,
            &staged_paths,
            &pr_paths,
        ),
    ];
    artifacts.sort_by(|left, right| left.path.cmp(&right.path));

    let all_present = artifacts.iter().all(|item| item.present);
    let all_valid = artifacts.iter().all(|item| item.valid);
    let all_staged = artifacts.iter().all(|item| item.staged);
    let all_in_pr_diff = base_ref.is_some() && artifacts.iter().all(|item| item.in_pr_diff);
    Ok(GovernanceArtifactInventory {
        schema_version: INVENTORY_SCHEMA_VERSION.to_string(),
        kind: "governance_artifact_inventory".to_string(),
        base_ref,
        artifacts,
        all_present,
        all_valid,
        all_staged,
        all_in_pr_diff,
        claims_source: "repository research ledger at .decapod/governance/claims.json; Health Engine claims remain in .decapod/data/health.db".to_string(),
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
    if !report.all_present || !report.all_valid || !report.all_in_pr_diff {
        return Err(crate::core::error::DecapodError::ValidationError(format!(
            "governance artifact inventory is incomplete; run `{INVENTORY_COMMAND} --repair`, then stage all four artifacts and rerun with `--base-branch <branch>`."
        )));
    }
    Ok(())
}

fn entry(
    repo_root: &Path,
    path: &str,
    role: &str,
    valid: bool,
    staged_paths: &BTreeSet<String>,
    pr_paths: &BTreeSet<String>,
) -> GovernanceArtifactEntry {
    let present = repo_root.join(path).is_file();
    GovernanceArtifactEntry {
        path: path.to_string(),
        role: role.to_string(),
        present,
        valid: present && valid,
        staged: staged_paths.contains(path),
        in_pr_diff: pr_paths.contains(path),
        remediation: if path == research_claims::CLAIMS_PATH {
            format!("Run `{INVENTORY_COMMAND} --repair`; existing claims content is preserved.")
        } else {
            format!("Create or refresh `{path}` through the governed workflow.")
        },
    }
}

fn validation_receipt_is_valid(repo_root: &Path) -> Result<bool, crate::core::error::DecapodError> {
    let path = repo_root.join(validate::VALIDATION_RECEIPT_PATH);
    if !path.is_file() {
        return Ok(false);
    }
    let raw = std::fs::read_to_string(path).map_err(crate::core::error::DecapodError::IoError)?;
    let receipt: validate::ValidationReceipt = serde_json::from_str(&raw).map_err(|error| {
        crate::core::error::DecapodError::ValidationError(format!(
            "invalid validation receipt: {error}"
        ))
    })?;
    Ok(receipt.validate_integrity().is_ok())
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
