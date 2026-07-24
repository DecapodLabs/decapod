use crate::core::context_capsule::DeterministicContextCapsule;
use crate::core::error;
use crate::core::validation_epoch::active_validation_epoch;
use crate::core::workunit::{self, WorkUnitStatus};
use crate::plan_governance;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

pub const COMPLETION_EVIDENCE_SCHEMA_VERSION: &str = "1.0.0";
pub const COMPLETION_EVIDENCE_DIR: &str =
    ".decapod/generated/artifacts/provenance/completion_evidence";
pub const IMPORTED_COMPLETION_EVIDENCE_DIR: &str =
    ".decapod/generated/artifacts/provenance/completion_evidence/imports";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct CompletionProofEvidence {
    pub gate: String,
    pub status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifact_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifact: Option<FileDigest>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct FileDigest {
    pub path: String,
    pub sha256: String,
    pub size: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct WorkingTreeEntry {
    pub path: String,
    pub exists: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RepositoryEvidence {
    pub head_revision: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_revision: Option<String>,
    pub changed_paths: Vec<String>,
    pub working_tree: Vec<WorkingTreeEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CapsuleEvidence {
    pub path: String,
    pub capsule_hash: String,
    pub policy_hash: String,
    pub policy_version: String,
    pub repo_revision: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ValidationEpochEvidence {
    pub epoch_id: String,
    pub epoch_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct UnresolvedClaim {
    pub kind: String,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CompletionEvidenceRecord {
    pub schema_version: String,
    pub task_id: String,
    pub workunit_hash: String,
    pub intent_ref: String,
    pub spec_refs: Vec<String>,
    pub state_refs: Vec<String>,
    pub proof_plan: Vec<String>,
    pub proof_results: Vec<CompletionProofEvidence>,
    pub capsule: CapsuleEvidence,
    pub repository: RepositoryEvidence,
    pub validation_epoch: ValidationEpochEvidence,
    #[serde(default)]
    pub unresolved_claims: Vec<UnresolvedClaim>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plan_hash: Option<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub evidence_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SourceRepositoryBinding {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repository_id: Option<String>,
    pub head_revision: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_revision: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PortableCompletionEvidence {
    pub schema_version: String,
    pub source_repository: SourceRepositoryBinding,
    pub record: CompletionEvidenceRecord,
    pub capsule_artifact: FileDigest,
    #[serde(default)]
    pub proof_artifacts: Vec<FileDigest>,
    /// Optional content custody for referenced artifacts. Digests alone are
    /// evidence references; content is carried only when explicitly included.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub artifact_contents: Vec<PortableArtifactContent>,
    pub envelope_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct PortableArtifactContent {
    pub digest: FileDigest,
    /// Hex encoding keeps the envelope JSON-only and avoids an implicit
    /// binary format or unbounded path-based extraction on import.
    pub content_hex: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct PortableCompletionEvidenceVerification {
    pub schema_version: String,
    pub task_id: String,
    pub envelope_hash: String,
    pub structural_status: String,
    pub local_decision: String,
    pub checks: Vec<EvidenceCheck>,
    pub unresolved_claims: Vec<UnresolvedClaim>,
    pub decision_path: String,
    pub custody_paths: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct EvidenceCheck {
    pub name: String,
    pub status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CompletionEvidenceVerification {
    pub schema_version: String,
    pub task_id: String,
    pub status: String,
    pub checks: Vec<EvidenceCheck>,
    pub unresolved_claims: Vec<UnresolvedClaim>,
}

impl CompletionEvidenceRecord {
    fn canonicalized(&self) -> Self {
        let mut out = self.clone();
        out.spec_refs.sort();
        out.spec_refs.dedup();
        out.state_refs.sort();
        out.state_refs.dedup();
        out.proof_plan.sort();
        out.proof_plan.dedup();
        out.proof_results.sort();
        out.unresolved_claims.sort();
        out.unresolved_claims.dedup();
        out.repository.changed_paths.sort();
        out.repository.changed_paths.dedup();
        out.repository.working_tree.sort();
        out.repository.working_tree.dedup();
        out.evidence_hash.clear();
        out
    }

    pub fn canonical_json_bytes(&self) -> Result<Vec<u8>, serde_json::Error> {
        serde_json::to_vec(&self.canonicalized())
    }

    pub fn computed_hash_hex(&self) -> Result<String, serde_json::Error> {
        let mut hasher = Sha256::new();
        hasher.update(self.canonical_json_bytes()?);
        Ok(format!("sha256:{:x}", hasher.finalize()))
    }

    pub fn with_recomputed_hash(&self) -> Result<Self, serde_json::Error> {
        let mut out = self.clone();
        out.evidence_hash = out.computed_hash_hex()?;
        Ok(out)
    }
}

impl PortableCompletionEvidence {
    fn canonicalized(&self) -> CanonicalPortableCompletionEvidence {
        let mut proof_artifacts = self.proof_artifacts.clone();
        proof_artifacts.sort();
        proof_artifacts.dedup();
        let mut artifact_contents = self.artifact_contents.clone();
        artifact_contents.sort();
        artifact_contents.dedup();
        CanonicalPortableCompletionEvidence {
            schema_version: self.schema_version.clone(),
            source_repository: self.source_repository.clone(),
            record: self.record.clone(),
            capsule_artifact: self.capsule_artifact.clone(),
            proof_artifacts,
            artifact_contents,
        }
    }

    pub fn canonical_json_bytes(&self) -> Result<Vec<u8>, serde_json::Error> {
        serde_json::to_vec(&self.canonicalized())
    }

    pub fn computed_hash_hex(&self) -> Result<String, serde_json::Error> {
        let mut hasher = Sha256::new();
        hasher.update(self.canonical_json_bytes()?);
        Ok(format!("sha256:{:x}", hasher.finalize()))
    }

    pub fn with_recomputed_hash(&self) -> Result<Self, serde_json::Error> {
        let mut out = self.clone();
        out.envelope_hash = out.computed_hash_hex()?;
        Ok(out)
    }
}

#[derive(Debug, Clone, Serialize)]
struct CanonicalPortableCompletionEvidence {
    schema_version: String,
    source_repository: SourceRepositoryBinding,
    record: CompletionEvidenceRecord,
    capsule_artifact: FileDigest,
    proof_artifacts: Vec<FileDigest>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    artifact_contents: Vec<PortableArtifactContent>,
}

pub fn default_record_path(
    project_root: &Path,
    task_id: &str,
) -> Result<PathBuf, error::DecapodError> {
    workunit::validate_task_id(task_id)?;
    Ok(project_root
        .join(COMPLETION_EVIDENCE_DIR)
        .join(format!("{task_id}.json")))
}

pub fn imported_record_path(
    project_root: &Path,
    envelope_hash: &str,
) -> Result<PathBuf, error::DecapodError> {
    if !envelope_hash.starts_with("sha256:")
        || envelope_hash.len() != "sha256:".len() + 64
        || !envelope_hash["sha256:".len()..]
            .chars()
            .all(|character| character.is_ascii_hexdigit())
    {
        return Err(error::DecapodError::ValidationError(
            "portable completion evidence has an invalid envelope hash".to_string(),
        ));
    }
    Ok(project_root
        .join(IMPORTED_COMPLETION_EVIDENCE_DIR)
        .join(format!("{envelope_hash}.json")))
}

pub fn export_record(
    project_root: &Path,
    task_id: &str,
    record_path: &Path,
    output_path: &Path,
) -> Result<PortableCompletionEvidence, error::DecapodError> {
    let record = read_local_record(project_root, task_id, record_path)?;
    let report = verify_record(project_root, task_id, record_path)?;
    if report.status != "current" {
        return Err(error::DecapodError::ValidationError(format!(
            "cannot export completion evidence with verification status {}",
            report.status
        )));
    }

    let capsule_artifact = digest_reference(project_root, &record.capsule.path)?;
    let mut proof_artifacts = record
        .proof_results
        .iter()
        .filter_map(|proof| proof.artifact.clone())
        .collect::<Vec<_>>();
    proof_artifacts.sort();
    proof_artifacts.dedup();
    let mut artifact_contents = Vec::new();
    let mut custody_digests = vec![capsule_artifact.clone()];
    custody_digests.extend(proof_artifacts.iter().cloned());
    custody_digests.sort();
    custody_digests.dedup();
    for digest in custody_digests {
        let bytes = fs::read(safe_reference_path(project_root, &digest.path)?)
            .map_err(error::DecapodError::IoError)?;
        artifact_contents.push(PortableArtifactContent {
            digest,
            content_hex: hex_encode(&bytes),
        });
    }

    let envelope = PortableCompletionEvidence {
        schema_version: COMPLETION_EVIDENCE_SCHEMA_VERSION.to_string(),
        source_repository: SourceRepositoryBinding {
            repository_id: source_repository_id(project_root),
            head_revision: record.repository.head_revision.clone(),
            base_revision: record.repository.base_revision.clone(),
        },
        record,
        capsule_artifact,
        proof_artifacts,
        artifact_contents,
        envelope_hash: String::new(),
    }
    .with_recomputed_hash()
    .map_err(|e| {
        error::DecapodError::ValidationError(format!(
            "portable completion evidence serialization failed: {e}"
        ))
    })?;
    validate_portable_evidence(&envelope)?;

    let parent = output_path.parent().ok_or_else(|| {
        error::DecapodError::ValidationError(
            "portable completion evidence export has no parent directory".to_string(),
        )
    })?;
    fs::create_dir_all(parent).map_err(error::DecapodError::IoError)?;
    let bytes = serde_json::to_vec_pretty(&envelope).map_err(|e| {
        error::DecapodError::ValidationError(format!(
            "portable completion evidence serialization failed: {e}"
        ))
    })?;
    fs::write(output_path, bytes).map_err(error::DecapodError::IoError)?;
    Ok(envelope)
}

pub fn import_record(
    project_root: &Path,
    task_id: &str,
    input_path: &Path,
) -> Result<(PathBuf, PortableCompletionEvidenceVerification), error::DecapodError> {
    let envelope = read_portable_evidence(input_path)?;
    if envelope.record.task_id != task_id {
        return Err(error::DecapodError::ValidationError(format!(
            "portable completion evidence task binding is {}, requested {task_id}",
            envelope.record.task_id
        )));
    }
    let destination = imported_record_path(project_root, &envelope.envelope_hash)?;
    let parent = destination.parent().ok_or_else(|| {
        error::DecapodError::ValidationError(
            "portable completion evidence import has no destination directory".to_string(),
        )
    })?;
    fs::create_dir_all(parent).map_err(error::DecapodError::IoError)?;
    let bytes = fs::read(input_path).map_err(error::DecapodError::IoError)?;
    fs::write(&destination, bytes).map_err(error::DecapodError::IoError)?;
    let custody_paths = write_artifact_custody(project_root, &envelope)?;
    let mut report = verify_portable_evidence(project_root, task_id, &envelope, &destination)?;
    report.custody_paths = custody_paths;
    report.decision_path = format!(
        "{IMPORTED_COMPLETION_EVIDENCE_DIR}/{}.decision.json",
        envelope.envelope_hash
    );
    write_receiver_decision(project_root, &envelope, &report)?;
    Ok((destination, report))
}

pub fn read_portable_evidence(
    input_path: &Path,
) -> Result<PortableCompletionEvidence, error::DecapodError> {
    let raw = fs::read_to_string(input_path).map_err(error::DecapodError::IoError)?;
    let envelope = serde_json::from_str(&raw).map_err(|e| {
        error::DecapodError::ValidationError(format!(
            "portable completion evidence is not valid JSON: {e}"
        ))
    })?;
    validate_portable_evidence(&envelope)?;
    Ok(envelope)
}

fn validate_portable_evidence(
    envelope: &PortableCompletionEvidence,
) -> Result<(), error::DecapodError> {
    if envelope.schema_version != COMPLETION_EVIDENCE_SCHEMA_VERSION {
        return Err(error::DecapodError::ValidationError(format!(
            "PORTABLE_COMPLETION_EVIDENCE_INCOMPATIBLE: schema_version {} is not supported",
            envelope.schema_version
        )));
    }
    let expected_envelope_hash = envelope.computed_hash_hex().map_err(|e| {
        error::DecapodError::ValidationError(format!(
            "PORTABLE_COMPLETION_EVIDENCE_INVALID: envelope hash: {e}"
        ))
    })?;
    if envelope.envelope_hash != expected_envelope_hash {
        return Err(error::DecapodError::ValidationError(
            "PORTABLE_COMPLETION_EVIDENCE_TAMPERED: envelope_hash does not match canonical contents"
                .to_string(),
        ));
    }
    let expected_record_hash = envelope.record.computed_hash_hex().map_err(|e| {
        error::DecapodError::ValidationError(format!(
            "PORTABLE_COMPLETION_EVIDENCE_INVALID: record hash: {e}"
        ))
    })?;
    if envelope.record.evidence_hash != expected_record_hash {
        return Err(error::DecapodError::ValidationError(
            "PORTABLE_COMPLETION_EVIDENCE_TAMPERED: evidence_hash does not match canonical record"
                .to_string(),
        ));
    }
    if envelope.source_repository.head_revision != envelope.record.repository.head_revision
        || envelope.source_repository.base_revision != envelope.record.repository.base_revision
    {
        return Err(error::DecapodError::ValidationError(
            "PORTABLE_COMPLETION_EVIDENCE_INVALID: source repository binding does not match record"
                .to_string(),
        ));
    }

    validate_relative_evidence_path(&envelope.record.capsule.path)?;
    if envelope.capsule_artifact.path != envelope.record.capsule.path {
        return Err(error::DecapodError::ValidationError(
            "PORTABLE_COMPLETION_EVIDENCE_INVALID: capsule artifact does not match record"
                .to_string(),
        ));
    }
    for path in &envelope.record.repository.changed_paths {
        validate_relative_evidence_path(path)?;
    }
    for entry in &envelope.record.repository.working_tree {
        validate_relative_evidence_path(&entry.path)?;
    }

    let mut expected_artifacts = Vec::new();
    for proof in &envelope.record.proof_results {
        match (&proof.artifact_ref, &proof.artifact) {
            (None, None) => {}
            (Some(reference), Some(artifact)) => {
                validate_relative_evidence_path(reference)?;
                if artifact.path != *reference {
                    return Err(error::DecapodError::ValidationError(format!(
                        "PORTABLE_COMPLETION_EVIDENCE_INVALID: proof artifact path {} does not match reference {reference}",
                        artifact.path
                    )));
                }
                expected_artifacts.push(artifact.clone());
            }
            _ => {
                return Err(error::DecapodError::ValidationError(
                    "PORTABLE_COMPLETION_EVIDENCE_INVALID: proof artifact reference and digest must be paired"
                        .to_string(),
                ));
            }
        }
    }
    expected_artifacts.sort();
    expected_artifacts.dedup();
    let mut actual_artifacts = envelope.proof_artifacts.clone();
    actual_artifacts.sort();
    actual_artifacts.dedup();
    if expected_artifacts != actual_artifacts {
        return Err(error::DecapodError::ValidationError(
            "PORTABLE_COMPLETION_EVIDENCE_INVALID: proof artifact custody manifest does not match record"
                .to_string(),
        ));
    }
    for artifact in &envelope.proof_artifacts {
        validate_relative_evidence_path(&artifact.path)?;
    }
    let expected_custody = expected_artifacts
        .iter()
        .chain(std::iter::once(&envelope.capsule_artifact))
        .cloned()
        .collect::<BTreeSet<_>>();
    let mut actual_custody = BTreeSet::new();
    for content in &envelope.artifact_contents {
        if !expected_custody.contains(&content.digest) {
            return Err(error::DecapodError::ValidationError(
                "PORTABLE_COMPLETION_EVIDENCE_INVALID: artifact custody contains an unreferenced digest".to_string(),
            ));
        }
        let bytes = hex_decode(&content.content_hex).ok_or_else(|| {
            error::DecapodError::ValidationError(
                "PORTABLE_COMPLETION_EVIDENCE_INVALID: artifact custody is not valid hex"
                    .to_string(),
            )
        })?;
        if hash_bytes(&bytes) != content.digest.sha256 || bytes.len() as u64 != content.digest.size
        {
            return Err(error::DecapodError::ValidationError(
                "PORTABLE_COMPLETION_EVIDENCE_TAMPERED: artifact custody digest mismatch"
                    .to_string(),
            ));
        }
        actual_custody.insert(content.digest.clone());
    }
    if !envelope.artifact_contents.is_empty() && actual_custody != expected_custody {
        return Err(error::DecapodError::ValidationError(
            "PORTABLE_COMPLETION_EVIDENCE_INVALID: artifact custody is incomplete".to_string(),
        ));
    }
    Ok(())
}

fn validate_relative_evidence_path(path: &str) -> Result<(), error::DecapodError> {
    let candidate = Path::new(path);
    if path.is_empty()
        || candidate.is_absolute()
        || candidate
            .components()
            .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        return Err(error::DecapodError::ValidationError(format!(
            "PORTABLE_COMPLETION_EVIDENCE_INVALID: path is not repository-relative: {path}"
        )));
    }
    Ok(())
}

fn source_repository_id(project_root: &Path) -> Option<String> {
    let remote = optional_git_output(project_root, &["remote", "get-url", "origin"])?;
    sanitized_source_repository_id(&remote)
}

fn sanitized_source_repository_id(remote: &str) -> Option<String> {
    let remote = remote.trim();
    if remote.is_empty() {
        return None;
    }
    if let Some((host, path)) = remote
        .strip_prefix("git@")
        .and_then(|value| value.split_once(':'))
    {
        return Some(format!("ssh://{host}/{path}"));
    }
    if let Some((scheme, remainder)) = remote.split_once("://") {
        let without_credentials = remainder
            .rsplit_once('@')
            .map(|(_, value)| value)
            .unwrap_or(remainder);
        return Some(format!("{scheme}://{without_credentials}"));
    }
    Some(remote.to_string())
}

fn verify_portable_evidence(
    project_root: &Path,
    task_id: &str,
    envelope: &PortableCompletionEvidence,
    imported_path: &Path,
) -> Result<PortableCompletionEvidenceVerification, error::DecapodError> {
    let mut checks = vec![check("envelope_integrity", "pass", None)];
    let workunit_path = workunit::workunit_path(project_root, task_id)?;
    let (structural_status, local_decision) = if !workunit_path.exists() {
        checks.push(check(
            "local_policy",
            "fail",
            Some("receiving repository has no matching local workunit".to_string()),
        ));
        ("structurally_valid", "rejected")
    } else {
        let local_report =
            verify_record_value(project_root, task_id, &envelope.record, imported_path)?;
        for local_check in local_report.checks {
            checks.push(check(
                &format!("local:{}", local_check.name),
                &local_check.status,
                local_check.detail,
            ));
        }
        if local_report.status == "current" {
            ("structurally_valid", "accepted")
        } else {
            checks.push(check(
                "local_policy",
                "fail",
                Some(format!(
                    "receiving repository verification status: {}",
                    local_report.status
                )),
            ));
            ("structurally_valid", "rejected")
        }
    };
    Ok(PortableCompletionEvidenceVerification {
        schema_version: envelope.schema_version.clone(),
        task_id: task_id.to_string(),
        envelope_hash: envelope.envelope_hash.clone(),
        structural_status: structural_status.to_string(),
        local_decision: local_decision.to_string(),
        checks,
        unresolved_claims: envelope.record.unresolved_claims.clone(),
        decision_path: String::new(),
        custody_paths: Vec::new(),
    })
}

fn write_artifact_custody(
    project_root: &Path,
    envelope: &PortableCompletionEvidence,
) -> Result<Vec<String>, error::DecapodError> {
    let base = project_root
        .join(IMPORTED_COMPLETION_EVIDENCE_DIR)
        .join(&envelope.envelope_hash)
        .join("artifacts");
    fs::create_dir_all(&base).map_err(error::DecapodError::IoError)?;
    let mut paths = Vec::new();
    for content in &envelope.artifact_contents {
        let path = base.join(&content.digest.sha256["sha256:".len()..]);
        let bytes = hex_decode(&content.content_hex).ok_or_else(|| {
            error::DecapodError::ValidationError(
                "portable completion evidence artifact custody is not valid hex".to_string(),
            )
        })?;
        if path.exists() {
            let existing = fs::read(&path).map_err(error::DecapodError::IoError)?;
            if existing != bytes {
                return Err(error::DecapodError::ValidationError(
                    "portable completion evidence artifact custody is immutable".to_string(),
                ));
            }
        } else {
            fs::write(&path, bytes).map_err(error::DecapodError::IoError)?;
        }
        paths.push(relative_path(project_root, &path)?);
    }
    Ok(paths)
}

fn write_receiver_decision(
    project_root: &Path,
    envelope: &PortableCompletionEvidence,
    report: &PortableCompletionEvidenceVerification,
) -> Result<(), error::DecapodError> {
    let path = project_root
        .join(IMPORTED_COMPLETION_EVIDENCE_DIR)
        .join(format!("{}.decision.json", envelope.envelope_hash));
    let bytes = serde_json::to_vec_pretty(report).map_err(|e| {
        error::DecapodError::ValidationError(format!(
            "portable completion evidence decision serialization failed: {e}"
        ))
    })?;
    if path.exists() {
        let existing = fs::read(&path).map_err(error::DecapodError::IoError)?;
        if existing != bytes {
            return Err(error::DecapodError::ValidationError(
                "portable completion evidence receiver decision is immutable".to_string(),
            ));
        }
    } else {
        fs::write(&path, bytes).map_err(error::DecapodError::IoError)?;
    }
    Ok(())
}

fn read_local_record(
    project_root: &Path,
    task_id: &str,
    record_path: &Path,
) -> Result<CompletionEvidenceRecord, error::DecapodError> {
    let record_path = fs::canonicalize(record_path).map_err(error::DecapodError::IoError)?;
    let project_root_canonical =
        fs::canonicalize(project_root).map_err(error::DecapodError::IoError)?;
    if !record_path.starts_with(&project_root_canonical) {
        return Err(error::DecapodError::ValidationError(
            "completion evidence record must be inside the repository".to_string(),
        ));
    }
    let raw = fs::read_to_string(&record_path).map_err(error::DecapodError::IoError)?;
    let record: CompletionEvidenceRecord = serde_json::from_str(&raw).map_err(|e| {
        error::DecapodError::ValidationError(format!(
            "completion evidence record is not valid JSON: {e}"
        ))
    })?;
    if record.task_id != task_id {
        return Err(error::DecapodError::ValidationError(format!(
            "completion evidence record task binding is {}, requested {task_id}",
            record.task_id
        )));
    }
    Ok(record)
}

pub fn build_record(
    project_root: &Path,
    task_id: &str,
) -> Result<CompletionEvidenceRecord, error::DecapodError> {
    let manifest = workunit::load_workunit(project_root, task_id)?;
    if manifest.status != WorkUnitStatus::Verified {
        return Err(error::DecapodError::ValidationError(format!(
            "completion evidence requires VERIFIED workunit status for '{task_id}'"
        )));
    }
    workunit::validate_verified_manifest(&manifest)?;
    workunit::verify_capsule_policy_lineage_for_task(project_root, &manifest)?;

    let capsule_path = project_root
        .join(".decapod/generated/context")
        .join(format!("{task_id}.json"));
    let capsule = load_capsule(&capsule_path)?;
    let capsule_hash = capsule.computed_hash_hex().map_err(|e| {
        error::DecapodError::ValidationError(format!(
            "completion evidence capsule hash could not be computed: {e}"
        ))
    })?;
    if capsule.capsule_hash != capsule_hash {
        return Err(error::DecapodError::ValidationError(format!(
            "completion evidence capsule hash mismatch at {}",
            capsule_path.display()
        )));
    }

    let active_epoch = active_validation_epoch(project_root)?;
    let captured_epoch = manifest.validation_epoch.as_ref().ok_or_else(|| {
        error::DecapodError::ValidationError(format!(
            "completion evidence requires a validation epoch in workunit '{task_id}'"
        ))
    })?;
    if captured_epoch.epoch_id != active_epoch.epoch_id {
        return Err(error::DecapodError::ValidationError(format!(
            "completion evidence validation epoch is stale: captured {} but active {}",
            captured_epoch.epoch_id, active_epoch.epoch_id
        )));
    }

    let record_path = default_record_path(project_root, task_id)?;
    let repository = repository_evidence(project_root, &record_path)?;
    let proof_results = manifest
        .proof_results
        .iter()
        .map(|result| {
            let artifact = result
                .artifact_ref
                .as_deref()
                .map(|reference| digest_reference(project_root, reference))
                .transpose()?;
            Ok(CompletionProofEvidence {
                gate: result.gate.clone(),
                status: result.status.clone(),
                artifact_ref: result.artifact_ref.clone(),
                artifact,
            })
        })
        .collect::<Result<Vec<_>, error::DecapodError>>()?;

    let (unresolved_claims, plan_hash) = unresolved_claims(project_root)?;
    let record = CompletionEvidenceRecord {
        schema_version: COMPLETION_EVIDENCE_SCHEMA_VERSION.to_string(),
        task_id: manifest.task_id.clone(),
        workunit_hash: manifest.canonical_hash_hex().map_err(|e| {
            error::DecapodError::ValidationError(format!(
                "completion evidence workunit hash failed: {e}"
            ))
        })?,
        intent_ref: manifest.intent_ref,
        spec_refs: manifest.spec_refs,
        state_refs: manifest.state_refs,
        proof_plan: manifest.proof_plan,
        proof_results,
        capsule: CapsuleEvidence {
            path: relative_path(project_root, &capsule_path)?,
            capsule_hash,
            policy_hash: capsule.policy.policy_hash,
            policy_version: capsule.policy.policy_version,
            repo_revision: capsule.policy.repo_revision,
        },
        repository,
        validation_epoch: ValidationEpochEvidence {
            epoch_id: active_epoch.epoch_id.clone(),
            epoch_hash: hash_json(&active_epoch)?,
        },
        unresolved_claims,
        plan_hash,
        evidence_hash: String::new(),
    };
    record.with_recomputed_hash().map_err(|e| {
        error::DecapodError::ValidationError(format!(
            "completion evidence serialization failed: {e}"
        ))
    })
}

pub fn write_record(
    project_root: &Path,
    record: &CompletionEvidenceRecord,
) -> Result<PathBuf, error::DecapodError> {
    let path = default_record_path(project_root, &record.task_id)?;
    let parent = path.parent().ok_or_else(|| {
        error::DecapodError::ValidationError(
            "completion evidence record has no parent directory".to_string(),
        )
    })?;
    fs::create_dir_all(parent).map_err(error::DecapodError::IoError)?;
    let bytes = serde_json::to_vec_pretty(record).map_err(|e| {
        error::DecapodError::ValidationError(format!(
            "completion evidence record serialization failed: {e}"
        ))
    })?;
    fs::write(&path, bytes).map_err(error::DecapodError::IoError)?;
    Ok(path)
}

pub fn verify_record(
    project_root: &Path,
    task_id: &str,
    record_path: &Path,
) -> Result<CompletionEvidenceVerification, error::DecapodError> {
    let record_path = fs::canonicalize(record_path).map_err(error::DecapodError::IoError)?;
    let project_root_canonical =
        fs::canonicalize(project_root).map_err(error::DecapodError::IoError)?;
    if !record_path.starts_with(&project_root_canonical) {
        return Err(error::DecapodError::ValidationError(
            "completion evidence record must be inside the repository".to_string(),
        ));
    }
    let raw = fs::read_to_string(&record_path).map_err(error::DecapodError::IoError)?;
    let record: CompletionEvidenceRecord = serde_json::from_str(&raw).map_err(|e| {
        error::DecapodError::ValidationError(format!(
            "completion evidence record is not valid JSON: {e}"
        ))
    })?;
    verify_record_value(project_root, task_id, &record, &record_path)
}

fn verify_record_value(
    project_root: &Path,
    task_id: &str,
    record: &CompletionEvidenceRecord,
    record_path: &Path,
) -> Result<CompletionEvidenceVerification, error::DecapodError> {
    if record.schema_version != COMPLETION_EVIDENCE_SCHEMA_VERSION {
        return Ok(report(
            record,
            "invalid",
            vec![check(
                "schema",
                "fail",
                Some(format!(
                    "unsupported schema version {}",
                    record.schema_version
                )),
            )],
        ));
    }
    if record.task_id != task_id {
        return Ok(report(
            record,
            "invalid",
            vec![check(
                "task_binding",
                "fail",
                Some(format!(
                    "record task_id is {}, requested {task_id}",
                    record.task_id
                )),
            )],
        ));
    }

    let mut checks = Vec::new();
    let computed_hash = record.computed_hash_hex().map_err(|e| {
        error::DecapodError::ValidationError(format!(
            "completion evidence hash computation failed: {e}"
        ))
    })?;
    let record_hash_ok = record.evidence_hash == computed_hash;
    checks.push(check(
        "record_integrity",
        if record_hash_ok { "pass" } else { "fail" },
        Some(format!(
            "expected {}, computed {}",
            record.evidence_hash, computed_hash
        )),
    ));
    if !record_hash_ok {
        return Ok(report(record, "altered", checks));
    }

    let manifest = match workunit::load_workunit(project_root, task_id) {
        Ok(manifest) => manifest,
        Err(error) => {
            checks.push(check("workunit", "fail", Some(error.to_string())));
            return Ok(report(record, "incomplete", checks));
        }
    };
    let current_workunit_hash = manifest.canonical_hash_hex().map_err(|e| {
        error::DecapodError::ValidationError(format!(
            "completion evidence workunit hash computation failed: {e}"
        ))
    })?;
    let workunit_ok = current_workunit_hash == record.workunit_hash;
    checks.push(check(
        "workunit_binding",
        if workunit_ok { "pass" } else { "fail" },
        Some(format!(
            "record {}, current {}",
            record.workunit_hash, current_workunit_hash
        )),
    ));

    let mut record_proofs: Vec<_> = record
        .proof_results
        .iter()
        .map(|proof| {
            (
                proof.gate.clone(),
                proof.status.clone(),
                proof.artifact_ref.clone(),
            )
        })
        .collect();
    let mut current_proofs: Vec<_> = manifest
        .proof_results
        .iter()
        .map(|proof| {
            (
                proof.gate.clone(),
                proof.status.clone(),
                proof.artifact_ref.clone(),
            )
        })
        .collect();
    record_proofs.sort();
    current_proofs.sort();
    let workunit_projection_ok = record.intent_ref == manifest.intent_ref
        && canonical_strings(&record.spec_refs) == canonical_strings(&manifest.spec_refs)
        && canonical_strings(&record.state_refs) == canonical_strings(&manifest.state_refs)
        && canonical_strings(&record.proof_plan) == canonical_strings(&manifest.proof_plan)
        && record_proofs == current_proofs;
    checks.push(check(
        "workunit_projection",
        if workunit_projection_ok {
            "pass"
        } else {
            "fail"
        },
        if workunit_projection_ok {
            None
        } else {
            Some("record fields do not match the current workunit".to_string())
        },
    ));

    let workunit_ready = workunit::validate_verified_manifest(&manifest).is_ok();
    checks.push(check(
        "proof_plan",
        if workunit_ready { "pass" } else { "fail" },
        if workunit_ready {
            None
        } else {
            Some("workunit is not VERIFIED with passing proof results".to_string())
        },
    ));

    let capsule_path = safe_reference_path(project_root, &record.capsule.path)?;
    let capsule_result = load_capsule(&capsule_path).and_then(|capsule| {
        let computed = capsule.computed_hash_hex().map_err(|e| {
            error::DecapodError::ValidationError(format!("capsule hash failed: {e}"))
        })?;
        if computed != capsule.capsule_hash || computed != record.capsule.capsule_hash {
            return Err(error::DecapodError::ValidationError(
                "capsule hash does not match record".to_string(),
            ));
        }
        if capsule.task_id.as_deref() != Some(task_id)
            && capsule.workunit_id.as_deref() != Some(task_id)
        {
            return Err(error::DecapodError::ValidationError(
                "capsule task/workunit binding mismatch".to_string(),
            ));
        }
        if capsule.policy.policy_hash != record.capsule.policy_hash
            || capsule.policy.policy_version != record.capsule.policy_version
            || capsule.policy.repo_revision != record.capsule.repo_revision
        {
            return Err(error::DecapodError::ValidationError(
                "capsule policy binding does not match record".to_string(),
            ));
        }
        Ok(capsule)
    });
    let capsule_ok = capsule_result.is_ok();
    checks.push(check(
        "capsule_binding",
        if capsule_ok { "pass" } else { "fail" },
        capsule_result.err().map(|e| e.to_string()),
    ));

    let active_epoch = active_validation_epoch(project_root)?;
    let current_epoch_hash = hash_json(&active_epoch)?;
    let epoch_ok = record.validation_epoch.epoch_id == active_epoch.epoch_id
        && record.validation_epoch.epoch_hash == current_epoch_hash;
    checks.push(check(
        "validation_epoch",
        if epoch_ok { "pass" } else { "fail" },
        Some(format!(
            "record {} / {}, active {} / {}",
            record.validation_epoch.epoch_id,
            record.validation_epoch.epoch_hash,
            active_epoch.epoch_id,
            current_epoch_hash
        )),
    ));

    let current_repository = repository_evidence(project_root, record_path)?;
    let repository_ok = current_repository == record.repository;
    checks.push(check(
        "repository_state",
        if repository_ok { "pass" } else { "fail" },
        if repository_ok {
            None
        } else {
            Some("repository revision or changed surface differs from the record".to_string())
        },
    ));

    let artifact_status = verify_proof_artifacts(project_root, &record.proof_results, &mut checks);
    let plan_status = verify_plan(
        project_root,
        &record.plan_hash,
        &record.unresolved_claims,
        &mut checks,
    )?;

    let status =
        if !workunit_ok || !workunit_projection_ok || !capsule_ok || artifact_status == "altered" {
            "altered"
        } else if !workunit_ready || artifact_status == "incomplete" || plan_status == "incomplete"
        {
            "incomplete"
        } else if !epoch_ok {
            "stale"
        } else if !repository_ok || plan_status == "mismatch" {
            "state_mismatch"
        } else {
            "current"
        };
    Ok(report(record, status, checks))
}

fn verify_proof_artifacts(
    project_root: &Path,
    results: &[CompletionProofEvidence],
    checks: &mut Vec<EvidenceCheck>,
) -> &'static str {
    let mut status = "pass";
    for result in results {
        let Some(reference) = result.artifact_ref.as_deref() else {
            continue;
        };
        match digest_reference(project_root, reference) {
            Ok(actual) if result.artifact.as_ref() == Some(&actual) => {
                checks.push(check(&format!("proof_artifact:{reference}"), "pass", None))
            }
            Ok(actual) => {
                status = "altered";
                checks.push(check(
                    &format!("proof_artifact:{reference}"),
                    "fail",
                    Some(format!(
                        "record {:?}, current {:?}",
                        result.artifact, actual
                    )),
                ));
            }
            Err(error) => {
                if status != "altered" {
                    status = "incomplete";
                }
                checks.push(check(
                    &format!("proof_artifact:{reference}"),
                    "fail",
                    Some(error.to_string()),
                ));
            }
        }
    }
    status
}

fn verify_plan(
    project_root: &Path,
    expected_hash: &Option<String>,
    expected_claims: &[UnresolvedClaim],
    checks: &mut Vec<EvidenceCheck>,
) -> Result<&'static str, error::DecapodError> {
    let Some(plan) = plan_governance::load_plan(project_root)? else {
        if expected_hash.is_some() {
            checks.push(check(
                "governed_plan",
                "fail",
                Some("record references a missing governed plan".to_string()),
            ));
            return Ok("incomplete");
        }
        checks.push(check("governed_plan", "pass", None));
        return Ok("pass");
    };
    let current_hash = hash_json(&plan)?;
    let current_claims = claims_from_plan(&plan);
    let hash_ok = expected_hash.as_deref() == Some(current_hash.as_str());
    let claims_ok = expected_claims == current_claims;
    checks.push(check(
        "governed_plan",
        if hash_ok && claims_ok { "pass" } else { "fail" },
        Some(format!(
            "record {:?}, current {current_hash}",
            expected_hash
        )),
    ));
    Ok(if hash_ok && claims_ok {
        "pass"
    } else {
        "mismatch"
    })
}

fn report(
    record: &CompletionEvidenceRecord,
    status: &str,
    checks: Vec<EvidenceCheck>,
) -> CompletionEvidenceVerification {
    CompletionEvidenceVerification {
        schema_version: record.schema_version.clone(),
        task_id: record.task_id.clone(),
        status: status.to_string(),
        checks,
        unresolved_claims: record.unresolved_claims.clone(),
    }
}

fn check(name: &str, status: &str, detail: Option<String>) -> EvidenceCheck {
    EvidenceCheck {
        name: name.to_string(),
        status: status.to_string(),
        detail,
    }
}

fn canonical_strings(values: &[String]) -> Vec<String> {
    let mut values = values.to_vec();
    values.sort();
    values.dedup();
    values
}

fn load_capsule(path: &Path) -> Result<DeterministicContextCapsule, error::DecapodError> {
    let raw = fs::read_to_string(path).map_err(error::DecapodError::IoError)?;
    serde_json::from_str(&raw).map_err(|e| {
        error::DecapodError::ValidationError(format!(
            "invalid context capsule {}: {e}",
            path.display()
        ))
    })
}

fn hash_json<T: Serialize>(value: &T) -> Result<String, error::DecapodError> {
    let bytes = serde_json::to_vec(value).map_err(|e| {
        error::DecapodError::ValidationError(format!("completion evidence JSON hash failed: {e}"))
    })?;
    Ok(hash_bytes(&bytes))
}

fn hash_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("sha256:{:x}", hasher.finalize())
}

fn hex_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

fn hex_decode(value: &str) -> Option<Vec<u8>> {
    if !value.len().is_multiple_of(2) {
        return None;
    }
    (0..value.len())
        .step_by(2)
        .map(|index| u8::from_str_radix(&value[index..index + 2], 16).ok())
        .collect()
}

fn digest_reference(
    project_root: &Path,
    reference: &str,
) -> Result<FileDigest, error::DecapodError> {
    let path = safe_reference_path(project_root, reference)?;
    let bytes = fs::read(&path).map_err(error::DecapodError::IoError)?;
    let metadata = fs::metadata(&path).map_err(error::DecapodError::IoError)?;
    Ok(FileDigest {
        path: relative_path(project_root, &path)?,
        sha256: hash_bytes(&bytes),
        size: metadata.len(),
    })
}

fn safe_reference_path(
    project_root: &Path,
    reference: &str,
) -> Result<PathBuf, error::DecapodError> {
    let root = fs::canonicalize(project_root).map_err(error::DecapodError::IoError)?;
    let candidate = if Path::new(reference).is_absolute() {
        PathBuf::from(reference)
    } else {
        project_root.join(reference)
    };
    let path = fs::canonicalize(&candidate).map_err(|_| {
        error::DecapodError::NotFound(format!(
            "completion evidence artifact not found: {reference}"
        ))
    })?;
    if !path.starts_with(&root) {
        return Err(error::DecapodError::ValidationError(format!(
            "completion evidence artifact escapes repository root: {reference}"
        )));
    }
    Ok(path)
}

fn relative_path(project_root: &Path, path: &Path) -> Result<String, error::DecapodError> {
    let root = fs::canonicalize(project_root).map_err(error::DecapodError::IoError)?;
    let path = fs::canonicalize(path).map_err(error::DecapodError::IoError)?;
    path.strip_prefix(root)
        .map(|relative| relative.to_string_lossy().replace('\\', "/"))
        .map_err(|_| {
            error::DecapodError::ValidationError(format!(
                "completion evidence path is outside repository: {}",
                path.display()
            ))
        })
}

fn git_output(project_root: &Path, args: &[&str]) -> Result<String, error::DecapodError> {
    let output = Command::new("git")
        .args(args)
        .current_dir(project_root)
        .output()
        .map_err(error::DecapodError::IoError)?;
    if !output.status.success() {
        return Err(error::DecapodError::ValidationError(format!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn optional_git_output(project_root: &Path, args: &[&str]) -> Option<String> {
    git_output(project_root, args)
        .ok()
        .filter(|value| !value.is_empty())
}

fn repository_evidence(
    project_root: &Path,
    record_path: &Path,
) -> Result<RepositoryEvidence, error::DecapodError> {
    let head_revision = git_output(project_root, &["rev-parse", "HEAD"])?;
    let base_revision = optional_git_output(project_root, &["rev-parse", "HEAD^"]);
    let mut changed_paths = BTreeSet::new();
    if let Some(base) = base_revision.as_deref() {
        let diff = git_output(
            project_root,
            &["diff", "--name-only", "--no-renames", base, &head_revision],
        )?;
        changed_paths.extend(
            diff.lines()
                .filter(|path| !path.is_empty())
                .map(str::to_string),
        );
    } else {
        let tree = git_output(
            project_root,
            &["ls-tree", "-r", "--name-only", &head_revision],
        )?;
        changed_paths.extend(
            tree.lines()
                .filter(|path| !path.is_empty())
                .map(str::to_string),
        );
    }

    let record_relative = relative_path(project_root, record_path).unwrap_or_default();
    let status = git_output(
        project_root,
        &["status", "--porcelain=v1", "--untracked-files=all"],
    )?;
    let mut working_tree = Vec::new();
    for line in status.lines().filter(|line| !line.is_empty()) {
        let raw_path = line.get(3..).unwrap_or(line).trim();
        let path = raw_path
            .rsplit_once(" -> ")
            .map(|(_, destination)| destination)
            .unwrap_or(raw_path)
            .trim_matches('"')
            .replace('\\', "/");
        if path == record_relative {
            continue;
        }
        changed_paths.insert(path.clone());
        let disk_path = project_root.join(&path);
        if disk_path.is_file() {
            let digest = digest_reference(project_root, &path)?;
            working_tree.push(WorkingTreeEntry {
                path,
                exists: true,
                sha256: Some(digest.sha256),
                size: Some(digest.size),
            });
        } else {
            working_tree.push(WorkingTreeEntry {
                path,
                exists: false,
                sha256: None,
                size: None,
            });
        }
    }

    Ok(RepositoryEvidence {
        head_revision,
        base_revision,
        changed_paths: changed_paths.into_iter().collect(),
        working_tree,
    })
}

fn unresolved_claims(
    project_root: &Path,
) -> Result<(Vec<UnresolvedClaim>, Option<String>), error::DecapodError> {
    let Some(plan) = plan_governance::load_plan(project_root)? else {
        return Ok((Vec::new(), None));
    };
    let plan_hash = Some(hash_json(&plan)?);
    Ok((claims_from_plan(&plan), plan_hash))
}

fn claims_from_plan(plan: &plan_governance::GovernedPlan) -> Vec<UnresolvedClaim> {
    let mut claims = Vec::new();
    for (kind, values) in [
        ("unknown", &plan.unknowns),
        ("human_question", &plan.human_questions),
        ("contradiction", &plan.unresolved_contradictions),
        ("deferred_question", &plan.deferred_questions),
    ] {
        claims.extend(values.iter().cloned().map(|text| UnresolvedClaim {
            kind: kind.to_string(),
            text,
        }));
    }
    claims.sort();
    claims.dedup();
    claims
}
#[cfg(test)]
#[path = "../../../tests/unit/core/completion_evidence_tests.rs"]
mod tests;
