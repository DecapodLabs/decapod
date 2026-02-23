use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};

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
