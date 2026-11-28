//! Bounded A2A Agent Card and governed task/handoff envelope adapter (issue #873).
//!
//! Advertises ONLY executable governance skills, reports core A2A and Decapod extension
//! conformance separately, and validates bounded task/handoff envelopes while keeping
//! repository-local custody, proof, and authority strictly authoritative.

use crate::core::error::DecapodError;
use crate::core::http_transport::{
    self, HttpRequest, HttpResponse, HttpTransportConfig, bind_checked_listener, error_code_json,
    serve_on_listener,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::io::Write;
use std::net::{SocketAddr, TcpListener};
use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub http_endpoint: Option<String>,
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
        http_endpoint: None,
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

/// Maximum number of non-authoritative external A2A task records retained per
/// bounded server invocation (`INV-DAEMONLESS`).
pub const MAX_A2A_HTTP_TASK_RECORDS: usize = 256;

/// Build the truthful A2A Agent Card for the opt-in authenticated HTTP loopback
/// adapter (#873), advertising only implemented HTTP and local CLI/RPC surfaces.
pub fn builtin_http_agent_card(endpoint_url: &str) -> A2aAgentCard {
    let mut card = builtin_agent_card();
    card.transport_mode = "authenticated-http-loopback".to_string();
    card.http_endpoint = Some(endpoint_url.to_string());
    card.authentication_modes = vec![
        "bearer-token".to_string(),
        "local-session-token".to_string(),
    ];
    card.skills.push(A2aSkillDescriptor {
        id: "governance.handoff.cancel".to_string(),
        name: "Handoff Artifact Cancellation".to_string(),
        description: "Mark an external A2A handoff artifact canceled so replays fail closed while leaving local repository custody untouched.".to_string(),
        read_only: false,
        backing_surface: "http:POST /a2a/v1/tasks/cancel".to_string(),
        input_modes: vec!["application/json".to_string()],
        output_modes: vec!["application/json".to_string()],
    });
    card.conformance_layers.core_a2a_supported_operations = vec![
        "agent_card.get".to_string(),
        "task.handoff.validate".to_string(),
        "task.artifact.inspect".to_string(),
        "task.handoff.cancel".to_string(),
    ];
    card
}

/// Options for serving the bounded authenticated HTTP A2A adapter (#873).
#[derive(Debug, Clone)]
pub struct A2aServeOptions {
    pub bind: String,
    pub auth_token: String,
    pub allow_remote: bool,
    pub max_requests: Option<usize>,
    pub max_body_bytes: usize,
    pub max_response_bytes: usize,
    pub read_timeout: Duration,
    pub write_timeout: Duration,
    pub allowed_origins: Vec<String>,
    pub local_context: LocalHandoffContext,
}

impl A2aServeOptions {
    pub fn loopback_default(
        auth_token: impl Into<String>,
        local_context: LocalHandoffContext,
    ) -> Self {
        Self {
            bind: "127.0.0.1:0".to_string(),
            auth_token: auth_token.into(),
            allow_remote: false,
            max_requests: None,
            max_body_bytes: 256 * 1024,
            max_response_bytes: http_transport::DEFAULT_MAX_RESPONSE_BYTES,
            read_timeout: Duration::from_secs(5),
            write_timeout: Duration::from_secs(5),
            allowed_origins: Vec::new(),
            local_context,
        }
    }
}

/// Stored external A2A task/handoff record (non-authoritative external evidence).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct A2aTaskRecord {
    pub task_id: String,
    pub handoff_id: String,
    pub task_state: A2aTaskState,
    pub sender_agent_id: String,
    pub receiver_agent_id: String,
    pub summary: String,
    pub dependencies: Vec<String>,
    pub decapod_extension: DecapodA2aExtension,
    pub content_sha256: String,
    pub validation_report: A2aHandoffValidationReport,
    pub imported_authority_granted: bool,
    pub local_custody_modified: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cancel_reason: Option<String>,
}

/// HTTP response payload returned by `POST /a2a/v1/handoff`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct A2aHandoffHttpResult {
    pub accepted: bool,
    pub imported_authority_granted: bool,
    pub local_custody_modified: bool,
    pub handoff_id: String,
    pub task_id: String,
    pub task_state: A2aTaskState,
    pub report: A2aHandoffValidationReport,
    pub continuation_hint: String,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum HandoffRequestBody {
    Wrapped {
        envelope: Box<A2aHandoffEnvelope>,
        #[allow(dead_code)]
        #[serde(default)]
        local_context: Option<LocalHandoffContext>,
    },
    Direct(Box<A2aHandoffEnvelope>),
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct TaskInspectRequest {
    task_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct TaskCancelRequest {
    task_id: String,
    #[serde(default)]
    reason: Option<String>,
}

struct A2aHttpServerState {
    endpoint_url: String,
    local_context: LocalHandoffContext,
    records: BTreeMap<String, A2aTaskRecord>,
}

impl A2aHttpServerState {
    fn new(endpoint_url: String, local_context: LocalHandoffContext) -> Self {
        Self {
            endpoint_url,
            local_context,
            records: BTreeMap::new(),
        }
    }

    fn handle_request(&mut self, req: &HttpRequest) -> HttpResponse {
        match (req.method.as_str(), req.path.as_str()) {
            ("GET", "/.well-known/agent.json") | ("GET", "/a2a/v1/card") => {
                let card = builtin_http_agent_card(&self.endpoint_url);
                match serde_json::to_vec_pretty(&card) {
                    Ok(bytes) => HttpResponse::json(200, bytes),
                    Err(err) => HttpResponse::json(
                        500,
                        error_code_json("serialization_error", &err.to_string()),
                    ),
                }
            }
            ("POST", "/a2a/v1/handoff") => self.handle_handoff(&req.body),
            ("POST", "/a2a/v1/tasks/inspect") => self.handle_inspect(&req.body),
            ("POST", "/a2a/v1/tasks/cancel") => self.handle_cancel(&req.body),
            _ => HttpResponse::json(404, error_code_json("not_found", "unknown A2A HTTP route")),
        }
    }

    fn handle_handoff(&mut self, body: &[u8]) -> HttpResponse {
        let parsed: HandoffRequestBody = match serde_json::from_slice(body) {
            Ok(val) => val,
            Err(err) => {
                return HttpResponse::json(
                    400,
                    error_code_json(
                        "invalid_a2a_payload",
                        &format!("invalid handoff JSON: {err}"),
                    ),
                );
            }
        };
        let envelope = match parsed {
            HandoffRequestBody::Wrapped { envelope, .. } => *envelope,
            HandoffRequestBody::Direct(envelope) => *envelope,
        };

        if let Some(existing) = self.records.get(&envelope.task_id)
            && existing.task_state == A2aTaskState::Canceled
        {
            let mut canceled_env = envelope.clone();
            canceled_env.task_state = A2aTaskState::Canceled;
            let report = validate_a2a_handoff(&canceled_env, &self.local_context);
            let res = A2aHandoffHttpResult {
                accepted: false,
                imported_authority_granted: false,
                local_custody_modified: false,
                handoff_id: envelope.handoff_id,
                task_id: envelope.task_id,
                task_state: A2aTaskState::Canceled,
                report,
                continuation_hint:
                    "task handoff was previously canceled; local custody remains authoritative"
                        .to_string(),
            };
            return HttpResponse::json(422, serde_json::to_vec(&res).unwrap_or_default());
        }

        // Always validate against server-owned local ground truth, never remote claims.
        let report = validate_a2a_handoff(&envelope, &self.local_context);
        let accepted = report.admissible_for_continuation;
        if accepted {
            if !self.records.contains_key(&envelope.task_id)
                && self.records.len() >= MAX_A2A_HTTP_TASK_RECORDS
            {
                return HttpResponse::json(
                    503,
                    error_code_json(
                        "a2a_capacity_exceeded",
                        "bounded A2A task artifact table is full",
                    ),
                );
            }
            let record = A2aTaskRecord {
                task_id: envelope.task_id.clone(),
                handoff_id: envelope.handoff_id.clone(),
                task_state: envelope.task_state,
                sender_agent_id: envelope.sender_agent_id.clone(),
                receiver_agent_id: envelope.receiver_agent_id.clone(),
                summary: envelope.summary.clone(),
                dependencies: envelope.dependencies.clone(),
                decapod_extension: envelope.decapod_extension.clone(),
                content_sha256: envelope.content_sha256.clone(),
                validation_report: report.clone(),
                imported_authority_granted: false,
                local_custody_modified: false,
                cancel_reason: None,
            };
            self.records.insert(envelope.task_id.clone(), record);
        }

        let res = A2aHandoffHttpResult {
            accepted,
            imported_authority_granted: false,
            local_custody_modified: false,
            handoff_id: envelope.handoff_id,
            task_id: envelope.task_id.clone(),
            task_state: envelope.task_state,
            report,
            continuation_hint: if accepted {
                format!(
                    "re-claim locally before authoring: decapod todo claim --id {}",
                    envelope.task_id
                )
            } else {
                "handoff failed closed; reconcile local custody/head/base before retrying"
                    .to_string()
            },
        };
        let status = if accepted { 200 } else { 422 };
        HttpResponse::json(status, serde_json::to_vec_pretty(&res).unwrap_or_default())
    }

    fn handle_inspect(&self, body: &[u8]) -> HttpResponse {
        let req: TaskInspectRequest = match serde_json::from_slice(body) {
            Ok(v) => v,
            Err(err) => {
                return HttpResponse::json(
                    400,
                    error_code_json(
                        "invalid_request",
                        &format!("invalid inspect payload: {err}"),
                    ),
                );
            }
        };
        match self.records.get(req.task_id.trim()) {
            Some(record) => {
                HttpResponse::json(200, serde_json::to_vec_pretty(record).unwrap_or_default())
            }
            None => HttpResponse::json(
                404,
                error_code_json(
                    "task_not_found",
                    &format!(
                        "no A2A handoff artifact recorded for task '{}'",
                        req.task_id
                    ),
                ),
            ),
        }
    }

    fn handle_cancel(&mut self, body: &[u8]) -> HttpResponse {
        let req: TaskCancelRequest = match serde_json::from_slice(body) {
            Ok(v) => v,
            Err(err) => {
                return HttpResponse::json(
                    400,
                    error_code_json("invalid_request", &format!("invalid cancel payload: {err}")),
                );
            }
        };
        let Some(record) = self.records.get_mut(req.task_id.trim()) else {
            return HttpResponse::json(
                404,
                error_code_json(
                    "task_not_found",
                    &format!(
                        "no A2A handoff artifact recorded for task '{}'",
                        req.task_id
                    ),
                ),
            );
        };
        record.task_state = A2aTaskState::Canceled;
        record.cancel_reason = req.reason;
        record.imported_authority_granted = false;
        record.local_custody_modified = false;
        HttpResponse::json(200, serde_json::to_vec_pretty(record).unwrap_or_default())
    }
}

/// Derive a `LocalHandoffContext` from a repository root's current git HEAD/base
/// and explicit caller overrides for owner/receiver/lease generation.
pub fn resolve_local_handoff_context(
    repo_root: &Path,
    repository_id: Option<&str>,
    base_sha: Option<&str>,
    owner_agent: Option<&str>,
    lease_generation: u64,
    receiver_agent: Option<&str>,
    now_epoch: Option<u64>,
) -> Result<LocalHandoffContext, String> {
    let git_rev = |rev: &str| -> Option<String> {
        let out = std::process::Command::new("git")
            .current_dir(repo_root)
            .args(["rev-parse", rev])
            .output()
            .ok()?;
        if out.status.success() {
            let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !s.is_empty() {
                return Some(s);
            }
        }
        None
    };
    let head_sha = git_rev("HEAD").ok_or_else(|| {
        format!(
            "failed to resolve git HEAD in '{}' for A2A local context",
            repo_root.display()
        )
    })?;
    let resolved_base = base_sha
        .map(str::to_string)
        .or_else(|| git_rev("origin/master"))
        .or_else(|| git_rev("origin/main"))
        .or_else(|| git_rev("master"))
        .or_else(|| git_rev("main"))
        .unwrap_or_else(|| head_sha.clone());
    let repo_name = repository_id.map(str::to_string).unwrap_or_else(|| {
        repo_root
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "local-repo".to_string())
    });
    let now = now_epoch.unwrap_or_else(|| {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
    });
    Ok(LocalHandoffContext {
        repository_id: repo_name,
        current_head_sha: head_sha,
        current_base_sha: resolved_base,
        current_owner_agent_id: owner_agent.unwrap_or("agent-a").to_string(),
        current_lease_generation: lease_generation.max(1),
        expected_receiver_agent_id: receiver_agent.unwrap_or("agent-b").to_string(),
        now_epoch: now,
    })
}

fn build_a2a_transport_config(repo_root: &Path, options: &A2aServeOptions) -> HttpTransportConfig {
    let canonical_repo = std::fs::canonicalize(repo_root)
        .unwrap_or_else(|_| repo_root.to_path_buf())
        .display()
        .to_string();
    let mut cfg = HttpTransportConfig::for_rpc(
        &options.bind,
        &options.auth_token,
        options.allow_remote,
        options.max_body_bytes,
        canonical_repo,
    );
    cfg.max_response_bytes = options.max_response_bytes;
    cfg.read_timeout = options.read_timeout;
    cfg.write_timeout = options.write_timeout;
    cfg.max_requests = options.max_requests;
    cfg.allowed_origins = options.allowed_origins.clone();
    cfg.allow_a2a_routes = true;
    cfg
}

/// Bind and serve the authenticated A2A HTTP adapter, printing one JSON readiness
/// line on stdout when bound (for zero-race ephemeral port discovery).
pub fn serve_a2a_http(
    repo_root: &Path,
    options: A2aServeOptions,
) -> Result<SocketAddr, DecapodError> {
    let (listener, addr) =
        bind_checked_listener(&options.bind, &options.auth_token, options.allow_remote)
            .map_err(DecapodError::IoError)?;
    let ready = serde_json::json!({
        "status": "listening",
        "endpoint": format!("http://{addr}"),
        "repository_id": options.local_context.repository_id,
    });
    println!("{ready}");
    let _ = std::io::stdout().flush();
    serve_a2a_on_listener(repo_root, listener, options)?;
    Ok(addr)
}

/// Serve the authenticated A2A HTTP adapter on an already-bound `TcpListener`.
pub fn serve_a2a_on_listener(
    repo_root: &Path,
    listener: TcpListener,
    options: A2aServeOptions,
) -> Result<(), DecapodError> {
    let addr = listener.local_addr().map_err(DecapodError::IoError)?;
    let endpoint_url = format!("http://{addr}");
    let config = build_a2a_transport_config(repo_root, &options);
    let mut state = A2aHttpServerState::new(endpoint_url, options.local_context);
    serve_on_listener(listener, config, move |req| state.handle_request(req))
        .map_err(DecapodError::IoError)
}

/// Raw HTTP response returned by [`A2aHttpClient`].
#[derive(Debug, Clone)]
pub struct A2aHttpClientResponse {
    pub status: u16,
    pub headers: BTreeMap<String, String>,
    pub body: Vec<u8>,
}

/// Pure-std bounded HTTP/1.1 client for `decapod a2a serve` endpoints (#873).
#[derive(Debug, Clone)]
pub struct A2aHttpClient {
    pub base_url: String,
    pub auth_token: String,
    pub timeout: Duration,
}

impl A2aHttpClient {
    pub fn new(base_url: impl Into<String>, auth_token: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into().trim_end_matches('/').to_string(),
            auth_token: auth_token.into(),
            timeout: Duration::from_secs(5),
        }
    }

    pub fn request(
        &self,
        method: &str,
        path: &str,
        idempotency_key: Option<&str>,
        body: Option<&[u8]>,
    ) -> Result<A2aHttpClientResponse, String> {
        let host_port = self
            .base_url
            .strip_prefix("http://")
            .ok_or_else(|| "A2aHttpClient supports http:// URLs only".to_string())?
            .split('/')
            .next()
            .ok_or_else(|| "missing host:port in A2A URL".to_string())?
            .to_string();

        let payload = body.unwrap_or(&[]);
        let mut req = format!(
            "{method} {path} HTTP/1.1\r\nHost: {host_port}\r\nAuthorization: Bearer {}\r\nConnection: close\r\n",
            self.auth_token
        );
        if let Some(key) = idempotency_key {
            req.push_str(&format!("Idempotency-Key: {key}\r\n"));
        }
        if method == "POST" || !payload.is_empty() {
            req.push_str(&format!(
                "Content-Type: application/json\r\nContent-Length: {}\r\n",
                payload.len()
            ));
        }
        req.push_str("\r\n");

        let timeout = self.timeout;
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_io()
            .enable_time()
            .build()
            .map_err(|e| format!("failed to build tokio runtime for A2A client: {e}"))?;
        let raw = rt.block_on(async move {
            tokio::time::timeout(timeout, async move {
                let mut stream = tokio::net::TcpStream::connect(&host_port)
                    .await
                    .map_err(|e| format!("failed to connect to A2A server '{host_port}': {e}"))?;
                stream
                    .write_all(req.as_bytes())
                    .await
                    .map_err(|e| format!("failed to write A2A HTTP headers: {e}"))?;
                stream
                    .write_all(payload)
                    .await
                    .map_err(|e| format!("failed to write A2A HTTP body: {e}"))?;
                let mut raw = Vec::new();
                stream
                    .read_to_end(&mut raw)
                    .await
                    .map_err(|e| format!("failed to read A2A HTTP response: {e}"))?;
                Ok::<Vec<u8>, String>(raw)
            })
            .await
            .map_err(|_| "A2A HTTP client request timed out".to_string())?
        })?;
        let header_end = raw
            .windows(4)
            .position(|w| w == b"\r\n\r\n")
            .ok_or_else(|| "malformed HTTP response from A2A server".to_string())?;
        let head_str = std::str::from_utf8(&raw[..header_end])
            .map_err(|e| format!("non-UTF8 HTTP response headers: {e}"))?;
        let mut lines = head_str.split("\r\n");
        let status_line = lines.next().unwrap_or_default();
        let status: u16 = status_line
            .split_whitespace()
            .nth(1)
            .and_then(|s| s.parse().ok())
            .ok_or_else(|| format!("malformed HTTP status line: '{status_line}'"))?;
        let mut headers = BTreeMap::new();
        for line in lines.filter(|l| !l.is_empty()) {
            if let Some((k, v)) = line.split_once(':') {
                headers.insert(k.trim().to_ascii_lowercase(), v.trim().to_string());
            }
        }
        let body_slice = &raw[header_end + 4..];
        let body = if let Some(len) = headers
            .get("content-length")
            .and_then(|v| v.parse::<usize>().ok())
        {
            body_slice[..body_slice.len().min(len)].to_vec()
        } else {
            body_slice.to_vec()
        };
        Ok(A2aHttpClientResponse {
            status,
            headers,
            body,
        })
    }

    pub fn fetch_agent_card(&self) -> Result<A2aAgentCard, String> {
        let resp = self.request("GET", "/.well-known/agent.json", None, None)?;
        if resp.status != 200 {
            return Err(format!(
                "GET /.well-known/agent.json failed with HTTP {}: {}",
                resp.status,
                String::from_utf8_lossy(&resp.body)
            ));
        }
        serde_json::from_slice(&resp.body).map_err(|e| format!("invalid agent card JSON: {e}"))
    }

    pub fn submit_handoff(
        &self,
        idempotency_key: &str,
        envelope: &A2aHandoffEnvelope,
    ) -> Result<(u16, A2aHandoffHttpResult), String> {
        let payload = serde_json::to_vec(envelope).map_err(|e| e.to_string())?;
        let resp = self.request(
            "POST",
            "/a2a/v1/handoff",
            Some(idempotency_key),
            Some(&payload),
        )?;
        let parsed: A2aHandoffHttpResult = serde_json::from_slice(&resp.body).map_err(|e| {
            format!(
                "invalid A2A handoff HTTP {} response: {e}: {}",
                resp.status,
                String::from_utf8_lossy(&resp.body)
            )
        })?;
        Ok((resp.status, parsed))
    }

    pub fn inspect_task(
        &self,
        idempotency_key: &str,
        task_id: &str,
    ) -> Result<(u16, A2aTaskRecord), String> {
        let payload = serde_json::to_vec(&serde_json::json!({ "task_id": task_id }))
            .map_err(|e| e.to_string())?;
        let resp = self.request(
            "POST",
            "/a2a/v1/tasks/inspect",
            Some(idempotency_key),
            Some(&payload),
        )?;
        let parsed: A2aTaskRecord = serde_json::from_slice(&resp.body).map_err(|e| {
            format!(
                "invalid A2A inspect HTTP {} response: {e}: {}",
                resp.status,
                String::from_utf8_lossy(&resp.body)
            )
        })?;
        Ok((resp.status, parsed))
    }

    pub fn cancel_task(
        &self,
        idempotency_key: &str,
        task_id: &str,
        reason: Option<&str>,
    ) -> Result<(u16, A2aTaskRecord), String> {
        let payload = serde_json::to_vec(&serde_json::json!({
            "task_id": task_id,
            "reason": reason,
        }))
        .map_err(|e| e.to_string())?;
        let resp = self.request(
            "POST",
            "/a2a/v1/tasks/cancel",
            Some(idempotency_key),
            Some(&payload),
        )?;
        let parsed: A2aTaskRecord = serde_json::from_slice(&resp.body).map_err(|e| {
            format!(
                "invalid A2A cancel HTTP {} response: {e}: {}",
                resp.status,
                String::from_utf8_lossy(&resp.body)
            )
        })?;
        Ok((resp.status, parsed))
    }
}
