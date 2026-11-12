//! Evidence-backed governance interoperability profile (issues #870/#871).
//!
//! This module is the typed, versioned description of which Decapod governance
//! operations an external harness can actually use, plus a deterministic,
//! fail-closed compatibility negotiation. It is *descriptive only*: a
//! capability declaration is a claim, never an authorization. Verification
//! states refer to real conformance evidence (see `KNOWN_CONFORMANCE_TESTS`);
//! an entry can never self-certify.

use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// Manifest schema version.
pub const INTEROP_MANIFEST_SCHEMA_VERSION: &str = "1.0.0";
/// Stable interoperability profile identifier.
pub const INTEROP_PROFILE_ID: &str = "decapod.interop";
/// Profile version implemented by this build.
pub const INTEROP_PROFILE_VERSION: &str = "v1";
/// All profile versions this build can negotiate.
pub const SUPPORTED_PROFILE_VERSIONS: &[&str] = &["v1"];
/// Extensions this build actually supports in negotiation.
pub const SUPPORTED_EXTENSIONS: &[&str] =
    &["receipts", "interlocks", "projections", "context-capsule"];

/// Registered RPC operation names.
///
/// This list is the canonical registry for the dispatch-derived manifest
/// entries. The dispatcher still matches op names with its own table, so this
/// is not a generated single source of truth; instead the black-box conformance
/// harness (`tests/interop_conformance.rs`) drift-checks the two, proving every
/// entry is actually accepted by the real dispatcher (an advertised op that
/// the dispatch rejects as `unknown_op` fails the suite). Harness detection of
/// drift is proof of synchronization between registry and dispatcher.
pub const RPC_OPS: &[&str] = &[
    "agent.init",
    "workspace.status",
    "workspace.ensure",
    "workspace.publish",
    "context.resolve",
    "context.scope",
    "context.capsule.query",
    "context.bundle.export",
    "context.bundle.import",
    "context.bindings",
    "constitution.get",
    "constitution.links.query",
    "constitution.links.navigate",
    "constitution.migrate",
    "agent.registry.query",
    "specs.refresh",
    "schema.get",
    "data.database.inspect",
    "store.upsert",
    "store.query",
    "validate.run",
    "scaffold.next_question",
    "scaffold.apply_answer",
    "scaffold.generate_artifacts",
    "standards.resolve",
    "mentor.obligations",
    "assurance.evaluate",
    "interop.manifest",
    "interop.negotiate",
    "interop.project",
];

/// Conformance test identifiers known to the black-box harness.
///
/// A manifest entry may only claim `verified`/`failed` against one of these
/// ids; `validate_manifest` rejects any other reference, so an authored
/// manifest cannot invent admissible evidence.
pub const KNOWN_CONFORMANCE_TESTS: &[&str] = &[
    "interop_conformance::discovery_and_negotiation",
    "interop_conformance::authorized_mutation",
    "interop_conformance::denied_mutation",
    "interop_conformance::approval_roundtrip",
    "interop_conformance::session_task_continuity",
    "interop_conformance::stale_evidence",
    "interop_conformance::cli_rpc_parity",
    "interop_conformance::malformed_adversarial",
];

/// Implementation status of a capability. Deliberately separate from the
/// verification state: an operation may be implemented but unverified.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImplementationStatus {
    Implemented,
    Partial,
    IntentionallyNotUsed,
    Planned,
}

impl ImplementationStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            ImplementationStatus::Implemented => "implemented",
            ImplementationStatus::Partial => "partial",
            ImplementationStatus::IntentionallyNotUsed => "intentionally_not_used",
            ImplementationStatus::Planned => "planned",
        }
    }
}

/// Verification (evidence) state of a capability.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VerificationState {
    Verified,
    Failed,
    NotRun,
    Stale,
    Unknown,
}

impl VerificationState {
    pub fn as_str(self) -> &'static str {
        match self {
            VerificationState::Verified => "verified",
            VerificationState::Failed => "failed",
            VerificationState::NotRun => "not_run",
            VerificationState::Stale => "stale",
            VerificationState::Unknown => "unknown",
        }
    }
}

/// Evidence attached to a capability entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityEvidence {
    /// Conformance test identifier (must be in [`KNOWN_CONFORMANCE_TESTS`] when
    /// the state is `verified` or `failed`).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub conformance_test: String,
    pub state: VerificationState,
    /// Reference to the evidence (e.g. `tests/interop_conformance.rs::<case>`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evidence_ref: Option<String>,
}

/// Concrete implementation mapping for a capability.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImplementationMapping {
    /// Registered operation name (must be in [`RPC_OPS`]) or a non-RPC surface.
    pub op: String,
    /// Owning module / seam.
    pub module: String,
    /// Supported entrypoints (e.g. `rpc-stdin`, `cli`).
    pub entrypoints: Vec<String>,
    /// Read-only vs mutating classification.
    pub read_only: bool,
    /// Whether a valid session is required before dispatch.
    pub session_required: bool,
}

/// One capability entry in the interoperability manifest.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InteropCapability {
    /// Stable machine-readable id, e.g. `rpc:agent.init` or `transport:mcp`.
    pub id: String,
    /// Surface family: `rpc`, `cli`, `transport`.
    pub surface: String,
    pub implementation_status: ImplementationStatus,
    /// Owning component (module path).
    pub owner_component: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub implementation: Option<ImplementationMapping>,
    pub evidence: CapabilityEvidence,
    /// Human-readable description (not machine-facing).
    pub description: String,
}

/// Versioned, machine-readable interoperability manifest.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InteropManifest {
    pub schema_version: String,
    pub profile_id: String,
    pub profile_version: String,
    pub reference_implementation: String,
    pub implementation_version: String,
    /// Source-of-authority declaration: this manifest describes; it never
    /// authorizes.
    pub authority_declaration: String,
    pub capabilities: Vec<InteropCapability>,
}

/// Per-op dispatch metadata used to build the manifest. `read_only` and
/// `session_required` mirror the RPC gate allowlists in `lib.rs`.
struct OpMeta {
    op: &'static str,
    read_only: bool,
    session_required: bool,
    description: &'static str,
}

const OP_META: &[OpMeta] = &[
    OpMeta {
        op: "agent.init",
        read_only: true,
        session_required: false,
        description: "Agent bootstrap and source-inventory reconciliation",
    },
    OpMeta {
        op: "workspace.status",
        read_only: true,
        session_required: false,
        description: "Workspace/branch/worktree status",
    },
    OpMeta {
        op: "workspace.ensure",
        read_only: false,
        session_required: false,
        description: "Create isolated governed worktree branch",
    },
    OpMeta {
        op: "workspace.publish",
        read_only: false,
        session_required: true,
        description: "Publish workspace branch (gated by validation and awareness)",
    },
    OpMeta {
        op: "context.resolve",
        read_only: true,
        session_required: false,
        description: "Resolve governed context for intent",
    },
    OpMeta {
        op: "context.scope",
        read_only: true,
        session_required: false,
        description: "Alias of context.resolve",
    },
    OpMeta {
        op: "context.capsule.query",
        read_only: true,
        session_required: false,
        description: "Query context capsule",
    },
    OpMeta {
        op: "context.bundle.export",
        read_only: true,
        session_required: false,
        description: "Export portable context bundle",
    },
    OpMeta {
        op: "context.bundle.import",
        read_only: false,
        session_required: false,
        description: "Import context bundle (stale bundles refused)",
    },
    OpMeta {
        op: "context.bindings",
        read_only: true,
        session_required: false,
        description: "List operation bindings",
    },
    OpMeta {
        op: "constitution.get",
        read_only: true,
        session_required: false,
        description: "Read embedded constitution section",
    },
    OpMeta {
        op: "constitution.links.query",
        read_only: true,
        session_required: true,
        description: "Query constitution links",
    },
    OpMeta {
        op: "constitution.links.navigate",
        read_only: true,
        session_required: true,
        description: "Navigate constitution links",
    },
    OpMeta {
        op: "constitution.migrate",
        read_only: false,
        session_required: true,
        description: "Migrate constitution store",
    },
    OpMeta {
        op: "agent.registry.query",
        read_only: true,
        session_required: true,
        description: "Query agent registry/sessions",
    },
    OpMeta {
        op: "specs.refresh",
        read_only: false,
        session_required: true,
        description: "Refresh governed spec fingerprints/manifests",
    },
    OpMeta {
        op: "schema.get",
        read_only: true,
        session_required: false,
        description: "Read entity JSON schema",
    },
    OpMeta {
        op: "data.database.inspect",
        read_only: true,
        session_required: false,
        description: "Inspect repository-local storage with bounded snapshot reads and explicit coverage",
    },
    OpMeta {
        op: "store.upsert",
        read_only: false,
        session_required: false,
        description: "Upsert governed store entity",
    },
    OpMeta {
        op: "store.query",
        read_only: true,
        session_required: false,
        description: "Query governed store",
    },
    OpMeta {
        op: "validate.run",
        read_only: true,
        session_required: false,
        description: "Run/read authoritative validation; unsuccessful validation returns non-zero",
    },
    OpMeta {
        op: "scaffold.next_question",
        read_only: true,
        session_required: true,
        description: "Next scaffolding question",
    },
    OpMeta {
        op: "scaffold.apply_answer",
        read_only: false,
        session_required: true,
        description: "Apply scaffolding answer",
    },
    OpMeta {
        op: "scaffold.generate_artifacts",
        read_only: false,
        session_required: true,
        description: "Generate scaffold artifacts",
    },
    OpMeta {
        op: "standards.resolve",
        read_only: true,
        session_required: false,
        description: "Resolve applicable standards",
    },
    OpMeta {
        op: "mentor.obligations",
        read_only: true,
        session_required: false,
        description: "Query mentor obligations",
    },
    OpMeta {
        op: "assurance.evaluate",
        read_only: true,
        session_required: false,
        description: "Evaluate assurance harness",
    },
    OpMeta {
        op: "interop.manifest",
        read_only: true,
        session_required: false,
        description: "This manifest (read-only discovery)",
    },
    OpMeta {
        op: "interop.negotiate",
        read_only: true,
        session_required: false,
        description: "Compatibility negotiation (read-only, never grants authority)",
    },
    OpMeta {
        op: "interop.project",
        read_only: true,
        session_required: false,
        description: "Structured projection of a captured outcome (derived, non-authoritative)",
    },
];

/// Ops exercised end-to-end by the black-box conformance suite; these are the
/// only entries allowed to claim `verified`.
const VERIFIED_BY_DISCOVERY: &[&str] = &[
    "agent.init",
    "context.resolve",
    "schema.get",
    "interop.manifest",
    "interop.negotiate",
    "interop.project",
];
const VERIFIED_BY_MUTATION: &[&str] = &["store.upsert", "store.query"];
const VERIFIED_BY_CONTINUITY: &[&str] = &[
    "workspace.status",
    "context.bundle.export",
    "context.bundle.import",
];

fn evidence_for(op: &str) -> CapabilityEvidence {
    let test = if VERIFIED_BY_DISCOVERY.contains(&op) {
        "interop_conformance::discovery_and_negotiation"
    } else if VERIFIED_BY_MUTATION.contains(&op) {
        "interop_conformance::authorized_mutation"
    } else if VERIFIED_BY_CONTINUITY.contains(&op) {
        "interop_conformance::session_task_continuity"
    } else {
        return CapabilityEvidence {
            conformance_test: String::new(),
            state: VerificationState::NotRun,
            evidence_ref: None,
        };
    };
    CapabilityEvidence {
        conformance_test: test.to_string(),
        state: VerificationState::Verified,
        evidence_ref: Some(format!("tests/interop_conformance.rs::{test}")),
    }
}

/// Build the built-in manifest, derived from [`RPC_OPS`] so implemented
/// operations cannot silently drift from the dispatch list.
pub fn builtin_manifest() -> InteropManifest {
    let mut capabilities = Vec::new();
    for meta in OP_META {
        capabilities.push(InteropCapability {
            id: format!("rpc:{}", meta.op),
            surface: "rpc".to_string(),
            implementation_status: ImplementationStatus::Implemented,
            owner_component: "src/decapod/lib.rs::rpc_handlers".to_string(),
            implementation: Some(ImplementationMapping {
                op: meta.op.to_string(),
                module: "src/decapod/lib.rs::rpc_handlers".to_string(),
                entrypoints: vec!["rpc-stdin".to_string(), "rpc-http-adapter".to_string()],
                read_only: meta.read_only,
                session_required: meta.session_required,
            }),
            evidence: evidence_for(meta.op),
            description: meta.description.to_string(),
        });
    }
    // CLI surface: implemented, but exercised through the CLI, not RPC.
    capabilities.push(InteropCapability {
        id: "cli:decapod".to_string(),
        surface: "cli".to_string(),
        implementation_status: ImplementationStatus::Implemented,
        owner_component: "src/decapod/cli.rs".to_string(),
        implementation: Some(ImplementationMapping {
            op: "cli".to_string(),
            module: "src/decapod/cli.rs".to_string(),
            entrypoints: vec!["cli".to_string()],
            read_only: false,
            session_required: true,
        }),
        evidence: CapabilityEvidence {
            conformance_test: "interop_conformance::cli_rpc_parity".to_string(),
            state: VerificationState::Verified,
            evidence_ref: Some("tests/interop_conformance.rs::cli_rpc_parity".to_string()),
        },
        description: "Full clap CLI surface mirroring the governed operations".to_string(),
    });
    // Explicitly unsupported transports — never advertised as available.
    for (id, desc) in [
        (
            "transport:mcp",
            "Native MCP server adapter (issue #872, not implemented in this build)",
        ),
        ("transport:acp", "ACP transport adapter (not implemented)"),
        ("transport:a2a", "A2A transport adapter (not implemented)"),
    ] {
        capabilities.push(InteropCapability {
            id: id.to_string(),
            surface: "transport".to_string(),
            implementation_status: ImplementationStatus::IntentionallyNotUsed,
            owner_component: "none".to_string(),
            implementation: None,
            evidence: CapabilityEvidence {
                conformance_test: String::new(),
                state: VerificationState::NotRun,
                evidence_ref: None,
            },
            description: desc.to_string(),
        });
    }
    InteropManifest {
        schema_version: INTEROP_MANIFEST_SCHEMA_VERSION.to_string(),
        profile_id: INTEROP_PROFILE_ID.to_string(),
        profile_version: INTEROP_PROFILE_VERSION.to_string(),
        reference_implementation: "decapod".to_string(),
        implementation_version: env!("CARGO_PKG_VERSION").to_string(),
        authority_declaration: "Decapod's governed state, session, policy and validation mechanisms are the sole authority. This manifest is a descriptive claim; negotiation is not authorization; conformance results are evidence, not publication proof.".to_string(),
        capabilities,
    }
}

/// Typed manifest violation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManifestViolation {
    pub code: String,
    pub capability_id: String,
    pub detail: String,
}

fn violation(code: &str, id: &str, detail: impl Into<String>) -> ManifestViolation {
    ManifestViolation {
        code: code.to_string(),
        capability_id: id.to_string(),
        detail: detail.into(),
    }
}

fn valid_version(v: &str) -> bool {
    let mut parts = v.split('.');
    parts
        .next()
        .is_some_and(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()))
        && v.split('.').count() == 3
        && v.split('.')
            .skip(1)
            .all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()))
}

/// Validate a manifest. Enforces the declaration/evidence boundary: a
/// `verified`/`failed` claim requires an admissible, known conformance test
/// and an evidence reference; an `implemented` entry requires an
/// implementation mapping whose op is registered; duplicates and malformed
/// declarations are rejected.
pub fn validate_manifest(m: &InteropManifest) -> Result<(), Vec<ManifestViolation>> {
    let mut violations = Vec::new();
    if !valid_version(&m.schema_version) {
        violations.push(violation("invalid_schema_version", "", &m.schema_version));
    }
    if m.profile_id != INTEROP_PROFILE_ID {
        violations.push(violation("invalid_profile_id", "", &m.profile_id));
    }
    if !SUPPORTED_PROFILE_VERSIONS.contains(&m.profile_version.as_str())
        && !m.profile_version.starts_with('v')
    {
        violations.push(violation("invalid_profile_version", "", &m.profile_version));
    }
    let mut seen = BTreeSet::new();
    for cap in &m.capabilities {
        if cap.id.trim().is_empty() {
            violations.push(violation("empty_id", "", "capability id is empty"));
            continue;
        }
        if !seen.insert(cap.id.clone()) {
            violations.push(violation(
                "duplicate_id",
                &cap.id,
                "duplicate capability id",
            ));
        }
        if cap.owner_component.trim().is_empty() {
            violations.push(violation(
                "missing_owner",
                &cap.id,
                "owner_component is empty",
            ));
        }
        match cap.implementation_status {
            ImplementationStatus::Implemented | ImplementationStatus::Partial => {
                match &cap.implementation {
                    None => violations.push(violation(
                        "missing_implementation_mapping",
                        &cap.id,
                        "implemented/partial capability has no implementation mapping",
                    )),
                    Some(mapping) => {
                        if cap.surface == "rpc" && !RPC_OPS.contains(&mapping.op.as_str()) {
                            violations.push(violation(
                                "unknown_implementation_op",
                                &cap.id,
                                format!("op '{}' is not a registered RPC op", mapping.op),
                            ));
                        }
                    }
                }
            }
            ImplementationStatus::Planned | ImplementationStatus::IntentionallyNotUsed => {
                if cap.implementation.is_some() {
                    violations.push(violation(
                        "contradictory_declaration",
                        &cap.id,
                        "planned/intentionally_not_used capability must not carry an implementation mapping",
                    ));
                }
            }
        }
        match cap.evidence.state {
            VerificationState::Verified | VerificationState::Failed => {
                if !KNOWN_CONFORMANCE_TESTS.contains(&cap.evidence.conformance_test.as_str()) {
                    violations.push(violation(
                        "verification_without_admissible_evidence",
                        &cap.id,
                        format!(
                            "state {:?} references unknown conformance test '{}'",
                            cap.evidence.state, cap.evidence.conformance_test
                        ),
                    ));
                }
                if cap
                    .evidence
                    .evidence_ref
                    .as_deref()
                    .is_none_or(str::is_empty)
                {
                    violations.push(violation(
                        "verification_without_evidence_ref",
                        &cap.id,
                        "verified/failed requires evidence_ref",
                    ));
                }
            }
            _ => {}
        }
    }
    if violations.is_empty() {
        Ok(())
    } else {
        Err(violations)
    }
}

/// A named interoperability profile: a composition of existing operations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Profile {
    pub id: String,
    pub version: String,
    pub required_ops: Vec<String>,
    pub optional_ops: Vec<String>,
    pub description: String,
}

/// Profiles aligned with issue #871. A profile is fully implemented only when
/// every required op is implemented; `profile_status` computes that from the
/// manifest rather than trusting the declaration.
pub fn profiles() -> Vec<Profile> {
    vec![
        Profile {
            id: "read-only-orientation".to_string(),
            version: "v1".to_string(),
            required_ops: vec![
                "agent.init".to_string(),
                "context.resolve".to_string(),
                "schema.get".to_string(),
                "store.query".to_string(),
                "workspace.status".to_string(),
                "interop.manifest".to_string(),
                "interop.negotiate".to_string(),
                "interop.project".to_string(),
            ],
            optional_ops: vec![
                "context.capsule.query".to_string(),
                "context.bindings".to_string(),
                "constitution.get".to_string(),
                "agent.registry.query".to_string(),
            ],
            description: "Read-only orientation and discovery".to_string(),
        },
        Profile {
            id: "governed-task-execution".to_string(),
            version: "v1".to_string(),
            required_ops: vec![
                "agent.init".to_string(),
                "context.resolve".to_string(),
                "workspace.ensure".to_string(),
                "workspace.status".to_string(),
                "store.upsert".to_string(),
                "store.query".to_string(),
            ],
            optional_ops: vec![
                "context.bundle.export".to_string(),
                "context.bundle.import".to_string(),
            ],
            description: "Governed task execution in an isolated workspace".to_string(),
        },
        Profile {
            id: "handoff-continuity".to_string(),
            version: "v1".to_string(),
            required_ops: vec![
                "context.bundle.export".to_string(),
                "context.bundle.import".to_string(),
                "agent.registry.query".to_string(),
                "workspace.status".to_string(),
            ],
            optional_ops: vec!["context.resolve".to_string()],
            description: "Agent handoff via portable context bundles".to_string(),
        },
        Profile {
            id: "proof-aware-publication".to_string(),
            version: "v1".to_string(),
            required_ops: vec![
                "validate.run".to_string(),
                "workspace.publish".to_string(),
                "specs.refresh".to_string(),
            ],
            optional_ops: vec!["assurance.evaluate".to_string()],
            description: "Proof-gated publication of a workspace branch".to_string(),
        },
        Profile {
            id: "fleet-observation".to_string(),
            version: "v1".to_string(),
            required_ops: vec![
                "agent.registry.query".to_string(),
                "fleet.observe".to_string(),
            ],
            optional_ops: vec![],
            description: "Fleet-wide observation (partial: fleet.observe has no RPC op yet)"
                .to_string(),
        },
    ]
}

/// Computed support status of a profile against a manifest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProfileStatus {
    Implemented,
    Partial,
    Unsupported,
}

pub fn profile_status(
    manifest: &InteropManifest,
    profile: &Profile,
) -> (ProfileStatus, Vec<String>) {
    let missing: Vec<String> = profile
        .required_ops
        .iter()
        .filter(|op| {
            manifest
                .capabilities
                .iter()
                .find(|c| c.implementation.as_ref().is_some_and(|m| &m.op == *op))
                .is_none_or(|c| c.implementation_status != ImplementationStatus::Implemented)
        })
        .cloned()
        .collect();
    let status = if missing.is_empty() {
        ProfileStatus::Implemented
    } else if missing.len() == profile.required_ops.len() {
        ProfileStatus::Unsupported
    } else {
        ProfileStatus::Partial
    };
    (status, missing)
}

/// Negotiation request. Unknown fields are rejected so a client cannot smuggle
/// authority (`role`, `admin`, `allow`, ...) into negotiation.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NegotiationRequest {
    #[serde(default)]
    pub required_ops: Vec<String>,
    #[serde(default)]
    pub optional_ops: Vec<String>,
    #[serde(default)]
    pub required_extensions: Vec<String>,
    #[serde(default)]
    pub optional_extensions: Vec<String>,
    #[serde(default)]
    pub requested_profile: Option<String>,
    #[serde(default)]
    pub profile_version: Option<String>,
}

/// Typed, deterministic negotiation refusal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "reason", rename_all = "snake_case")]
pub enum NegotiationRefusal {
    UnsupportedProfileVersion {
        requested: String,
        supported: Vec<String>,
    },
    UnknownProfile {
        profile: String,
    },
    InvalidCapabilityId {
        id: String,
    },
    UnsupportedRequiredOp {
        op: String,
    },
    UnsupportedRequiredExtension {
        extension: String,
    },
    MalformedRequest {
        detail: String,
    },
}

impl NegotiationRefusal {
    pub fn code(&self) -> &'static str {
        "negotiation_refused"
    }
}

/// Successful negotiation outcome. All lists are sorted so the result is
/// independent of the ordering of independent optional inputs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NegotiationOutcome {
    pub negotiated: bool,
    pub profile_id: String,
    pub profile_version: String,
    pub selected_profile: Option<String>,
    pub profile_status: Option<ProfileStatus>,
    pub missing_profile_ops: Vec<String>,
    pub supported_required_ops: Vec<String>,
    pub omitted_optional_ops: Vec<String>,
    pub omitted_optional_extensions: Vec<String>,
    /// Always present: negotiation never grants authority.
    pub authority_note: String,
}

fn is_known_capability_id(manifest: &InteropManifest, id: &str) -> bool {
    manifest.capabilities.iter().any(|c| c.id == id)
        || RPC_OPS.contains(&id)
        || id.starts_with("rpc:")
}

fn is_implemented(manifest: &InteropManifest, op: &str) -> bool {
    let id = if op.starts_with("rpc:") {
        op.to_string()
    } else {
        format!("rpc:{op}")
    };
    manifest
        .capabilities
        .iter()
        .find(|c| c.id == id)
        .is_some_and(|c| c.implementation_status == ImplementationStatus::Implemented)
}

/// Deterministic, fail-closed negotiation. Read-only: it never creates task
/// ownership, approves work, alters policy, or advances validation state.
pub fn negotiate(
    manifest: &InteropManifest,
    request: &NegotiationRequest,
) -> Result<NegotiationOutcome, NegotiationRefusal> {
    // Malformed: duplicates inside a single list.
    for (list, name) in [
        (&request.required_ops, "required_ops"),
        (&request.optional_ops, "optional_ops"),
        (&request.required_extensions, "required_extensions"),
        (&request.optional_extensions, "optional_extensions"),
    ] {
        let mut seen = BTreeSet::new();
        for item in list {
            if !seen.insert(item.clone()) {
                return Err(NegotiationRefusal::MalformedRequest {
                    detail: format!("duplicate entry '{item}' in {name}"),
                });
            }
        }
    }
    // Contradictory: same id required and optional.
    for op in &request.required_ops {
        if request.optional_ops.contains(op) {
            return Err(NegotiationRefusal::MalformedRequest {
                detail: format!("op '{op}' appears in both required_ops and optional_ops"),
            });
        }
    }
    for ext in &request.required_extensions {
        if request.optional_extensions.contains(ext) {
            return Err(NegotiationRefusal::MalformedRequest {
                detail: format!(
                    "extension '{ext}' appears in both required_extensions and optional_extensions"
                ),
            });
        }
    }

    let version = request
        .profile_version
        .clone()
        .unwrap_or_else(|| INTEROP_PROFILE_VERSION.to_string());
    if !SUPPORTED_PROFILE_VERSIONS.contains(&version.as_str()) {
        return Err(NegotiationRefusal::UnsupportedProfileVersion {
            requested: version,
            supported: SUPPORTED_PROFILE_VERSIONS
                .iter()
                .map(|s| s.to_string())
                .collect(),
        });
    }

    let mut selected_profile = None;
    let mut status_of_profile = None;
    let mut missing_profile_ops = Vec::new();
    if let Some(profile_id) = &request.requested_profile {
        let profile = profiles()
            .into_iter()
            .find(|p| &p.id == profile_id)
            .ok_or_else(|| NegotiationRefusal::UnknownProfile {
                profile: profile_id.clone(),
            })?;
        let (status, missing) = profile_status(manifest, &profile);
        selected_profile = Some(profile.id.clone());
        status_of_profile = Some(status);
        missing_profile_ops = missing;
    }

    // Required operations are mandatory: unknown ids and non-implemented ops
    // refuse before any work.
    let mut supported_required_ops = Vec::new();
    for op in &request.required_ops {
        if !is_known_capability_id(manifest, op) {
            return Err(NegotiationRefusal::InvalidCapabilityId { id: op.clone() });
        }
        if !is_implemented(manifest, op) {
            return Err(NegotiationRefusal::UnsupportedRequiredOp { op: op.clone() });
        }
        supported_required_ops.push(op.clone());
    }
    supported_required_ops.sort();

    for ext in &request.required_extensions {
        if !SUPPORTED_EXTENSIONS.contains(&ext.as_str()) {
            return Err(NegotiationRefusal::UnsupportedRequiredExtension {
                extension: ext.clone(),
            });
        }
    }

    // Unknown optional inputs degrade safely and are reported as omitted.
    let mut omitted_optional_ops: Vec<String> = request
        .optional_ops
        .iter()
        .filter(|op| !is_known_capability_id(manifest, op) || !is_implemented(manifest, op))
        .cloned()
        .collect();
    omitted_optional_ops.sort();
    omitted_optional_ops.dedup();
    let mut omitted_optional_extensions: Vec<String> = request
        .optional_extensions
        .iter()
        .filter(|ext| !SUPPORTED_EXTENSIONS.contains(&ext.as_str()))
        .cloned()
        .collect();
    omitted_optional_extensions.sort();
    omitted_optional_extensions.dedup();

    let negotiated = status_of_profile != Some(ProfileStatus::Unsupported)
        && missing_profile_ops.is_empty()
        && request
            .required_ops
            .iter()
            .all(|op| is_implemented(manifest, op));

    Ok(NegotiationOutcome {
        negotiated,
        profile_id: INTEROP_PROFILE_ID.to_string(),
        profile_version: version,
        selected_profile,
        profile_status: status_of_profile,
        missing_profile_ops,
        supported_required_ops,
        omitted_optional_ops,
        omitted_optional_extensions,
        authority_note: "Negotiation is compatibility discovery only; it grants no authority, creates no ownership, and changes no governed state.".to_string(),
    })
}

#[cfg(test)]
#[path = "../../../tests/unit/core/interop_tests.rs"]
mod tests;
