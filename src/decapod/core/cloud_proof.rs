//! Cloud candidate proof acceptance lifecycle and hosted lease-fence client (#1809, #1038).
//!
//! Aligns byte-for-byte with `propodus` `proof_capture_contract.go`, `proof_acceptance.go`,
//! and `029_candidate_proof_acceptance.sql`:
//! `captured -> independently_verified -> currently_admissible -> published -> historically_accepted`.
//!
//! Never fabricates local SQLite receipts on cloud denial, preserves `outcome_unknown`
//! reconciliation via `get_operation`/`reconstruct`, and maps hosted 403/413/429
//! responses into distinct typed Decapod errors.

use crate::core::error::DecapodError;
use serde::{Deserialize, Serialize};
use serde_json::{Value as JsonValue, json};
use sha2::{Digest, Sha256};

pub const PROOF_CAPTURE_CONTRACT_ID: &str = "propodus.proof-capture.v1";
pub const PROOF_CAPTURE_CONTRACT_DIGEST: &str =
    "sha256:0000000000000000000000000000000000000000000000000000000000000001";
pub const PROOF_ACCEPTANCE_CONTROL_OP: &str = "propodus_proof_acceptance_v1";
pub const PROOF_ACCEPTANCE_READ_OP: &str = "propodus_proof_acceptance_get_v1";
pub const REPOSITORY_ACCESS_CONTROL_OP: &str = "propodus_repository_access_v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LeaseCustodyRecord {
    pub custody_id: String,
    pub fencing_generation: i64,
    pub lease_ttl_seconds: u64,
    pub lease_expires_at: String,
    pub heartbeat_at: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProofLifecycleState {
    Captured,
    IndependentlyVerified,
    CurrentlyAdmissible,
    Published,
    HistoricallyAccepted,
}

impl ProofLifecycleState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Captured => "captured",
            Self::IndependentlyVerified => "independently_verified",
            Self::CurrentlyAdmissible => "currently_admissible",
            Self::Published => "published",
            Self::HistoricallyAccepted => "historically_accepted",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloudControlDenialKind {
    EntitlementForbidden,
    PayloadTooLarge,
    QuotaRateLimited,
    OutcomeUnknown,
    Conflict,
    InvalidRequest,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CloudControlError {
    pub http_status: Option<u16>,
    pub code: String,
    pub detail: String,
    pub kind: CloudControlDenialKind,
}

impl CloudControlError {
    pub fn to_decapod_error(&self, operation_id: Option<&str>) -> DecapodError {
        let op_hint = operation_id
            .map(|id| format!(" (operation_id={id})"))
            .unwrap_or_default();
        match self.kind {
            CloudControlDenialKind::EntitlementForbidden => DecapodError::ValidationError(format!(
                "CLOUD_ENTITLEMENT_DENIED{op_hint}: [HTTP {}] {}: {}. No local fallback occurred.",
                self.http_status.unwrap_or(403),
                self.code,
                self.detail
            )),
            CloudControlDenialKind::PayloadTooLarge => DecapodError::ValidationError(format!(
                "CLOUD_PAYLOAD_BOUND_EXCEEDED{op_hint}: [HTTP {}] {}: {}. Reduce submission/evidence bytes within server bounds.",
                self.http_status.unwrap_or(413),
                self.code,
                self.detail
            )),
            CloudControlDenialKind::QuotaRateLimited => DecapodError::ValidationError(format!(
                "CLOUD_QUOTA_EXCEEDED{op_hint}: [HTTP {}] {}: {}. Retry with bounded backoff; no local fallback occurred.",
                self.http_status.unwrap_or(429),
                self.code,
                self.detail
            )),
            CloudControlDenialKind::OutcomeUnknown => DecapodError::ValidationError(format!(
                "CLOUD_OUTCOME_UNKNOWN{op_hint}: {}: {}. Reconcile via `get_operation` before retrying; do not auto-replay.",
                self.code, self.detail
            )),
            CloudControlDenialKind::Conflict => DecapodError::ValidationError(format!(
                "CLOUD_PROOF_CONFLICT{op_hint}: {}: {}",
                self.code, self.detail
            )),
            CloudControlDenialKind::InvalidRequest => DecapodError::ValidationError(format!(
                "CLOUD_PROOF_INVALID{op_hint}: {}: {}",
                self.code, self.detail
            )),
            CloudControlDenialKind::Unavailable => DecapodError::ValidationError(format!(
                "CLOUD_CONTROL_UNAVAILABLE{op_hint}: {}: {}",
                self.code, self.detail
            )),
        }
    }
}

pub fn classify_control_status(
    http_status: Option<u16>,
    code: &str,
    detail: &str,
    outcome_unknown: bool,
) -> CloudControlError {
    let kind = if outcome_unknown {
        CloudControlDenialKind::OutcomeUnknown
    } else {
        match http_status {
            Some(403) => CloudControlDenialKind::EntitlementForbidden,
            Some(413) => CloudControlDenialKind::PayloadTooLarge,
            Some(429) => CloudControlDenialKind::QuotaRateLimited,
            Some(409) => CloudControlDenialKind::Conflict,
            Some(400) => CloudControlDenialKind::InvalidRequest,
            _ => {
                let lower = code.to_ascii_lowercase();
                if lower.contains("limit") || lower.contains("payload_too_large") {
                    CloudControlDenialKind::PayloadTooLarge
                } else if lower.contains("quota") || lower.contains("rate_limit") {
                    CloudControlDenialKind::QuotaRateLimited
                } else if lower.contains("denied")
                    || lower.contains("forbidden")
                    || lower.contains("owner_required")
                    || lower.contains("unverified")
                    || lower.contains("revoked")
                {
                    CloudControlDenialKind::EntitlementForbidden
                } else if lower.contains("conflict") || lower.contains("stale") {
                    CloudControlDenialKind::Conflict
                } else if lower.contains("invalid") {
                    CloudControlDenialKind::InvalidRequest
                } else {
                    CloudControlDenialKind::Unavailable
                }
            }
        }
    };
    CloudControlError {
        http_status,
        code: code.to_string(),
        detail: detail.to_string(),
        kind,
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProofGitIdentity {
    pub commit: String,
    pub tree: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProofEvidenceReference {
    pub kind: String,
    pub digest: String,
    pub bytes: i64,
    pub provenance_digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProofSubject {
    pub repository_id: String,
    pub task_id: String,
    pub workspace_id: String,
    pub custody_id: String,
    pub task_revision: i64,
    pub fencing_generation: i64,
    pub candidate: ProofGitIdentity,
    pub accepted_target: ProofGitIdentity,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub integration: Option<ProofGitIdentity>,
    pub policy_digest: String,
    pub requirements_digest: String,
    pub material_digest: String,
    pub prerequisites_digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProofCaptureRequest {
    pub contract: String,
    pub contract_digest: String,
    pub operation_id: String,
    pub subject: ProofSubject,
    pub evidence: Vec<ProofEvidenceReference>,
}

/// Domain-separated SHA-256 fingerprint matching `propodus`
/// `ProofCaptureFingerprint` byte-for-byte (non-semantic evidence order sorted by `(kind, digest)`).
pub fn proof_capture_fingerprint(request: &ProofCaptureRequest) -> Result<String, DecapodError> {
    if request.evidence.is_empty() || request.evidence.len() > 64 {
        return Err(DecapodError::ValidationError(
            "proof capture requires between 1 and 64 evidence references".into(),
        ));
    }
    let mut h = Sha256::new();
    h.update(b"propodus.proof-capture.fingerprint.v1");
    let s = &request.subject;
    let mut values = vec![
        request.contract.clone(),
        request.contract_digest.clone(),
        request.operation_id.clone(),
        s.repository_id.clone(),
        s.task_id.clone(),
        s.workspace_id.clone(),
        s.custody_id.clone(),
        s.task_revision.to_string(),
        s.fencing_generation.to_string(),
        s.candidate.commit.clone(),
        s.candidate.tree.clone(),
        s.accepted_target.commit.clone(),
        s.accepted_target.tree.clone(),
    ];
    if let Some(integration) = &s.integration {
        values.push("present".to_string());
        values.push(integration.commit.clone());
        values.push(integration.tree.clone());
    } else {
        values.push("absent".to_string());
        values.push(String::new());
        values.push(String::new());
    }
    values.push(s.policy_digest.clone());
    values.push(s.requirements_digest.clone());
    values.push(s.material_digest.clone());
    values.push(s.prerequisites_digest.clone());
    values.push(request.evidence.len().to_string());
    for value in &values {
        proof_frame(&mut h, value);
    }

    let mut refs = request.evidence.clone();
    refs.sort_by(|a, b| a.kind.cmp(&b.kind).then_with(|| a.digest.cmp(&b.digest)));
    let mut seen_digests = std::collections::BTreeSet::new();
    for r in &refs {
        if !seen_digests.insert(r.digest.clone()) {
            return Err(DecapodError::ValidationError(
                "duplicate evidence digest in proof capture".into(),
            ));
        }
        proof_frame(&mut h, &r.kind);
        proof_frame(&mut h, &r.digest);
        proof_frame(&mut h, &r.bytes.to_string());
        proof_frame(&mut h, &r.provenance_digest);
    }
    let sum = h.finalize();
    let mut hex = String::with_capacity(7 + 64);
    hex.push_str("sha256:");
    for b in sum {
        use std::fmt::Write;
        let _ = write!(&mut hex, "{b:02x}");
    }
    Ok(hex)
}

fn proof_frame(h: &mut Sha256, value: &str) {
    let len = (value.len() as u64).to_be_bytes();
    h.update(len);
    h.update(value.as_bytes());
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CustodyBindRequest {
    pub project_id: String,
    pub operation_id: String,
    pub task_id: String,
    pub workspace_id: String,
    pub custody_id: String,
    pub native_task_version: i64,
    pub ttl_seconds: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CustodyRevokeRequest {
    pub project_id: String,
    pub operation_id: String,
    pub task_id: String,
    pub expected_fencing_generation: i64,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CustodyBindReceipt {
    pub task_revision: i64,
    pub fencing_generation: i64,
    pub expires_at: String,
}

pub trait ProofControlTransport {
    fn write_control(
        &self,
        op_name: &str,
        payload: &JsonValue,
    ) -> Result<JsonValue, CloudControlError>;
    fn read_control(
        &self,
        op_name: &str,
        payload: &JsonValue,
    ) -> Result<JsonValue, CloudControlError>;
}

/// Live `ProofControlTransport` backed by `dactyl-db 0.18.0`
/// `CloudConnector::control_request` (`OperationKind::Write` for mutations and
/// `OperationKind::Read` for read-only reconciliation/reconstruction).
#[cfg(feature = "supabase-cloud")]
pub struct DactylProofControlTransport {
    connector: dactyl_db::CloudConnector,
    bearer: String,
}

#[cfg(feature = "supabase-cloud")]
impl DactylProofControlTransport {
    pub fn new(connector: dactyl_db::CloudConnector, bearer: String) -> Self {
        Self { connector, bearer }
    }

    pub fn from_context(
        project_root: &std::path::Path,
        identity: &crate::core::repo_identity::RepositoryIdentity,
    ) -> Result<(Self, crate::core::repo_identity::RepositoryIdentity), DecapodError> {
        let (connector, routed, bearer) =
            crate::core::cloud_client::connect_control(project_root, identity)?;
        Ok((Self::new(connector, bearer), routed))
    }
}

#[cfg(feature = "supabase-cloud")]
fn map_dactyl_control_error(err: dactyl_db::DactylError) -> CloudControlError {
    use dactyl_db::AdapterErrorKind;
    let code = err.adapter_code().unwrap_or("control_error").to_string();
    let detail = err.to_string();
    if code == "outcome_unknown" {
        return CloudControlError {
            kind: CloudControlDenialKind::OutcomeUnknown,
            http_status: None,
            code,
            detail,
        };
    }
    if code == "proof_acceptance_limit"
        || code == "payload_too_large"
        || code == "parameter_budget_exceeded"
    {
        return CloudControlError {
            kind: CloudControlDenialKind::PayloadTooLarge,
            http_status: Some(413),
            code,
            detail,
        };
    }
    if code == "proof_acceptance_not_found" {
        return CloudControlError {
            kind: CloudControlDenialKind::InvalidRequest,
            http_status: Some(404),
            code,
            detail,
        };
    }
    match err.adapter_kind() {
        Some(AdapterErrorKind::Authorization) => CloudControlError {
            kind: CloudControlDenialKind::EntitlementForbidden,
            http_status: Some(403),
            code,
            detail,
        },
        Some(AdapterErrorKind::Quota | AdapterErrorKind::RateLimited) => CloudControlError {
            kind: CloudControlDenialKind::QuotaRateLimited,
            http_status: Some(429),
            code,
            detail,
        },
        Some(AdapterErrorKind::Authentication) => CloudControlError {
            kind: CloudControlDenialKind::EntitlementForbidden,
            http_status: Some(401),
            code,
            detail,
        },
        Some(AdapterErrorKind::Conflict | AdapterErrorKind::VersionConflict) => CloudControlError {
            kind: CloudControlDenialKind::Conflict,
            http_status: Some(409),
            code,
            detail,
        },
        Some(
            AdapterErrorKind::Timeout | AdapterErrorKind::Transport | AdapterErrorKind::Unavailable,
        ) => CloudControlError {
            kind: CloudControlDenialKind::OutcomeUnknown,
            http_status: Some(503),
            code,
            detail,
        },
        _ => CloudControlError {
            kind: CloudControlDenialKind::InvalidRequest,
            http_status: Some(400),
            code,
            detail,
        },
    }
}

#[cfg(feature = "supabase-cloud")]
impl ProofControlTransport for DactylProofControlTransport {
    fn write_control(
        &self,
        op_name: &str,
        payload: &JsonValue,
    ) -> Result<JsonValue, CloudControlError> {
        self.connector
            .control_request(
                op_name,
                payload,
                Some(&self.bearer),
                dactyl_db::OperationKind::Write,
                dactyl_db::ResourceBudget::default(),
            )
            .map_err(map_dactyl_control_error)
    }

    fn read_control(
        &self,
        op_name: &str,
        payload: &JsonValue,
    ) -> Result<JsonValue, CloudControlError> {
        self.connector
            .control_request(
                op_name,
                payload,
                Some(&self.bearer),
                dactyl_db::OperationKind::Read,
                dactyl_db::ResourceBudget::default(),
            )
            .map_err(map_dactyl_control_error)
    }
}

pub fn custody_bind<T: ProofControlTransport>(
    transport: &T,
    req: &CustodyBindRequest,
) -> Result<CustodyBindReceipt, DecapodError> {
    let payload = json!({
        "version": 1,
        "action": "custody_bind",
        "project_id": req.project_id,
        "operation_id": req.operation_id,
        "task_id": req.task_id,
        "workspace_id": req.workspace_id,
        "custody_id": req.custody_id,
        "native_task_version": req.native_task_version,
        "ttl_seconds": req.ttl_seconds.clamp(60, 86400),
    });
    let resp = transport
        .write_control(PROOF_ACCEPTANCE_CONTROL_OP, &payload)
        .map_err(|err| err.to_decapod_error(Some(&req.operation_id)))?;
    Ok(CustodyBindReceipt {
        task_revision: resp
            .get("task_revision")
            .and_then(|v| v.as_i64())
            .ok_or_else(|| {
                DecapodError::ValidationError(
                    "missing task_revision in custody_bind receipt".into(),
                )
            })?,
        fencing_generation: resp
            .get("fencing_generation")
            .and_then(|v| v.as_i64())
            .ok_or_else(|| {
                DecapodError::ValidationError(
                    "missing fencing_generation in custody_bind receipt".into(),
                )
            })?,
        expires_at: resp
            .get("expires_at")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string(),
    })
}

pub fn custody_revoke<T: ProofControlTransport>(
    transport: &T,
    req: &CustodyRevokeRequest,
) -> Result<JsonValue, DecapodError> {
    let payload = json!({
        "version": 1,
        "action": "custody_revoke",
        "project_id": req.project_id,
        "operation_id": req.operation_id,
        "task_id": req.task_id,
        "expected_fencing_generation": req.expected_fencing_generation,
        "reason": req.reason,
    });
    transport
        .write_control(PROOF_ACCEPTANCE_CONTROL_OP, &payload)
        .map_err(|err| err.to_decapod_error(Some(&req.operation_id)))
}

pub fn submit_capture<T: ProofControlTransport>(
    transport: &T,
    project_id: &str,
    native_task_version: i64,
    submission: &ProofCaptureRequest,
) -> Result<JsonValue, DecapodError> {
    let sub_json = serde_json::to_string(submission).map_err(|e| {
        DecapodError::ValidationError(format!("failed serializing proof submission: {e}"))
    })?;
    let payload = json!({
        "version": 1,
        "action": "capture",
        "project_id": project_id,
        "operation_id": submission.operation_id,
        "native_task_version": native_task_version,
        "submission": sub_json,
    });
    match transport.write_control(PROOF_ACCEPTANCE_CONTROL_OP, &payload) {
        Ok(receipt) => Ok(receipt),
        Err(err) if err.kind == CloudControlDenialKind::OutcomeUnknown => {
            reconcile_operation_after_unknown(
                transport,
                project_id,
                &submission.operation_id,
                Some(&proof_capture_fingerprint(submission)?),
            )
        }
        Err(err) => Err(err.to_decapod_error(Some(&submission.operation_id))),
    }
}

#[derive(Debug, Clone)]
pub struct VerifyProofTransitionRequest {
    pub project_id: String,
    pub operation_id: String,
    pub attestation_id: String,
    pub expected_fingerprint: String,
    pub expected_candidate_commit: String,
    pub expected_candidate_tree: String,
    pub verified_evidence_digest: String,
    pub verifier_scope: String,
}

pub fn submit_verify<T: ProofControlTransport>(
    transport: &T,
    req: &VerifyProofTransitionRequest,
) -> Result<JsonValue, DecapodError> {
    let payload = json!({
        "version": 1,
        "action": "verify",
        "project_id": req.project_id,
        "operation_id": req.operation_id,
        "attestation_id": req.attestation_id,
        "expected_fingerprint": req.expected_fingerprint,
        "expected_candidate_commit": req.expected_candidate_commit,
        "expected_candidate_tree": req.expected_candidate_tree,
        "verified_evidence_digest": req.verified_evidence_digest,
        "verifier_scope": req.verifier_scope,
    });
    match transport.write_control(PROOF_ACCEPTANCE_CONTROL_OP, &payload) {
        Ok(receipt) => Ok(receipt),
        Err(err) if err.kind == CloudControlDenialKind::OutcomeUnknown => {
            reconcile_operation_after_unknown(
                transport,
                &req.project_id,
                &req.operation_id,
                Some(&req.expected_fingerprint),
            )
        }
        Err(err) => Err(err.to_decapod_error(Some(&req.operation_id))),
    }
}

#[derive(Debug, Clone)]
pub struct AdmitOrPublishProofRequest {
    pub project_id: String,
    pub operation_id: String,
    pub attestation_id: String,
    pub expected_fingerprint: String,
    pub expected_task_revision: i64,
    pub expected_fencing_generation: i64,
    pub expected_candidate_commit: String,
    pub expected_accepted_target_commit: String,
    pub expected_policy_digest: String,
}

pub fn submit_admit_or_publish<T: ProofControlTransport>(
    transport: &T,
    action: &str,
    req: &AdmitOrPublishProofRequest,
) -> Result<JsonValue, DecapodError> {
    if action != "admit" && action != "publish" {
        return Err(DecapodError::ValidationError(format!(
            "unsupported proof admission action {action:?}"
        )));
    }
    let payload = json!({
        "version": 1,
        "action": action,
        "project_id": req.project_id,
        "operation_id": req.operation_id,
        "attestation_id": req.attestation_id,
        "expected_fingerprint": req.expected_fingerprint,
        "expected_task_revision": req.expected_task_revision,
        "expected_fencing_generation": req.expected_fencing_generation,
        "expected_candidate_commit": req.expected_candidate_commit,
        "expected_accepted_target_commit": req.expected_accepted_target_commit,
        "expected_policy_digest": req.expected_policy_digest,
    });
    match transport.write_control(PROOF_ACCEPTANCE_CONTROL_OP, &payload) {
        Ok(receipt) => Ok(receipt),
        Err(err) if err.kind == CloudControlDenialKind::OutcomeUnknown => {
            reconcile_operation_after_unknown(
                transport,
                &req.project_id,
                &req.operation_id,
                Some(&req.expected_fingerprint),
            )
        }
        Err(err) => Err(err.to_decapod_error(Some(&req.operation_id))),
    }
}

pub fn submit_retire_historical<T: ProofControlTransport>(
    transport: &T,
    project_id: &str,
    operation_id: &str,
    attestation_id: &str,
    expected_fingerprint: &str,
    reason: &str,
) -> Result<JsonValue, DecapodError> {
    let payload = json!({
        "version": 1,
        "action": "retire_historical",
        "project_id": project_id,
        "operation_id": operation_id,
        "attestation_id": attestation_id,
        "expected_fingerprint": expected_fingerprint,
        "reason": reason,
    });
    match transport.write_control(PROOF_ACCEPTANCE_CONTROL_OP, &payload) {
        Ok(receipt) => Ok(receipt),
        Err(err) if err.kind == CloudControlDenialKind::OutcomeUnknown => {
            reconcile_operation_after_unknown(
                transport,
                project_id,
                operation_id,
                Some(expected_fingerprint),
            )
        }
        Err(err) => Err(err.to_decapod_error(Some(operation_id))),
    }
}

/// Read-only reconciliation of an uncertain write using `get_operation` rather
/// than blind replay.
pub fn reconcile_operation_after_unknown<T: ProofControlTransport>(
    transport: &T,
    project_id: &str,
    operation_id: &str,
    expected_fingerprint: Option<&str>,
) -> Result<JsonValue, DecapodError> {
    let mut payload = json!({
        "version": 1,
        "action": "get_operation",
        "project_id": project_id,
        "operation_id": operation_id,
    });
    if let Some(fp) = expected_fingerprint
        && let Some(obj) = payload.as_object_mut()
    {
        obj.insert("expected_fingerprint".to_string(), json!(fp));
    }
    let lookup = match transport.read_control(PROOF_ACCEPTANCE_READ_OP, &payload) {
        Ok(val) => val,
        Err(err) if err.http_status == Some(404) || err.code == "proof_acceptance_not_found" => {
            return Err(DecapodError::ValidationError(format!(
                "CLOUD_OUTCOME_UNKNOWN (operation_id={operation_id}): operation not found during reconciliation; retry safely only after checking task revision"
            )));
        }
        Err(err) => return Err(err.to_decapod_error(Some(operation_id))),
    };
    if lookup.get("found").and_then(|v| v.as_bool()) == Some(true)
        && let Some(receipt) = lookup.get("receipt")
    {
        return Ok(receipt.clone());
    }
    if lookup.get("state").is_some()
        || lookup.get("attestation_id").is_some()
        || lookup.get("fencing_generation").is_some()
    {
        return Ok(lookup);
    }
    Err(DecapodError::ValidationError(format!(
        "CLOUD_OUTCOME_UNKNOWN (operation_id={operation_id}): operation not found during reconciliation; retry safely only after checking task revision"
    )))
}

pub fn reconstruct_attestation<T: ProofControlTransport>(
    transport: &T,
    project_id: &str,
    attestation_id: Option<&str>,
    task_id: Option<&str>,
) -> Result<JsonValue, DecapodError> {
    let mut payload = json!({
        "version": 1,
        "action": "reconstruct",
        "project_id": project_id,
    });
    if let Some(obj) = payload.as_object_mut() {
        if let Some(att_id) = attestation_id {
            obj.insert("attestation_id".to_string(), json!(att_id));
        }
        if let Some(tid) = task_id {
            obj.insert("task_id".to_string(), json!(tid));
        }
    }
    transport
        .read_control(PROOF_ACCEPTANCE_READ_OP, &payload)
        .map_err(|err| err.to_decapod_error(attestation_id.or(task_id)))
}

/// Bind a hosted logical task custody fence via `dactyl-db 0.18.0`
/// `CloudConnector::control_request("propodus_proof_acceptance_v1", ..., Write)`.
#[cfg(feature = "supabase-cloud")]
pub fn bind_hosted_custody_fence(
    project_root: &std::path::Path,
    identity: &crate::core::repo_identity::RepositoryIdentity,
    task_id: &str,
    workspace_id: &str,
    custody_id: &str,
    native_task_version: i64,
    ttl_seconds: i64,
) -> Result<CustodyBindReceipt, DecapodError> {
    let (transport, routed) = DactylProofControlTransport::from_context(project_root, identity)?;
    custody_bind(
        &transport,
        &CustodyBindRequest {
            project_id: routed.canonical_name,
            operation_id: new_uuid_v4()?,
            task_id: task_id.to_string(),
            workspace_id: workspace_id.to_string(),
            custody_id: custody_id.to_string(),
            native_task_version,
            ttl_seconds,
        },
    )
}

/// Revoke a hosted logical task custody fence via `dactyl-db 0.18.0`
/// `CloudConnector::control_request("propodus_proof_acceptance_v1", ..., Write)`.
#[cfg(feature = "supabase-cloud")]
pub fn revoke_hosted_custody_fence(
    project_root: &std::path::Path,
    identity: &crate::core::repo_identity::RepositoryIdentity,
    task_id: &str,
    expected_fencing_generation: i64,
    reason: &str,
) -> Result<JsonValue, DecapodError> {
    let (transport, routed) = DactylProofControlTransport::from_context(project_root, identity)?;
    custody_revoke(
        &transport,
        &CustodyRevokeRequest {
            project_id: routed.canonical_name,
            operation_id: new_uuid_v4()?,
            task_id: task_id.to_string(),
            expected_fencing_generation,
            reason: reason.to_string(),
        },
    )
}

/// Generate a valid RFC 4122/9562 UUID v4 string from OS randomness (`ring`),
/// matching Propodus's strict UUID regex (`[1-8]` version nibble, `[89ab]` variant).
pub fn new_uuid_v4() -> Result<String, DecapodError> {
    use ring::rand::{SecureRandom, SystemRandom};
    let mut bytes = [0u8; 16];
    SystemRandom::new()
        .fill(&mut bytes)
        .map_err(|_| DecapodError::ValidationError("failed generating random uuid".into()))?;
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    Ok(format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        bytes[0],
        bytes[1],
        bytes[2],
        bytes[3],
        bytes[4],
        bytes[5],
        bytes[6],
        bytes[7],
        bytes[8],
        bytes[9],
        bytes[10],
        bytes[11],
        bytes[12],
        bytes[13],
        bytes[14],
        bytes[15]
    ))
}

#[cfg(test)]
#[path = "../../../tests/unit/core/cloud_proof_tests.rs"]
mod tests;
