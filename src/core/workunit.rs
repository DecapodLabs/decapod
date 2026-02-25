use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};

use crate::core::context_capsule::DeterministicContextCapsule;
use crate::core::error;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum WorkUnitStatus {
    Draft,
    Executing,
    Claimed,
    Verified,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct WorkUnitProofResult {
    pub gate: String,
    pub status: String,
    pub artifact_ref: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkUnitManifest {
    pub task_id: String,
    pub intent_ref: String,
    pub spec_refs: Vec<String>,
    pub state_refs: Vec<String>,
    pub proof_plan: Vec<String>,
    pub proof_results: Vec<WorkUnitProofResult>,
    pub status: WorkUnitStatus,
}

impl WorkUnitManifest {
    pub fn canonicalized(&self) -> Self {
        let mut out = self.clone();

        out.spec_refs.sort();
        out.spec_refs.dedup();

        out.state_refs.sort();
        out.state_refs.dedup();

        out.proof_plan.sort();
        out.proof_plan.dedup();

        out.proof_results.sort();

        out
    }

    pub fn canonical_json_bytes(&self) -> Result<Vec<u8>, serde_json::Error> {
        serde_json::to_vec(&self.canonicalized())
    }

    pub fn canonical_hash_hex(&self) -> Result<String, serde_json::Error> {
        let bytes = self.canonical_json_bytes()?;
        let mut hasher = Sha256::new();
        hasher.update(&bytes);
        Ok(format!("{:x}", hasher.finalize()))
    }
}

pub fn workunits_dir(project_root: &Path) -> PathBuf {
    project_root
        .join(".decapod")
        .join("governance")
        .join("workunits")
}

pub fn validate_task_id(task_id: &str) -> Result<(), error::DecapodError> {
    if task_id.is_empty() {
        return Err(error::DecapodError::ValidationError(
            "task_id cannot be empty".to_string(),
        ));
    }
    if task_id
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        Ok(())
    } else {
        Err(error::DecapodError::ValidationError(format!(
            "invalid task_id '{}': allowed characters are [A-Za-z0-9_-]",
            task_id
        )))
    }
}

pub fn workunit_path(project_root: &Path, task_id: &str) -> Result<PathBuf, error::DecapodError> {
    validate_task_id(task_id)?;
    Ok(workunits_dir(project_root).join(format!("{task_id}.json")))
}

pub fn init_workunit(
    project_root: &Path,
    task_id: &str,
    intent_ref: &str,
) -> Result<WorkUnitManifest, error::DecapodError> {
    let path = workunit_path(project_root, task_id)?;
    if path.exists() {
        return Err(error::DecapodError::ValidationError(format!(
            "workunit '{}' already exists",
            task_id
        )));
    }

    let manifest = WorkUnitManifest {
        task_id: task_id.to_string(),
        intent_ref: intent_ref.to_string(),
        spec_refs: Vec::new(),
        state_refs: Vec::new(),
        proof_plan: Vec::new(),
        proof_results: Vec::new(),
        status: WorkUnitStatus::Draft,
    };
    write_workunit(project_root, &manifest)?;
    Ok(manifest)
}

pub fn load_workunit(
    project_root: &Path,
    task_id: &str,
) -> Result<WorkUnitManifest, error::DecapodError> {
    let path = workunit_path(project_root, task_id)?;
    if !path.exists() {
        return Err(error::DecapodError::NotFound(format!(
            "workunit '{}' not found at {}",
            task_id,
            path.display()
        )));
    }
    let raw = fs::read_to_string(&path).map_err(error::DecapodError::IoError)?;
    serde_json::from_str(&raw).map_err(|e| {
        error::DecapodError::ValidationError(format!(
            "invalid workunit manifest {}: {}",
            path.display(),
            e
        ))
    })
}

pub fn write_workunit(
    project_root: &Path,
    manifest: &WorkUnitManifest,
) -> Result<PathBuf, error::DecapodError> {
    let path = workunit_path(project_root, &manifest.task_id)?;
    let parent = path.parent().ok_or_else(|| {
        error::DecapodError::ValidationError("invalid workunit parent path".to_string())
    })?;
    fs::create_dir_all(parent).map_err(error::DecapodError::IoError)?;

    let bytes = serde_json::to_vec_pretty(&manifest.canonicalized()).map_err(|e| {
        error::DecapodError::ValidationError(format!("failed to serialize workunit manifest: {e}"))
    })?;
    fs::write(&path, bytes).map_err(error::DecapodError::IoError)?;
    Ok(path)
}

pub fn add_spec_ref(
    project_root: &Path,
    task_id: &str,
    spec_ref: &str,
) -> Result<WorkUnitManifest, error::DecapodError> {
    let mut manifest = load_workunit(project_root, task_id)?;
    manifest.spec_refs.push(spec_ref.to_string());
    write_workunit(project_root, &manifest)?;
    load_workunit(project_root, task_id)
}

pub fn add_state_ref(
    project_root: &Path,
    task_id: &str,
    state_ref: &str,
) -> Result<WorkUnitManifest, error::DecapodError> {
    let mut manifest = load_workunit(project_root, task_id)?;
    manifest.state_refs.push(state_ref.to_string());
    write_workunit(project_root, &manifest)?;
    load_workunit(project_root, task_id)
}

pub fn set_proof_plan(
    project_root: &Path,
    task_id: &str,
    gates: &[String],
) -> Result<WorkUnitManifest, error::DecapodError> {
    let mut manifest = load_workunit(project_root, task_id)?;
    manifest.proof_plan = gates.to_vec();
    write_workunit(project_root, &manifest)?;
    load_workunit(project_root, task_id)
}

pub fn record_proof_result(
    project_root: &Path,
    task_id: &str,
    gate: &str,
    status: &str,
    artifact_ref: Option<String>,
) -> Result<WorkUnitManifest, error::DecapodError> {
    if !matches!(status, "pass" | "fail") {
        return Err(error::DecapodError::ValidationError(format!(
            "invalid proof status '{}': expected pass|fail",
            status
        )));
    }

    let mut manifest = load_workunit(project_root, task_id)?;
    manifest.proof_results.retain(|r| r.gate != gate);
    manifest.proof_results.push(WorkUnitProofResult {
        gate: gate.to_string(),
        status: status.to_string(),
        artifact_ref,
    });
    write_workunit(project_root, &manifest)?;
    load_workunit(project_root, task_id)
}

pub fn transition_status(
    project_root: &Path,
    task_id: &str,
    to: WorkUnitStatus,
) -> Result<WorkUnitManifest, error::DecapodError> {
    let mut manifest = load_workunit(project_root, task_id)?;
    let from = manifest.status.clone();
    if !can_transition(&from, &to) {
        return Err(error::DecapodError::ValidationError(format!(
            "invalid workunit transition: {:?} -> {:?}",
            from, to
        )));
    }

    if to == WorkUnitStatus::Verified {
        ensure_verified_ready(&manifest)?;
    }

    manifest.status = to;
    write_workunit(project_root, &manifest)?;
    load_workunit(project_root, task_id)
}

pub fn validate_verified_manifest(manifest: &WorkUnitManifest) -> Result<(), error::DecapodError> {
    ensure_verified_ready(manifest)
}

fn can_transition(from: &WorkUnitStatus, to: &WorkUnitStatus) -> bool {
    use WorkUnitStatus::*;
    matches!(
        (from, to),
        (Draft, Executing)
            | (Executing, Claimed)
            | (Claimed, Verified)
            | (Executing, Draft)
            | (Draft, Draft)
            | (Executing, Executing)
            | (Claimed, Claimed)
            | (Verified, Verified)
    )
}

fn ensure_verified_ready(manifest: &WorkUnitManifest) -> Result<(), error::DecapodError> {
    if manifest.proof_plan.is_empty() {
        return Err(error::DecapodError::ValidationError(
            "cannot transition to VERIFIED without proof_plan gates".to_string(),
        ));
    }

    for gate in &manifest.proof_plan {
        let hit = manifest
            .proof_results
            .iter()
            .any(|r| &r.gate == gate && r.status == "pass");
        if !hit {
            return Err(error::DecapodError::ValidationError(format!(
                "cannot transition to VERIFIED: missing passing proof result for gate '{}'",
                gate
            )));
        }
    }

    Ok(())
}

pub fn verify_capsule_policy_lineage_for_task(
    project_root: &Path,
    manifest: &WorkUnitManifest,
) -> Result<(), error::DecapodError> {
    let task_id = manifest.task_id.as_str();
    let expected_rel = format!(".decapod/generated/context/{task_id}.json");
    let expected_abs = project_root
        .join(&expected_rel)
        .to_string_lossy()
        .to_string()
        .replace('\\', "/");
    let expected_rel_norm = expected_rel.replace('\\', "/");
    let has_capsule_state_ref = manifest.state_refs.iter().any(|state_ref| {
        let normalized = state_ref.replace('\\', "/");
        normalized == expected_rel_norm
            || normalized == expected_abs
            || normalized.ends_with(&format!("/{expected_rel_norm}"))
    });
    if !has_capsule_state_ref {
        return Err(error::DecapodError::ValidationError(format!(
            "WORKUNIT_CAPSULE_POLICY_LINEAGE_STATE_REF_MISSING: expected state_ref '{}' for task '{}'",
            expected_rel, task_id
        )));
    }

    let capsule_path = project_root
        .join(".decapod")
        .join("generated")
        .join("context")
        .join(format!("{task_id}.json"));
    if !capsule_path.exists() {
        return Err(error::DecapodError::ValidationError(format!(
            "WORKUNIT_CAPSULE_POLICY_LINEAGE_MISSING: expected context capsule for task '{}' at {}",
            task_id,
            capsule_path.display()
        )));
    }

    let raw = fs::read_to_string(&capsule_path).map_err(error::DecapodError::IoError)?;
    let capsule: DeterministicContextCapsule = serde_json::from_str(&raw).map_err(|e| {
        error::DecapodError::ValidationError(format!(
            "WORKUNIT_CAPSULE_POLICY_LINEAGE_INVALID: invalid capsule JSON at {}: {}",
            capsule_path.display(),
            e
        ))
    })?;
    let expected_hash = capsule.computed_hash_hex().map_err(|e| {
        error::DecapodError::ValidationError(format!(
            "WORKUNIT_CAPSULE_POLICY_LINEAGE_INVALID: hash compute failed at {}: {}",
            capsule_path.display(),
            e
        ))
    })?;
    if capsule.capsule_hash != expected_hash {
        return Err(error::DecapodError::ValidationError(format!(
            "WORKUNIT_CAPSULE_POLICY_LINEAGE_INVALID: capsule hash mismatch at {}",
            capsule_path.display()
        )));
    }

    let task_match = capsule.task_id.as_deref() == Some(task_id)
        || capsule.workunit_id.as_deref() == Some(task_id);
    if !task_match {
        return Err(error::DecapodError::ValidationError(format!(
            "WORKUNIT_CAPSULE_POLICY_LINEAGE_INVALID: capsule task/workunit binding mismatch for task '{}'",
            task_id
        )));
    }

    let policy = capsule.policy;
    if policy.risk_tier.trim().is_empty()
        || policy.policy_hash.trim().is_empty()
        || policy.policy_version.trim().is_empty()
        || policy.policy_path.trim().is_empty()
        || policy.repo_revision.trim().is_empty()
    {
        return Err(error::DecapodError::ValidationError(format!(
            "WORKUNIT_CAPSULE_POLICY_LINEAGE_INVALID: missing policy lineage fields in {}",
            capsule_path.display()
        )));
    }

    Ok(())
}
