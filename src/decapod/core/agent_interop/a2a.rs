//! Bounded A2A Agent Card and governed task/handoff envelope adapter (issue #873).
//!
//! Advertises ONLY executable governance skills, reports core A2A and Decapod extension
//! conformance separately, and validates bounded task/handoff envelopes while keeping
//! repository-local custody, proof, and authority strictly authoritative.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// A2A adapter schema version.
pub const A2A_CARD_SCHEMA_VERSION: &str = "1.0.0";
/// Supported A2A protocol version for bounded card/handoff exchange.
pub const A2A_PROTOCOL_VERSION: &str = "0.2.0";

/// Executable governance skill advertised in Decapod's A2A Agent Card.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct A2aSkillDescriptor {
    pub id: String,
    pub name: String,
    pub description: String,
    pub read_only: bool,
    pub backing_surface: String,
    pub input_modes: Vec<String>,
    pub output_modes: Vec<String>,
}

/// Explicit separation between core A2A semantics and Decapod governance extensions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct A2aConformanceLayers {
    pub core_a2a_protocol_version: String,
    pub core_a2a_supported_operations: Vec<String>,
    pub decapod_extension_uri: String,
    pub decapod_extension_required_by_core: bool,
    pub decapod_extensions: Vec<String>,
    pub note: String,
}

/// Bounded A2A Agent Card describing only implemented local governance skills.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct A2aAgentCard {
    pub schema_version: String,
    pub protocol_version: String,
    pub name: String,
    pub description: String,
    pub version: String,
    pub transport_mode: String,
    pub daemonless_local_compatible: bool,
    pub authentication_modes: Vec<String>,
    pub authority_boundary: String,
    pub skills: Vec<A2aSkillDescriptor>,
    pub conformance_layers: A2aConformanceLayers,
}

/// Build the truthful A2A Agent Card advertising only executable local skills.
pub fn builtin_agent_card() -> A2aAgentCard {
    A2aAgentCard {
        schema_version: A2A_CARD_SCHEMA_VERSION.to_string(),
        protocol_version: A2A_PROTOCOL_VERSION.to_string(),
        name: "decapod-governance-kernel".to_string(),
        description: "Local-first repo-native governance kernel exposing bounded task orientation, handoff validation, overlap classification, and proof-status inspection.".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        transport_mode: "bounded-cli-envelope".to_string(),
        daemonless_local_compatible: true,
        authentication_modes: vec![
            "local-session-token".to_string(),
            "read-only-unauthenticated".to_string(),
        ],
        authority_boundary: "An Agent Card is descriptive discovery only. It never confers trust, imports remote task ownership, or overrides repository-local custody and proof gates.".to_string(),
        skills: vec![
            A2aSkillDescriptor {
                id: "governance.orient".to_string(),
                name: "Governed Orientation".to_string(),
                description: "Inspect repository manifest, context bindings, and negotiation compatibility.".to_string(),
                read_only: true,
                backing_surface: "rpc:interop.manifest,rpc:interop.negotiate".to_string(),
                input_modes: vec!["application/json".to_string()],
                output_modes: vec!["application/json".to_string()],
            },
            A2aSkillDescriptor {
                id: "governance.handoff.validate".to_string(),
                name: "Handoff Envelope Validation".to_string(),
                description: "Validate bounded A2A task/handoff artifacts for custody continuity, freshness, unresolved obligations, and digest integrity.".to_string(),
                read_only: true,
                backing_surface: "cli:decapod a2a handoff validate".to_string(),
                input_modes: vec!["application/json".to_string()],
                output_modes: vec!["application/json".to_string()],
            },
            A2aSkillDescriptor {
                id: "governance.overlap.classify".to_string(),
                name: "Semantic Task Overlap Classification".to_string(),
                description: "Classify concurrent task-scope declarations into 6 semantic overlap states without false serialization.".to_string(),
                read_only: true,
                backing_surface: "rpc:interop.overlap.classify".to_string(),
                input_modes: vec!["application/json".to_string()],
                output_modes: vec!["application/json".to_string()],
            },
            A2aSkillDescriptor {
                id: "governance.rules.evaluate".to_string(),
                name: "Coding Rule Applicability Evaluation".to_string(),
                description: "Evaluate typed rule metadata, conflict precedence, and prompt-injection/escalation rejection.".to_string(),
                read_only: true,
                backing_surface: "rpc:interop.rules.evaluate".to_string(),
                input_modes: vec!["application/json".to_string()],
                output_modes: vec!["application/json".to_string()],
            },
        ],
        conformance_layers: A2aConformanceLayers {
            core_a2a_protocol_version: A2A_PROTOCOL_VERSION.to_string(),
            core_a2a_supported_operations: vec![
                "agent_card.get".to_string(),
                "task.handoff.validate".to_string(),
                "task.artifact.inspect".to_string(),
            ],
            decapod_extension_uri: "urn:decapod:a2a:extensions:governance:v1".to_string(),
            decapod_extension_required_by_core: false,
            decapod_extensions: vec![
                "custody_lease_generation".to_string(),
                "interlock_references".to_string(),
                "proof_lineage_commit_binding".to_string(),
                "local_authority_isolation".to_string(),
            ],
            note: "Core A2A artifact framing is reported separately from Decapod governance extensions; remote agents need not implement Decapod extensions to inspect core fields, but local admission always evaluates Decapod extensions.".to_string(),
        },
    }
}

/// A2A task lifecycle states aligned with governed Decapod task states.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum A2aTaskState {
    Submitted,
    Working,
    InputRequired,
    Completed,
    Canceled,
    Failed,
    Unknown,
}

/// Decapod extension payload carried alongside core A2A handoff fields.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DecapodA2aExtension {
    pub repository_id: String,
    pub expected_head_sha: String,
    pub expected_base_sha: String,
    pub lease_generation: u64,
    pub owner_agent_id: String,
    pub intent_anchor: String,
    pub unresolved_questions: Vec<String>,
    pub pending_obligations: Vec<String>,
    pub interlock_refs: Vec<String>,
    pub context_bundle_sha256: String,
    pub proof_commit_sha: Option<String>,
    pub claims_local_authority: bool,
}

/// Bounded A2A task/handoff envelope exchanged across independent agents.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct A2aHandoffEnvelope {
    pub schema_version: String,
    pub protocol_version: String,
    pub handoff_id: String,
    pub task_id: String,
    pub task_state: A2aTaskState,
    pub sender_agent_id: String,
    pub receiver_agent_id: String,
    pub summary: String,
    pub dependencies: Vec<String>,
    pub created_at_epoch: u64,
    pub expires_at_epoch: u64,
    pub decapod_extension: DecapodA2aExtension,
    /// Canonical SHA-256 over handoff content fields.
    pub content_sha256: String,
}

impl A2aHandoffEnvelope {
    /// Compute canonical content digest over all fields except `content_sha256`.
    pub fn compute_content_sha256(&self) -> String {
        let mut clone = self.clone();
        clone.content_sha256 = String::new();
        let canonical = serde_json::to_vec(&clone).unwrap_or_default();
        let mut hasher = Sha256::new();
        hasher.update(canonical);
        format!("sha256:{:x}", hasher.finalize())
    }

    /// Populate `content_sha256` deterministically.
    pub fn with_computed_digest(mut self) -> Self {
        self.content_sha256 = self.compute_content_sha256();
        self
    }
}

/// Local repository ground truth used to verify an incoming A2A handoff.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalHandoffContext {
    pub repository_id: String,
    pub current_head_sha: String,
    pub current_base_sha: String,
    pub current_owner_agent_id: String,
    pub current_lease_generation: u64,
    pub expected_receiver_agent_id: String,
    pub now_epoch: u64,
}

/// Structured result of validating an A2A handoff envelope.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct A2aHandoffValidationReport {
    pub core_a2a_valid: bool,
    pub decapod_extension_valid: bool,
    pub admissible_for_continuation: bool,
    pub imported_authority_granted: bool,
    pub reason_codes: Vec<String>,
    pub preserved_unresolved_questions: Vec<String>,
    pub preserved_pending_obligations: Vec<String>,
    pub authority_note: String,
}

/// Validate an A2A handoff envelope fail-closed against local repository ground truth.
/// Core A2A structure and Decapod governance extensions are reported separately.
pub fn validate_a2a_handoff(
    envelope: &A2aHandoffEnvelope,
    local: &LocalHandoffContext,
) -> A2aHandoffValidationReport {
    let mut core_errors = Vec::new();
    let mut ext_errors = Vec::new();

    // 1. Core A2A framing checks.
    if envelope.schema_version != A2A_CARD_SCHEMA_VERSION {
        core_errors.push("unsupported_schema_version".to_string());
    }
    if envelope.protocol_version != A2A_PROTOCOL_VERSION {
        core_errors.push("unsupported_a2a_protocol_version".to_string());
    }
    if envelope.handoff_id.trim().is_empty() || envelope.task_id.trim().is_empty() {
        core_errors.push("missing_task_or_handoff_id".to_string());
    }
    if envelope.receiver_agent_id != local.expected_receiver_agent_id {
        core_errors.push("wrong_handoff_recipient".to_string());
    }
    if envelope.expires_at_epoch <= envelope.created_at_epoch
        || local.now_epoch > envelope.expires_at_epoch
    {
        core_errors.push("handoff_expired".to_string());
    }
    let expected_digest = envelope.compute_content_sha256();
    if envelope.content_sha256 != expected_digest {
        core_errors.push("handoff_digest_tampered".to_string());
    }
    if matches!(
        envelope.task_state,
        A2aTaskState::Canceled | A2aTaskState::Unknown
    ) {
        core_errors.push("non_continuable_task_state".to_string());
    }

    // 2. Decapod extension custody, freshness, and authority-isolation checks.
    let ext = &envelope.decapod_extension;
    if ext.repository_id != local.repository_id {
        ext_errors.push("cross_repository_handoff_denied".to_string());
    }
    if ext.owner_agent_id != local.current_owner_agent_id {
        ext_errors.push("contradictory_task_ownership".to_string());
    }
    if ext.lease_generation < local.current_lease_generation {
        ext_errors.push("stale_custody_lease_generation".to_string());
    }
    if ext.expected_head_sha != local.current_head_sha {
        ext_errors.push("stale_head_commit".to_string());
    }
    if ext.expected_base_sha != local.current_base_sha {
        ext_errors.push("moved_accepted_base".to_string());
    }
    if let Some(ref proof_sha) = ext.proof_commit_sha
        && proof_sha != &local.current_head_sha
    {
        ext_errors.push("stale_proof_commit".to_string());
    }
    if ext.intent_anchor.trim().is_empty() {
        ext_errors.push("missing_intent_anchor".to_string());
    }
    if !ext.context_bundle_sha256.starts_with("sha256:")
        || ext.context_bundle_sha256.len() != 71
        || !ext.context_bundle_sha256[7..]
            .bytes()
            .all(|b| b.is_ascii_hexdigit())
    {
        ext_errors.push("invalid_context_bundle_digest".to_string());
    }
    if ext.claims_local_authority {
        ext_errors.push("imported_authority_claim_denied".to_string());
    }

    let core_a2a_valid = core_errors.is_empty();
    let decapod_extension_valid = ext_errors.is_empty();
    let mut reason_codes = core_errors;
    reason_codes.extend(ext_errors);

    A2aHandoffValidationReport {
        core_a2a_valid,
        decapod_extension_valid,
        admissible_for_continuation: core_a2a_valid && decapod_extension_valid,
        // Handoff validation NEVER imports authority automatically.
        imported_authority_granted: false,
        reason_codes,
        preserved_unresolved_questions: ext.unresolved_questions.clone(),
        preserved_pending_obligations: ext.pending_obligations.clone(),
        authority_note: "Handoff artifacts remain external evidence until locally re-claimed under repository custody; no remote authority is imported.".to_string(),
    }
}
