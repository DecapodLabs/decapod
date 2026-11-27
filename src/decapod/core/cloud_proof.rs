//! Cloud candidate proof acceptance lifecycle and hosted lease-fence client (#1809, #1038).
//!
//! Implements the public Decapod Cloud v1 commit-bound proof acceptance lifecycle:
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
pub struct CustodyGetRequest {
    pub project_id: String,
    pub task_id: String,
    pub workspace_id: String,
    pub custody_id: String,
    pub task_revision: i64,
    pub fencing_generation: i64,
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
    if resp.get("operation_id") != payload.get("operation_id") {
        return Err(invalid_custody_receipt());
    }
    custody_receipt(&resp, &payload, "custody_bound")
}

/// Check a previously bound fence against current hosted repository authority.
/// A historical mutation receipt cannot establish that the fence is still live.
pub fn custody_get<T: ProofControlTransport>(
    transport: &T,
    req: &CustodyGetRequest,
) -> Result<CustodyBindReceipt, DecapodError> {
    if req.task_revision <= 0 || req.fencing_generation <= 0 {
        return Err(DecapodError::ValidationError(
            "CLOUD_CUSTODY_FENCE_INVALID: expected revision and fencing generation must be positive"
                .into(),
        ));
    }
    let payload = json!({
        "version": 1,
        "action": "get_custody",
        "project_id": req.project_id,
        "task_id": req.task_id,
        "workspace_id": req.workspace_id,
        "custody_id": req.custody_id,
        "task_revision": req.task_revision,
        "fencing_generation": req.fencing_generation,
    });
    let resp = transport
        .read_control(PROOF_ACCEPTANCE_READ_OP, &payload)
        .map_err(|mut err| {
            if err.kind == CloudControlDenialKind::OutcomeUnknown {
                err.kind = CloudControlDenialKind::Unavailable;
            }
            err.to_decapod_error(None)
        })?;
    let receipt = custody_receipt(&resp, &payload, "custody_current")?;
    if receipt.task_revision != req.task_revision
        || receipt.fencing_generation != req.fencing_generation
    {
        return Err(invalid_custody_receipt());
    }
    Ok(receipt)
}

fn invalid_custody_receipt() -> DecapodError {
    DecapodError::ValidationError(
        "CLOUD_CUSTODY_RECEIPT_INVALID: hosted custody response is missing, stale, or mismatched"
            .into(),
    )
}

fn custody_receipt(
    response: &JsonValue,
    request: &JsonValue,
    status: &str,
) -> Result<CustodyBindReceipt, DecapodError> {
    if response["version"] != 1
        || response["profile"] != "candidate-acceptance-v1"
        || response["status"] != status
        || ["project_id", "task_id", "workspace_id", "custody_id"]
            .iter()
            .any(|key| response.get(*key) != request.get(*key))
    {
        return Err(invalid_custody_receipt());
    }
    let positive = |key| {
        response[key]
            .as_i64()
            .filter(|value| *value > 0)
            .ok_or_else(invalid_custody_receipt)
    };
    positive("authority_generation")?;
    let expires_at = response["expires_at"]
        .as_str()
        .filter(|value| {
            crate::core::time::parse_rfc3339(value)
                .is_ok_and(|expires| expires > time::OffsetDateTime::now_utc())
        })
        .ok_or_else(invalid_custody_receipt)?;
    Ok(CustodyBindReceipt {
        task_revision: positive("task_revision")?,
        fencing_generation: positive("fencing_generation")?,
        expires_at: expires_at.to_owned(),
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

fn invalid_proof_receipt(detail: &str) -> DecapodError {
    DecapodError::ValidationError(format!(
        "CLOUD_PROOF_RECEIPT_INVALID: hosted proof receipt is missing, failed, or mismatched ({detail})"
    ))
}

fn receipt_lifecycle_state(receipt: &JsonValue) -> Option<&str> {
    receipt
        .get("state")
        .or_else(|| receipt.get("status"))
        .and_then(|v| v.as_str())
}

fn assert_optional_str_match(
    receipt: &JsonValue,
    key: &str,
    expected: &str,
) -> Result<(), DecapodError> {
    if let Some(value) = receipt.get(key)
        && !value.is_null()
        && value.as_str() != Some(expected)
    {
        return Err(invalid_proof_receipt(&format!("{key} mismatch")));
    }
    Ok(())
}

fn assert_optional_i64_match(
    receipt: &JsonValue,
    key: &str,
    expected: i64,
) -> Result<(), DecapodError> {
    if let Some(value) = receipt.get(key)
        && !value.is_null()
        && value.as_i64() != Some(expected)
    {
        return Err(invalid_proof_receipt(&format!("{key} mismatch")));
    }
    Ok(())
}

fn assert_required_str_match(
    receipt: &JsonValue,
    key: &str,
    expected: &str,
) -> Result<(), DecapodError> {
    if receipt.get(key).and_then(|v| v.as_str()) != Some(expected) {
        return Err(invalid_proof_receipt(&format!("expected {key}")));
    }
    Ok(())
}

fn assert_required_i64_match(
    receipt: &JsonValue,
    key: &str,
    expected: i64,
) -> Result<(), DecapodError> {
    if receipt.get(key).and_then(|v| v.as_i64()) != Some(expected) {
        return Err(invalid_proof_receipt(&format!("expected {key}")));
    }
    Ok(())
}

fn assert_not_escalated_true(receipt: &JsonValue, key: &str) -> Result<(), DecapodError> {
    if receipt.get(key).and_then(|v| v.as_bool()) == Some(true) {
        return Err(invalid_proof_receipt(&format!(
            "unexpected privilege escalation flag {key}=true"
        )));
    }
    Ok(())
}

fn assert_required_bool(
    receipt: &JsonValue,
    key: &str,
    expected: bool,
) -> Result<(), DecapodError> {
    if receipt.get(key).and_then(|v| v.as_bool()) != Some(expected) {
        return Err(invalid_proof_receipt(&format!("expected {key}={expected}")));
    }
    Ok(())
}

fn assert_candidate_acceptance_envelope(
    receipt: &JsonValue,
    project_id: &str,
    operation_id: &str,
    attestation_id: &str,
    expected_fingerprint: &str,
    expected_state: &str,
) -> Result<(), DecapodError> {
    if !receipt.is_object() {
        return Err(invalid_proof_receipt("receipt is not an object"));
    }
    assert_required_i64_match(receipt, "version", 1)?;
    assert_required_str_match(receipt, "profile", "candidate-acceptance-v1")?;
    if receipt_lifecycle_state(receipt) != Some(expected_state) {
        return Err(invalid_proof_receipt(&format!(
            "expected state {expected_state}"
        )));
    }
    assert_optional_str_match(receipt, "project_id", project_id)?;
    assert_required_str_match(receipt, "operation_id", operation_id)?;
    assert_required_str_match(receipt, "attestation_id", attestation_id)?;
    assert_required_str_match(receipt, "fingerprint", expected_fingerprint)?;
    assert_not_escalated_true(receipt, "forge_merge_authority")?;
    Ok(())
}

fn verify_capture_receipt(
    receipt: &JsonValue,
    project_id: &str,
    submission: &ProofCaptureRequest,
    expected_fingerprint: &str,
) -> Result<(), DecapodError> {
    if !receipt.is_object() {
        return Err(invalid_proof_receipt("capture receipt is not an object"));
    }
    if !matches!(
        receipt_lifecycle_state(receipt),
        Some("captured" | "recorded_unverified")
    ) {
        return Err(invalid_proof_receipt(
            "capture receipt must report captured or recorded_unverified state",
        ));
    }
    assert_optional_str_match(receipt, "project_id", project_id)?;
    assert_optional_str_match(receipt, "operation_id", &submission.operation_id)?;
    assert_optional_str_match(receipt, "capture_operation_id", &submission.operation_id)?;
    let op_bound = [
        receipt.get("operation_id"),
        receipt.get("capture_operation_id"),
        receipt.get("attestation_id"),
    ]
    .into_iter()
    .flatten()
    .filter_map(|v| v.as_str())
    .any(|v| v == submission.operation_id);
    if !op_bound {
        return Err(invalid_proof_receipt(
            "capture receipt does not bind expected operation_id",
        ));
    }
    assert_optional_str_match(receipt, "task_id", &submission.subject.task_id)?;
    assert_optional_str_match(receipt, "workspace_id", &submission.subject.workspace_id)?;
    assert_optional_str_match(receipt, "custody_id", &submission.subject.custody_id)?;
    assert_optional_i64_match(receipt, "task_revision", submission.subject.task_revision)?;
    assert_optional_i64_match(
        receipt,
        "fencing_generation",
        submission.subject.fencing_generation,
    )?;
    assert_optional_str_match(receipt, "fingerprint", expected_fingerprint)?;
    assert_optional_str_match(
        receipt,
        "source_commit",
        &submission.subject.candidate.commit,
    )?;
    assert_optional_str_match(
        receipt,
        "policy_revision",
        &submission.subject.policy_digest,
    )?;
    assert_optional_str_match(receipt, "policy_digest", &submission.subject.policy_digest)?;
    if let Some(cand) = receipt.get("candidate").filter(|v| !v.is_null()) {
        assert_optional_str_match(cand, "commit", &submission.subject.candidate.commit)?;
        assert_optional_str_match(cand, "tree", &submission.subject.candidate.tree)?;
    }
    if let Some(target) = receipt.get("accepted_target").filter(|v| !v.is_null()) {
        assert_optional_str_match(target, "commit", &submission.subject.accepted_target.commit)?;
        assert_optional_str_match(target, "tree", &submission.subject.accepted_target.tree)?;
    }
    for flag in [
        "logical_custody_verified",
        "acceptance_supported",
        "publication_eligible",
        "forge_merge_authority",
    ] {
        assert_not_escalated_true(receipt, flag)?;
    }
    Ok(())
}

fn execute_proof_transition<T: ProofControlTransport, F>(
    transport: &T,
    project_id: &str,
    operation_id: &str,
    expected_fingerprint: Option<&str>,
    payload: &JsonValue,
    validate: F,
) -> Result<JsonValue, DecapodError>
where
    F: FnOnce(&JsonValue) -> Result<(), DecapodError>,
{
    let receipt = match transport.write_control(PROOF_ACCEPTANCE_CONTROL_OP, payload) {
        Ok(receipt) => receipt,
        Err(err) if err.kind == CloudControlDenialKind::OutcomeUnknown => {
            reconcile_operation_after_unknown(
                transport,
                project_id,
                operation_id,
                expected_fingerprint,
            )?
        }
        Err(err) => return Err(err.to_decapod_error(Some(operation_id))),
    };
    validate(&receipt)?;
    Ok(receipt)
}

pub fn submit_capture<T: ProofControlTransport>(
    transport: &T,
    project_id: &str,
    native_task_version: i64,
    submission: &ProofCaptureRequest,
) -> Result<JsonValue, DecapodError> {
    if submission.subject.repository_id != project_id {
        return Err(invalid_proof_receipt(
            "submission subject repository_id disagrees with target project_id",
        ));
    }
    let fingerprint = proof_capture_fingerprint(submission)?;
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
    execute_proof_transition(
        transport,
        project_id,
        &submission.operation_id,
        Some(&fingerprint),
        &payload,
        |receipt| verify_capture_receipt(receipt, project_id, submission, &fingerprint),
    )
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
    execute_proof_transition(
        transport,
        &req.project_id,
        &req.operation_id,
        Some(&req.expected_fingerprint),
        &payload,
        |receipt| {
            assert_candidate_acceptance_envelope(
                receipt,
                &req.project_id,
                &req.operation_id,
                &req.attestation_id,
                &req.expected_fingerprint,
                ProofLifecycleState::IndependentlyVerified.as_str(),
            )?;
            let candidate = receipt
                .get("candidate")
                .filter(|v| v.is_object())
                .ok_or_else(|| invalid_proof_receipt("expected candidate identity"))?;
            assert_required_str_match(candidate, "commit", &req.expected_candidate_commit)?;
            assert_required_str_match(candidate, "tree", &req.expected_candidate_tree)?;
            assert_required_str_match(
                receipt,
                "verified_evidence_digest",
                &req.verified_evidence_digest,
            )?;
            assert_required_bool(receipt, "references_verified", true)?;
            for flag in [
                "logical_custody_verified",
                "acceptance_supported",
                "publication_eligible",
            ] {
                assert_not_escalated_true(receipt, flag)?;
            }
            Ok(())
        },
    )
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
    let expected_state = match action {
        "admit" => ProofLifecycleState::CurrentlyAdmissible.as_str(),
        "publish" => ProofLifecycleState::Published.as_str(),
        _ => {
            return Err(DecapodError::ValidationError(format!(
                "unsupported proof admission action {action:?}"
            )));
        }
    };
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
    execute_proof_transition(
        transport,
        &req.project_id,
        &req.operation_id,
        Some(&req.expected_fingerprint),
        &payload,
        |receipt| {
            assert_candidate_acceptance_envelope(
                receipt,
                &req.project_id,
                &req.operation_id,
                &req.attestation_id,
                &req.expected_fingerprint,
                expected_state,
            )?;
            assert_required_i64_match(receipt, "task_revision", req.expected_task_revision)?;
            assert_required_i64_match(
                receipt,
                "fencing_generation",
                req.expected_fencing_generation,
            )?;
            let candidate = receipt
                .get("candidate")
                .filter(|v| v.is_object())
                .ok_or_else(|| invalid_proof_receipt("expected candidate identity"))?;
            assert_required_str_match(candidate, "commit", &req.expected_candidate_commit)?;
            let target = receipt
                .get("accepted_target")
                .filter(|v| v.is_object())
                .ok_or_else(|| invalid_proof_receipt("expected accepted_target identity"))?;
            assert_required_str_match(target, "commit", &req.expected_accepted_target_commit)?;
            assert_required_str_match(receipt, "policy_digest", &req.expected_policy_digest)?;
            assert_required_bool(receipt, "references_verified", true)?;
            assert_required_bool(receipt, "logical_custody_verified", true)?;
            assert_required_bool(receipt, "acceptance_supported", true)?;
            assert_required_bool(receipt, "publication_eligible", action == "publish")?;
            Ok(())
        },
    )
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
    execute_proof_transition(
        transport,
        project_id,
        operation_id,
        Some(expected_fingerprint),
        &payload,
        |receipt| {
            assert_candidate_acceptance_envelope(
                receipt,
                project_id,
                operation_id,
                attestation_id,
                expected_fingerprint,
                ProofLifecycleState::HistoricallyAccepted.as_str(),
            )?;
            assert_required_bool(receipt, "publication_eligible", false)
        },
    )
}

/// Read-only reconciliation of an uncertain write using `get_operation` rather
/// than blind replay, failing closed on missing/mismatched/failed receipts.
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
    if lookup.get("found").and_then(|v| v.as_bool()) == Some(false) {
        return Err(DecapodError::ValidationError(format!(
            "CLOUD_OUTCOME_UNKNOWN (operation_id={operation_id}): operation not found during reconciliation; retry safely only after checking task revision"
        )));
    }
    let receipt = if lookup.get("found").and_then(|v| v.as_bool()) == Some(true) {
        lookup
            .get("receipt")
            .filter(|v| v.is_object())
            .cloned()
            .ok_or_else(|| {
                invalid_proof_receipt("get_operation found=true without receipt object")
            })?
    } else if lookup.is_object()
        && (lookup.get("state").is_some()
            || lookup.get("status").is_some()
            || lookup.get("attestation_id").is_some()
            || lookup.get("fencing_generation").is_some())
    {
        lookup
    } else {
        return Err(DecapodError::ValidationError(format!(
            "CLOUD_OUTCOME_UNKNOWN (operation_id={operation_id}): operation not found during reconciliation; retry safely only after checking task revision"
        )));
    };
    if !matches!(
        receipt_lifecycle_state(&receipt),
        Some(
            "captured"
                | "recorded_unverified"
                | "independently_verified"
                | "currently_admissible"
                | "published"
                | "historically_accepted"
                | "custody_bound"
                | "custody_current"
                | "custody_revoked"
        )
    ) {
        return Err(invalid_proof_receipt(
            "reconciled receipt has missing or disallowed lifecycle state",
        ));
    }
    assert_optional_str_match(&receipt, "project_id", project_id)?;
    assert_optional_str_match(&receipt, "operation_id", operation_id)?;
    let op_bound = [
        receipt.get("operation_id"),
        receipt.get("capture_operation_id"),
        receipt.get("attestation_id"),
    ]
    .into_iter()
    .flatten()
    .filter_map(|v| v.as_str())
    .any(|v| v == operation_id);
    if !op_bound {
        return Err(invalid_proof_receipt(
            "reconciled receipt does not match requested operation_id",
        ));
    }
    if let Some(fp) = expected_fingerprint {
        assert_optional_str_match(&receipt, "fingerprint", fp)?;
        assert_optional_str_match(&receipt, "expected_fingerprint", fp)?;
    }
    assert_not_escalated_true(&receipt, "forge_merge_authority")?;
    Ok(receipt)
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
