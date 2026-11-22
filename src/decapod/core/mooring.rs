//! Houseboat 5 (#855): Bounded collaboration and mooring contracts.
//!
//! Typed, expiring collaboration contracts binding repositories, workunits,
//! agents, external artifacts, or integrations to explicit allowed/forbidden
//! operations, data boundaries, capability grants, provenance, verification
//! obligations, and auditable disengagement/revocation records.
//!
//! Fail-closed guarantees:
//! - Expired, revoked, disengaged, forbidden, or over-scoped operations fail closed.
//! - Explicit forbidden operations always override allowed operations.
//! - External artifacts cannot bypass verification by altering wrapper metadata
//!   while keeping or disguising unverified content digests.
//! - Revocation and disengagement are idempotent and preserved in the audit trail.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

pub const MOORING_CONTRACT_SCHEMA_VERSION: &str = "decapod.mooring-contract.v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MooringSubjectKind {
    Repository,
    Workunit,
    Agent,
    ExternalArtifact,
    Integration,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MooringOperation {
    Read,
    ProposePatch,
    RunCheck,
    ImportArtifact,
    Publish,
    Admin,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContractState {
    Active,
    Expired,
    Revoked,
    Disengaged,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DisengagementKind {
    Disengaged,
    Revoked,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum MooringDenialCode {
    ContractExpired,
    ContractRevoked,
    ContractDisengaged,
    ForbiddenOperation,
    OperationNotGranted,
    OutOfDataBoundary,
    MissingCapabilityGrant,
    UnverifiedExternalArtifact,
    ProvenanceTampered,
    MissingApprovingAuthority,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactProvenance {
    pub artifact_id: String,
    pub source_uri: String,
    /// Canonical cryptographic SHA-256 of the underlying artifact content.
    pub content_sha256: String,
    /// Cryptographic SHA-256 of the wrapper/transport metadata envelope.
    pub wrapper_sha256: String,
    /// Verification obligations required before import or publication.
    pub verification_obligations: BTreeSet<String>,
    /// Verification obligations actually verified against `content_sha256`.
    pub verified_obligations: BTreeSet<String>,
    pub approved_by: Option<String>,
}

impl ArtifactProvenance {
    pub fn missing_obligations(&self) -> Vec<String> {
        self.verification_obligations
            .difference(&self.verified_obligations)
            .cloned()
            .collect()
    }

    pub fn is_fully_verified(&self) -> bool {
        !self.verification_obligations.is_empty() && self.missing_obligations().is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DisengagementRecord {
    pub contract_id: String,
    pub kind: DisengagementKind,
    pub actor: String,
    pub reason: String,
    pub recorded_at_epoch: u64,
    pub evidence_digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MooringAuditEntry {
    pub seq: u64,
    pub action: String,
    pub actor: String,
    pub recorded_at_epoch: u64,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CollaborationContract {
    pub schema_version: String,
    pub contract_id: String,
    pub subject_kind: MooringSubjectKind,
    pub subject_id: String,
    pub allowed_operations: BTreeSet<MooringOperation>,
    pub forbidden_operations: BTreeSet<MooringOperation>,
    pub data_boundaries: Vec<String>,
    pub capability_grants: BTreeSet<String>,
    pub issued_at_epoch: u64,
    pub expires_at_epoch: u64,
    pub approving_authority: String,
    pub state: ContractState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provenance: Option<ArtifactProvenance>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disengagement: Option<DisengagementRecord>,
    #[serde(default)]
    pub audit_trail: Vec<MooringAuditEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MooringEnforcementRequest {
    pub operation: MooringOperation,
    pub target_path: Option<String>,
    pub required_capability: Option<String>,
    pub now_epoch: u64,
    /// Observed content digest when verifying an external artifact boundary.
    pub observed_content_sha256: Option<String>,
    /// Observed wrapper digest to detect wrapper-metadata tampering.
    pub observed_wrapper_sha256: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MooringVerdict {
    pub allowed: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub denial_code: Option<MooringDenialCode>,
    pub reason: String,
    pub contract_id: String,
    pub contract_digest: String,
    pub effective_state: ContractState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MooringProvenanceSummary {
    pub contract_id: String,
    pub contract_digest: String,
    pub subject_kind: MooringSubjectKind,
    pub subject_id: String,
    pub state: ContractState,
    pub expires_at_epoch: u64,
    pub approving_authority: String,
    pub artifact_verified: bool,
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("sha256:{:x}", hasher.finalize())
}

fn normalize_boundary_path(raw: &str) -> Option<String> {
    let trimmed = raw.trim().replace('\\', "/");
    if trimmed.is_empty() || trimmed.starts_with('/') {
        return None;
    }
    let mut parts = Vec::new();
    for segment in trimmed.split('/') {
        if segment.is_empty() || segment == "." {
            continue;
        }
        if segment == ".." {
            return None;
        }
        parts.push(segment);
    }
    if parts.is_empty() {
        return None;
    }
    Some(parts.join("/"))
}

fn path_within_boundaries(target: &str, boundaries: &[String]) -> bool {
    let Some(norm_target) = normalize_boundary_path(target) else {
        return false;
    };
    boundaries.iter().any(|boundary| {
        if boundary == "." || boundary == "*" {
            return true;
        }
        let Some(norm_boundary) = normalize_boundary_path(boundary) else {
            return false;
        };
        norm_target == norm_boundary || norm_target.starts_with(&format!("{norm_boundary}/"))
    })
}

impl CollaborationContract {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        contract_id: impl Into<String>,
        subject_kind: MooringSubjectKind,
        subject_id: impl Into<String>,
        allowed_operations: impl IntoIterator<Item = MooringOperation>,
        forbidden_operations: impl IntoIterator<Item = MooringOperation>,
        data_boundaries: impl IntoIterator<Item = String>,
        capability_grants: impl IntoIterator<Item = String>,
        issued_at_epoch: u64,
        expires_at_epoch: u64,
        approving_authority: impl Into<String>,
        provenance: Option<ArtifactProvenance>,
    ) -> Self {
        let contract_id = contract_id.into();
        let approving_authority = approving_authority.into();
        let audit_trail = vec![MooringAuditEntry {
            seq: 1,
            action: "contract_issued".to_string(),
            actor: approving_authority.clone(),
            recorded_at_epoch: issued_at_epoch,
            detail: format!("expires_at={expires_at_epoch}"),
        }];
        Self {
            schema_version: MOORING_CONTRACT_SCHEMA_VERSION.to_string(),
            contract_id,
            subject_kind,
            subject_id: subject_id.into(),
            allowed_operations: allowed_operations.into_iter().collect(),
            forbidden_operations: forbidden_operations.into_iter().collect(),
            data_boundaries: data_boundaries.into_iter().collect(),
            capability_grants: capability_grants.into_iter().collect(),
            issued_at_epoch,
            expires_at_epoch,
            approving_authority,
            state: ContractState::Active,
            provenance,
            disengagement: None,
            audit_trail,
        }
    }

    pub fn canonical_digest(&self) -> String {
        let payload = serde_json::to_vec(&(
            &self.schema_version,
            &self.contract_id,
            &self.subject_kind,
            &self.subject_id,
            &self.allowed_operations,
            &self.forbidden_operations,
            &self.data_boundaries,
            &self.capability_grants,
            self.issued_at_epoch,
            self.expires_at_epoch,
            &self.approving_authority,
            &self.provenance,
        ))
        .unwrap_or_default();
        sha256_hex(&payload)
    }

    pub fn effective_state(&self, now_epoch: u64) -> ContractState {
        match self.state {
            ContractState::Revoked => ContractState::Revoked,
            ContractState::Disengaged => ContractState::Disengaged,
            ContractState::Expired => ContractState::Expired,
            ContractState::Active => {
                if now_epoch >= self.expires_at_epoch {
                    ContractState::Expired
                } else {
                    ContractState::Active
                }
            }
        }
    }

    pub fn enforce(&self, req: &MooringEnforcementRequest) -> MooringVerdict {
        let digest = self.canonical_digest();
        let effective = self.effective_state(req.now_epoch);

        let deny = |code: MooringDenialCode, reason: String| MooringVerdict {
            allowed: false,
            denial_code: Some(code),
            reason,
            contract_id: self.contract_id.clone(),
            contract_digest: digest.clone(),
            effective_state: effective,
        };

        if self.approving_authority.trim().is_empty() {
            return deny(
                MooringDenialCode::MissingApprovingAuthority,
                "contract lacks an explicit approving authority".to_string(),
            );
        }

        match effective {
            ContractState::Revoked => {
                return deny(
                    MooringDenialCode::ContractRevoked,
                    format!("contract '{}' has been revoked", self.contract_id),
                );
            }
            ContractState::Disengaged => {
                return deny(
                    MooringDenialCode::ContractDisengaged,
                    format!("contract '{}' has been disengaged", self.contract_id),
                );
            }
            ContractState::Expired => {
                return deny(
                    MooringDenialCode::ContractExpired,
                    format!(
                        "contract '{}' expired at {} (now={})",
                        self.contract_id, self.expires_at_epoch, req.now_epoch
                    ),
                );
            }
            ContractState::Active => {}
        }

        // Explicit deny always overrides allow.
        if self.forbidden_operations.contains(&req.operation) {
            return deny(
                MooringDenialCode::ForbiddenOperation,
                format!(
                    "operation '{:?}' is explicitly forbidden by contract '{}'",
                    req.operation, self.contract_id
                ),
            );
        }

        if !self.allowed_operations.contains(&req.operation) {
            return deny(
                MooringDenialCode::OperationNotGranted,
                format!(
                    "operation '{:?}' is not granted by contract '{}'",
                    req.operation, self.contract_id
                ),
            );
        }

        if let Some(target_path) = &req.target_path
            && !path_within_boundaries(target_path, &self.data_boundaries)
        {
            return deny(
                MooringDenialCode::OutOfDataBoundary,
                format!(
                    "path '{}' escapes data boundaries {:?}",
                    target_path, self.data_boundaries
                ),
            );
        }

        if let Some(cap) = &req.required_capability
            && !self.capability_grants.contains(cap)
        {
            return deny(
                MooringDenialCode::MissingCapabilityGrant,
                format!("capability '{}' not granted by contract", cap),
            );
        }

        // External artifact / publish boundary checks.
        let requires_artifact_proof = self.subject_kind == MooringSubjectKind::ExternalArtifact
            || matches!(
                req.operation,
                MooringOperation::ImportArtifact | MooringOperation::Publish
            );
        if requires_artifact_proof {
            if let Some(prov) = &self.provenance {
                // Wrapper tampering or content digest drift fails closed.
                if let Some(obs_content) = &req.observed_content_sha256
                    && obs_content != &prov.content_sha256
                {
                    return deny(
                        MooringDenialCode::ProvenanceTampered,
                        format!(
                            "artifact '{}' content digest mismatch (expected {}, observed {})",
                            prov.artifact_id, prov.content_sha256, obs_content
                        ),
                    );
                }
                if let Some(obs_wrapper) = &req.observed_wrapper_sha256
                    && obs_wrapper != &prov.wrapper_sha256
                {
                    return deny(
                        MooringDenialCode::ProvenanceTampered,
                        format!(
                            "artifact '{}' wrapper metadata changed without re-verification (expected {}, observed {})",
                            prov.artifact_id, prov.wrapper_sha256, obs_wrapper
                        ),
                    );
                }
                if !prov.is_fully_verified() {
                    let missing = prov.missing_obligations();
                    return deny(
                        MooringDenialCode::UnverifiedExternalArtifact,
                        format!(
                            "artifact '{}' missing required verification obligations: {:?}",
                            prov.artifact_id, missing
                        ),
                    );
                }
            } else if self.subject_kind == MooringSubjectKind::ExternalArtifact
                || req.operation == MooringOperation::ImportArtifact
            {
                return deny(
                    MooringDenialCode::UnverifiedExternalArtifact,
                    "external artifact operation requires bound ArtifactProvenance".to_string(),
                );
            }
        }

        MooringVerdict {
            allowed: true,
            denial_code: None,
            reason: format!(
                "operation '{:?}' permitted under contract '{}'",
                req.operation, self.contract_id
            ),
            contract_id: self.contract_id.clone(),
            contract_digest: digest,
            effective_state: effective,
        }
    }

    /// Idempotently revoke the contract and record revocation evidence.
    pub fn revoke(
        &mut self,
        actor: impl Into<String>,
        reason: impl Into<String>,
        now_epoch: u64,
    ) -> DisengagementRecord {
        if self.state == ContractState::Revoked
            && let Some(existing) = &self.disengagement
        {
            return existing.clone();
        }
        let actor = actor.into();
        let reason = reason.into();
        let evidence_digest = sha256_hex(
            format!(
                "revoke:{}:{}:{}:{}",
                self.contract_id, actor, reason, now_epoch
            )
            .as_bytes(),
        );
        let record = DisengagementRecord {
            contract_id: self.contract_id.clone(),
            kind: DisengagementKind::Revoked,
            actor: actor.clone(),
            reason: reason.clone(),
            recorded_at_epoch: now_epoch,
            evidence_digest,
        };
        self.state = ContractState::Revoked;
        self.disengagement = Some(record.clone());
        let next_seq = self.audit_trail.last().map_or(1, |e| e.seq + 1);
        self.audit_trail.push(MooringAuditEntry {
            seq: next_seq,
            action: "contract_revoked".to_string(),
            actor,
            recorded_at_epoch: now_epoch,
            detail: reason,
        });
        record
    }

    /// Idempotently disengage the contract cleanly (never downgrades a revocation).
    pub fn disengage(
        &mut self,
        actor: impl Into<String>,
        reason: impl Into<String>,
        now_epoch: u64,
    ) -> Result<DisengagementRecord, String> {
        if self.state == ContractState::Revoked {
            return Err(format!(
                "contract '{}' is already revoked and cannot transition to disengaged",
                self.contract_id
            ));
        }
        if self.state == ContractState::Disengaged
            && let Some(existing) = &self.disengagement
        {
            return Ok(existing.clone());
        }
        let actor = actor.into();
        let reason = reason.into();
        let evidence_digest = sha256_hex(
            format!(
                "disengage:{}:{}:{}:{}",
                self.contract_id, actor, reason, now_epoch
            )
            .as_bytes(),
        );
        let record = DisengagementRecord {
            contract_id: self.contract_id.clone(),
            kind: DisengagementKind::Disengaged,
            actor: actor.clone(),
            reason: reason.clone(),
            recorded_at_epoch: now_epoch,
            evidence_digest,
        };
        self.state = ContractState::Disengaged;
        self.disengagement = Some(record.clone());
        let next_seq = self.audit_trail.last().map_or(1, |e| e.seq + 1);
        self.audit_trail.push(MooringAuditEntry {
            seq: next_seq,
            action: "contract_disengaged".to_string(),
            actor,
            recorded_at_epoch: now_epoch,
            detail: reason,
        });
        Ok(record)
    }

    pub fn provenance_summary(&self, now_epoch: u64) -> MooringProvenanceSummary {
        MooringProvenanceSummary {
            contract_id: self.contract_id.clone(),
            contract_digest: self.canonical_digest(),
            subject_kind: self.subject_kind,
            subject_id: self.subject_id.clone(),
            state: self.effective_state(now_epoch),
            expires_at_epoch: self.expires_at_epoch,
            approving_authority: self.approving_authority.clone(),
            artifact_verified: self
                .provenance
                .as_ref()
                .map(ArtifactProvenance::is_fully_verified)
                .unwrap_or(true),
        }
    }
}

/// Deterministic in-memory registry of mooring contracts checked at command/RPC
/// and publication boundaries.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MooringRegistry {
    contracts: BTreeMap<String, CollaborationContract>,
}

impl MooringRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, contract: CollaborationContract) -> Result<(), String> {
        if contract.contract_id.trim().is_empty() {
            return Err("contract_id must not be empty".to_string());
        }
        if contract.expires_at_epoch <= contract.issued_at_epoch {
            return Err(format!(
                "contract '{}' must have expires_at_epoch > issued_at_epoch",
                contract.contract_id
            ));
        }
        if let Some(existing) = self.contracts.get(&contract.contract_id) {
            if existing.canonical_digest() == contract.canonical_digest() {
                return Ok(());
            }
            return Err(format!(
                "conflicting contract already registered for id '{}'",
                contract.contract_id
            ));
        }
        self.contracts
            .insert(contract.contract_id.clone(), contract);
        Ok(())
    }

    pub fn get(&self, contract_id: &str) -> Option<&CollaborationContract> {
        self.contracts.get(contract_id)
    }

    pub fn get_mut(&mut self, contract_id: &str) -> Option<&mut CollaborationContract> {
        self.contracts.get_mut(contract_id)
    }

    pub fn enforce_at_boundary(
        &self,
        contract_id: &str,
        req: &MooringEnforcementRequest,
    ) -> Result<MooringVerdict, String> {
        let contract = self
            .contracts
            .get(contract_id)
            .ok_or_else(|| format!("unknown mooring contract '{contract_id}'"))?;
        Ok(contract.enforce(req))
    }

    pub fn check_publish_gate(
        &self,
        contract_id: &str,
        target_path: Option<String>,
        now_epoch: u64,
        observed_content_sha256: Option<String>,
        observed_wrapper_sha256: Option<String>,
    ) -> Result<MooringVerdict, String> {
        self.enforce_at_boundary(
            contract_id,
            &MooringEnforcementRequest {
                operation: MooringOperation::Publish,
                target_path,
                required_capability: None,
                now_epoch,
                observed_content_sha256,
                observed_wrapper_sha256,
            },
        )
    }

    pub fn active_summaries(&self, now_epoch: u64) -> Vec<MooringProvenanceSummary> {
        self.contracts
            .values()
            .filter(|c| c.effective_state(now_epoch) == ContractState::Active)
            .map(|c| c.provenance_summary(now_epoch))
            .collect()
    }
}
