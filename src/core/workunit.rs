use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

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
