//! Decapod RPC Interface
//!
//! This module implements the agent-native JSON-RPC interface for Decapod.
//! Agents communicate with Decapod via structured JSON messages over stdin/stdout.
//!
//! # Standard Response Envelope
//!
//! Every RPC response returns:
//! - `receipt`: What happened, hashes, touched paths, governing anchors
//! - `context_capsule`: Minimal relevant spec/arch/security/standards slices
//! - `allowed_next_ops`: Contract for what to do next
//! - `blocked_by`: Missing answers/proofs

use crate::core::docs::{DocFragment, Mandate};
use serde::{Deserialize, Serialize};
use sha2::Digest;
use std::collections::HashMap;

/// Standard RPC request envelope
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RpcRequest {
    /// Operation to perform
    pub op: String,
    /// Operation parameters
    #[serde(default)]
    pub params: serde_json::Value,
    /// Request ID for correlation
    #[serde(default = "default_request_id")]
    pub id: String,
    /// Session token (optional, can use env var)
    #[serde(default)]
    pub session: Option<String>,
}

pub fn default_request_id() -> String {
    ulid::Ulid::new().to_string()
}

/// Standard RPC response envelope
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RpcResponse {
    /// Request ID for correlation
    pub id: String,
    /// Whether the operation succeeded
    pub success: bool,
    /// Receipt of what happened
    pub receipt: Receipt,
    /// Mandates governing this specific operation
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub mandates: Vec<Mandate>,
    /// Context capsule with relevant documentation slices
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_capsule: Option<ContextCapsule>,
    /// Result of the operation
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    /// Allowed next operations
    pub allowed_next_ops: Vec<AllowedOp>,
    /// Blockers preventing progress
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub blocked_by: Vec<Blocker>,
    /// Binding enforcement interlock
    #[serde(skip_serializing_if = "Option::is_none")]
    pub interlock: Option<Interlock>,
    /// Non-binding advisory guidance
    #[serde(skip_serializing_if = "Option::is_none")]
    pub advisory: Option<Advisory>,
    /// Structured evidence for this operation
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attestation: Option<Attestation>,
    /// Error details (if success is false)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<RpcError>,
}

/// Receipt documenting what happened
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Receipt {
    /// Operation performed
    pub op: String,
    /// Timestamp (ISO 8601)
    pub timestamp: String,
    /// Content hash of inputs
    pub inputs_hash: String,
    /// Content hash of outputs
    pub outputs_hash: String,
    /// Paths touched by the operation
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub touched_paths: Vec<String>,
    /// Governing anchors (rules that governed this operation)
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub governing_anchors: Vec<String>,
}

/// Context capsule containing relevant documentation
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ContextCapsule {
    /// Relevant fragments from the constitution/authority docs
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub fragments: Vec<DocFragment>,
    /// Relevant spec slices
    #[serde(skip_serializing_if = "Option::is_none")]
    pub spec: Option<String>,
    /// Relevant architecture slices
    #[serde(skip_serializing_if = "Option::is_none")]
    pub architecture: Option<String>,
    /// Relevant security slices
    #[serde(skip_serializing_if = "Option::is_none")]
    pub security: Option<String>,
    /// Resolved standards applicable to this operation
    #[serde(skip_serializing_if = "Option::is_none")]
    pub standards: Option<HashMap<String, serde_json::Value>>,
}

/// Allowed next operation
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AllowedOp {
    /// Operation name
    pub op: String,
    /// Why this is allowed
    pub reason: String,
    /// Required parameters
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub required_params: Vec<String>,
}

/// Blocker preventing operation completion
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Blocker {
    /// Blocker type
    pub kind: BlockerKind,
    /// Human-readable description
    pub message: String,
    /// How to resolve
    pub resolve_hint: String,
}

/// Types of blockers
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BlockerKind {
    MissingAnswer,
    MissingProof,
    Unauthorized,
    Conflict,
    ValidationFailed,
    WorkspaceRequired,
    ProtectedBranch,
}

/// RPC error details
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RpcError {
    /// Error code
    pub code: String,
    /// Error message
    pub message: String,
    /// Additional context
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

/// Capabilities report for agent discovery
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CapabilitiesReport {
    /// Decapod version
    pub version: String,
    /// Capabilities offered
    pub capabilities: Vec<Capability>,
    /// Subsystems available
    pub subsystems: Vec<SubsystemInfo>,
    /// Workspace features
    pub workspace: WorkspaceCapabilities,
    /// Interview features
    pub interview: InterviewCapabilities,
    /// Stable interlock codes exposed by the assurance harness
    pub interlock_codes: Vec<String>,
}

/// Individual capability
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Capability {
    /// Capability name
    pub name: String,
    /// Description
    pub description: String,
    /// Stability: stable, beta, alpha
    pub stability: String,
    /// Cost metric (relative)
    pub cost: String,
}

/// Subsystem information
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SubsystemInfo {
    /// Subsystem name
    pub name: String,
    /// Status: active, deprecated
    pub status: String,
    /// Operations supported
    pub ops: Vec<String>,
}

/// Workspace capabilities
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WorkspaceCapabilities {
    /// Whether workspace enforcement is available
    pub enforcement_available: bool,
    /// Whether docker execution is available
    pub docker_available: bool,
    /// Protected branch patterns
    pub protected_patterns: Vec<String>,
}

/// Interview capabilities
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct InterviewCapabilities {
    /// Whether interview engine is available
    pub available: bool,
    /// Artifact types that can be generated
    pub artifact_types: Vec<String>,
    /// Standards resolution available
    pub standards_resolution: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Interlock {
    pub code: String,
    pub message: String,
    pub unblock_ops: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub evidence: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct EvidenceRef {
    pub source: String,
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hash: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ReconciliationPointer {
    pub kind: String,
    pub r#ref: String,
    pub title: String,
    pub why_short: String,
    pub evidence: EvidenceRef,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ReconciliationSets {
    pub must: Vec<ReconciliationPointer>,
    pub recommended: Vec<ReconciliationPointer>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct VerificationPlan {
    pub required: Vec<String>,
    pub checklist: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LoopSignal {
    pub code: String,
    pub message: String,
    pub suggested_redirect_ops: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Advisory {
    pub reconciliations: ReconciliationSets,
    pub verification_plan: VerificationPlan,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub loop_signal: Option<LoopSignal>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<Vec<String>>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Attestation {
    pub id: String,
    pub op: String,
    pub timestamp: String,
    pub input_hash: String,
    pub touched_paths: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub interlock_code: Option<String>,
    pub outcome: String,
    pub trace_path: String,
}

/// Generate capabilities report
pub fn generate_capabilities() -> CapabilitiesReport {
    CapabilitiesReport {
        version: env!("CARGO_PKG_VERSION").to_string(),
        capabilities: vec![
            Capability {
                name: "daemonless".to_string(),
                description: "Decapod never runs in the background; it is invoked by agents"
                    .to_string(),
                stability: "stable".to_string(),
                cost: "none".to_string(),
            },
            Capability {
                name: "deterministic".to_string(),
                description: "Same inputs produce identical outputs given fixed repo state"
                    .to_string(),
                stability: "stable".to_string(),
                cost: "none".to_string(),
            },
            Capability {
                name: "context.resolve".to_string(),
                description: "Resolve relevant constitution/authority fragments for an operation"
                    .to_string(),
                stability: "stable".to_string(),
                cost: "low".to_string(),
            },
            Capability {
                name: "context.scope".to_string(),
                description:
                    "Return scoped, query-matched constitution fragments for just-in-time context"
                        .to_string(),
                stability: "stable".to_string(),
                cost: "low".to_string(),
            },
            Capability {
                name: "schema.get".to_string(),
                description: "Get authoritative JSON schemas for entities".to_string(),
                stability: "stable".to_string(),
                cost: "low".to_string(),
            },
            Capability {
                name: "store.upsert".to_string(),
                description: "Deterministic storage for decisions/knowledge/todos".to_string(),
                stability: "stable".to_string(),
                cost: "medium".to_string(),
            },
            Capability {
                name: "store.query".to_string(),
                description: "Retrieve canonical entities deterministically".to_string(),
                stability: "stable".to_string(),
                cost: "medium".to_string(),
            },
            Capability {
                name: "validate.run".to_string(),
                description: "Run deterministic validation gates".to_string(),
                stability: "stable".to_string(),
                cost: "medium".to_string(),
            },
            Capability {
                name: "workspace.ensure".to_string(),
                description: "Create or enter an isolated agent workspace".to_string(),
                stability: "stable".to_string(),
                cost: "low".to_string(),
            },
            Capability {
                name: "workspace.status".to_string(),
                description: "Check current workspace and branch status".to_string(),
                stability: "stable".to_string(),
                cost: "low".to_string(),
            },
        ],
        subsystems: vec![
            SubsystemInfo {
                name: "todo".to_string(),
                status: "active".to_string(),
                ops: vec![
                    "add".to_string(),
                    "claim".to_string(),
                    "done".to_string(),
                    "list".to_string(),
                ],
            },
            SubsystemInfo {
                name: "knowledge".to_string(),
                status: "active".to_string(),
                ops: vec!["add".to_string(), "search".to_string()],
            },
            SubsystemInfo {
                name: "federation".to_string(),
                status: "active".to_string(),
                ops: vec!["add".to_string(), "get".to_string(), "graph".to_string()],
            },
            SubsystemInfo {
                name: "lcm".to_string(),
                status: "active".to_string(),
                ops: vec![
                    "ingest".to_string(),
                    "list".to_string(),
                    "show".to_string(),
                    "summarize".to_string(),
                    "summary".to_string(),
                    "schema".to_string(),
                    "rebuild".to_string(),
                ],
            },
            SubsystemInfo {
                name: "map".to_string(),
                status: "active".to_string(),
                ops: vec![
                    "llm".to_string(),
                    "agentic".to_string(),
                    "schema".to_string(),
                ],
            },
        ],
        workspace: WorkspaceCapabilities {
            enforcement_available: true,
            docker_available: std::env::var("DECAPOD_CONTAINER_RUNTIME_DISABLED").is_err(),
            protected_patterns: vec![
                "main".to_string(),
                "master".to_string(),
                "production".to_string(),
                "release/*".to_string(),
            ],
        },
        interview: InterviewCapabilities {
            available: true,
            artifact_types: vec![
                "spec".to_string(),
                "architecture".to_string(),
                "security".to_string(),
                "ops".to_string(),
                "adr".to_string(),
            ],
            standards_resolution: true,
        },
        interlock_codes: vec![
            "workspace_required".to_string(),
            "verification_required".to_string(),
            "store_boundary_violation".to_string(),
            "decision_required".to_string(),
        ],
    }
}

/// Create a successful response
#[allow(clippy::too_many_arguments)]
pub fn success_response(
    request_id: String,
    op: String,
    params: serde_json::Value,
    result: Option<serde_json::Value>,
    touched_paths: Vec<String>,
    context_capsule: Option<ContextCapsule>,
    allowed_next_ops: Vec<AllowedOp>,
    mandates: Vec<Mandate>,
) -> RpcResponse {
    let timestamp = crate::core::time::now_epoch_z();

    let inputs_hash = format!(
        "{:x}",
        sha2::Sha256::digest(serde_json::to_string(&params).unwrap_or_default())
    );
    let outputs_hash = format!(
        "{:x}",
        sha2::Sha256::digest(serde_json::to_string(&result).unwrap_or_default())
    );

    RpcResponse {
        id: request_id,
        success: true,
        receipt: Receipt {
            op,
            timestamp,
            inputs_hash,
            outputs_hash,
            touched_paths,
            governing_anchors: mandates.iter().map(|m| m.fragment.r#ref.clone()).collect(),
        },
        mandates,
        context_capsule,
        result,
        allowed_next_ops,
        blocked_by: vec![],
        interlock: None,
        advisory: None,
        attestation: None,
        error: None,
    }
}

/// Create an error response
pub fn error_response(
    request_id: String,
    op: String,
    params: serde_json::Value,
    code: String,
    message: String,
    blocker: Option<Blocker>,
    mandates: Vec<Mandate>,
) -> RpcResponse {
    let timestamp = crate::core::time::now_epoch_z();
    let inputs_hash = format!(
        "{:x}",
        sha2::Sha256::digest(serde_json::to_string(&params).unwrap_or_default())
    );
    let outputs_hash = format!("{:x}", sha2::Sha256::digest("error"));

    let blocked_by = if let Some(b) = blocker {
        vec![b]
    } else {
        vec![]
    };

    RpcResponse {
        id: request_id,
        success: false,
        receipt: Receipt {
            op,
            timestamp,
            inputs_hash,
            outputs_hash,
            touched_paths: vec![],
            governing_anchors: mandates.iter().map(|m| m.fragment.r#ref.clone()).collect(),
        },
        mandates,
        context_capsule: None,
        result: None,
        allowed_next_ops: vec![AllowedOp {
            op: "agent.init".to_string(),
            reason: "Session may be invalid or expired".to_string(),
            required_params: vec![],
        }],
        blocked_by,
        interlock: None,
        advisory: None,
        attestation: None,
        error: Some(RpcError {
            code,
            message,
            details: None,
        }),
    }
}
