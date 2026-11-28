//! Bounded local stdio MCP JSON-RPC 2.0 server adapter and schema/config exporter (issue #872).
//!
//! This adapter exposes a narrow, policy-checked governance surface over local stdio
//! without making MCP transport state authoritative or leaving background daemons behind.
//! Every read-only resource redacts secrets and refuses hidden rubrics or path traversal;
//! every mutating tool enforces session/scope/authority checks identically to CLI/RPC.

use crate::core::{
    agent_interop::{
        EnvelopeImportReport, GovernanceEnvelopeV1, LocalTrustPolicy, ReplayLedger,
        import_external_envelope,
    },
    error::DecapodError,
    interop, json,
    trace::redact_string,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::io::{BufRead, Write};
use std::path::Path;

/// Supported MCP protocol revisions (newest first).
pub const SUPPORTED_MCP_PROTOCOL_VERSIONS: &[&str] = &["2025-03-26", "2024-11-05"];
/// Default maximum single JSON-RPC frame size in bytes (256 KiB).
pub const DEFAULT_MAX_FRAME_BYTES: usize = 256 * 1024;
/// Default maximum response payload size in bytes (128 KiB).
pub const DEFAULT_MAX_RESPONSE_BYTES: usize = 128 * 1024;

/// Stdio framing mode for `decapod mcp serve`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum McpFramingMode {
    Auto,
    Jsonl,
    ContentLength,
}

/// Bounded configuration for the local stdio MCP server loop.
#[derive(Debug, Clone)]
pub struct McpServerConfig {
    pub repo_root: std::path::PathBuf,
    pub max_frame_bytes: usize,
    pub max_response_bytes: usize,
    pub max_requests: Option<usize>,
    pub one_shot: bool,
    pub framing: McpFramingMode,
    /// Explicit read-only mode (default true unless a valid session/authority context is supplied).
    pub read_only_session: bool,
    pub session_authenticated: bool,
    pub in_managed_worktree: bool,
    /// When true, the repository uses `backend = "cloud"` and non-todo store ops fail closed.
    pub cloud_backend: bool,
}

impl McpServerConfig {
    pub fn bounded_for_root(repo_root: impl Into<std::path::PathBuf>) -> Self {
        Self {
            repo_root: repo_root.into(),
            max_frame_bytes: DEFAULT_MAX_FRAME_BYTES,
            max_response_bytes: DEFAULT_MAX_RESPONSE_BYTES,
            max_requests: None,
            one_shot: false,
            framing: McpFramingMode::Auto,
            read_only_session: true,
            session_authenticated: false,
            in_managed_worktree: false,
            cloud_backend: false,
        }
    }
}

/// Typed MCP tool schema descriptor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpToolSchema {
    pub name: String,
    pub description: String,
    pub read_only: bool,
    pub requires_session: bool,
    pub requires_worktree: bool,
    #[serde(rename = "inputSchema")]
    pub input_schema: Value,
}

/// Typed MCP resource schema descriptor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpResourceSchema {
    pub uri: String,
    pub name: String,
    pub description: String,
    #[serde(rename = "mimeType")]
    pub mime_type: String,
    pub redacted: bool,
}

/// Complete exported MCP surface schema (tools + resources + protocol metadata).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpSurfaceSchema {
    pub schema_version: String,
    pub adapter: String,
    pub server_version: String,
    pub supported_protocol_versions: Vec<String>,
    pub transport: String,
    pub daemonless: bool,
    pub authority_boundary: String,
    pub tools: Vec<McpToolSchema>,
    pub resources: Vec<McpResourceSchema>,
}

/// Export the deterministic MCP tool and resource schema catalog.
pub fn export_mcp_schema() -> McpSurfaceSchema {
    McpSurfaceSchema {
        schema_version: "1.0.0".to_string(),
        adapter: "decapod-mcp-stdio".to_string(),
        server_version: env!("CARGO_PKG_VERSION").to_string(),
        supported_protocol_versions: SUPPORTED_MCP_PROTOCOL_VERSIONS
            .iter()
            .map(|v| (*v).to_string())
            .collect(),
        transport: "stdio".to_string(),
        daemonless: true,
        authority_boundary: "MCP transport is a bounded adapter over Decapod's local repository governance kernel. Tool discovery never grants authority; mutating tools require authenticated session and isolated worktree custody.".to_string(),
        tools: mcp_tools(),
        resources: mcp_resources(),
    }
}

pub fn mcp_tools() -> Vec<McpToolSchema> {
    vec![
        McpToolSchema {
            name: "decapod_interop_manifest".to_string(),
            description: "Return Decapod's evidence-backed interoperability manifest (read-only)."
                .to_string(),
            read_only: true,
            requires_session: false,
            requires_worktree: false,
            input_schema: json!({
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "validate": { "type": "boolean" }
                }
            }),
        },
        McpToolSchema {
            name: "decapod_interop_negotiate".to_string(),
            description: "Run deterministic fail-closed capability negotiation (read-only)."
                .to_string(),
            read_only: true,
            requires_session: false,
            requires_worktree: false,
            input_schema: json!({
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "profile_version": { "type": "string" },
                    "requested_profile": { "type": "string" },
                    "required_ops": { "type": "array", "items": { "type": "string" } },
                    "optional_ops": { "type": "array", "items": { "type": "string" } },
                    "required_extensions": { "type": "array", "items": { "type": "string" } },
                    "optional_extensions": { "type": "array", "items": { "type": "string" } }
                }
            }),
        },
        McpToolSchema {
            name: "decapod_rules_taxonomy".to_string(),
            description: "Export the machine-readable 8-domain coding-rule taxonomy and gap matrix (read-only)."
                .to_string(),
            read_only: true,
            requires_session: false,
            requires_worktree: false,
            input_schema: json!({
                "type": "object",
                "additionalProperties": false,
                "properties": {}
            }),
        },
        McpToolSchema {
            name: "decapod_overlap_classify".to_string(),
            description: "Classify semantic task-scope overlap into the 6-state matrix (read-only)."
                .to_string(),
            read_only: true,
            requires_session: false,
            requires_worktree: false,
            input_schema: json!({
                "type": "object",
                "required": ["task_a", "task_b"],
                "additionalProperties": false,
                "properties": {
                    "task_a": { "type": "object" },
                    "task_b": { "type": "object" }
                }
            }),
        },
        McpToolSchema {
            name: "decapod_envelope_verify".to_string(),
            description: "Evaluate an external governance envelope v1 under local trust policy without importing authority."
                .to_string(),
            read_only: true,
            requires_session: false,
            requires_worktree: false,
            input_schema: json!({
                "type": "object",
                "required": ["envelope", "trust_policy"],
                "additionalProperties": false,
                "properties": {
                    "envelope": { "type": "object" },
                    "trust_policy": { "type": "object" },
                    "now_epoch": { "type": "integer" }
                }
            }),
        },
        McpToolSchema {
            name: "decapod_governed_mutate".to_string(),
            description: "Policy-checked mutation gate verifying authenticated session, non-read-only role, and isolated worktree custody before delegating."
                .to_string(),
            read_only: false,
            requires_session: true,
            requires_worktree: true,
            input_schema: json!({
                "type": "object",
                "required": ["op", "params"],
                "additionalProperties": false,
                "properties": {
                    "op": { "type": "string" },
                    "params": { "type": "object" }
                }
            }),
        },
    ]
}

pub fn mcp_resources() -> Vec<McpResourceSchema> {
    vec![
        McpResourceSchema {
            uri: "decapod://manifest".to_string(),
            name: "Interop Manifest".to_string(),
            description: "Machine-readable Decapod interoperability manifest (no secrets)."
                .to_string(),
            mime_type: "application/json".to_string(),
            redacted: true,
        },
        McpResourceSchema {
            uri: "decapod://constitution/core/DECAPOD".to_string(),
            name: "Core Constitution Router".to_string(),
            description: "Bounded excerpt of the Decapod core router contract.".to_string(),
            mime_type: "text/markdown".to_string(),
            redacted: true,
        },
        McpResourceSchema {
            uri: "decapod://rules/taxonomy".to_string(),
            name: "Coding Rule Taxonomy Matrix".to_string(),
            description: "Machine-readable coding-rule taxonomy and gap analysis matrix."
                .to_string(),
            mime_type: "application/json".to_string(),
            redacted: true,
        },
        McpResourceSchema {
            uri: "decapod://harness/matrix".to_string(),
            name: "Harness Compatibility Matrix".to_string(),
            description: "Honest host integration matrix separating instruction, discovery, hook, and OS/forge surfaces."
                .to_string(),
            mime_type: "application/json".to_string(),
            redacted: true,
        },
    ]
}

/// JSON-RPC 2.0 response envelope.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JsonRpcError {
    pub code: i64,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

impl JsonRpcResponse {
    pub fn ok(id: Option<Value>, result: Value) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: Some(result),
            error: None,
        }
    }

    pub fn err(
        id: Option<Value>,
        code: i64,
        message: impl Into<String>,
        data: Option<Value>,
    ) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: None,
            error: Some(JsonRpcError {
                code,
                message: message.into(),
                data,
            }),
        }
    }
}

/// Stateful, bounded MCP JSON-RPC 2.0 router for a single local stdio lifecycle.
#[derive(Debug)]
pub struct McpServer {
    config: McpServerConfig,
    initialized: bool,
    negotiated_protocol: Option<String>,
    cancelled_ids: std::collections::BTreeSet<String>,
    replay_ledger: ReplayLedger,
    requests_handled: usize,
}

impl McpServer {
    pub fn new(config: McpServerConfig) -> Self {
        Self {
            config,
            initialized: false,
            negotiated_protocol: None,
            cancelled_ids: std::collections::BTreeSet::new(),
            replay_ledger: ReplayLedger::new(),
            requests_handled: 0,
        }
    }

    pub fn requests_handled(&self) -> usize {
        self.requests_handled
    }

    /// Process a single raw JSON-RPC 2.0 frame. Returns `Ok(None)` for notifications.
    pub fn handle_raw_frame(&mut self, raw: &[u8]) -> Option<JsonRpcResponse> {
        if raw.len() > self.config.max_frame_bytes {
            return Some(JsonRpcResponse::err(
                None,
                -32600,
                "frame exceeds configured max_frame_bytes limit",
                Some(json!({
                    "max_frame_bytes": self.config.max_frame_bytes,
                    "observed_bytes": raw.len(),
                })),
            ));
        }

        let value = match json::unique_value(raw) {
            Ok(v) => v,
            Err(e) => {
                return Some(JsonRpcResponse::err(
                    None,
                    -32700,
                    format!("parse error: {e}"),
                    None,
                ));
            }
        };

        let Some(obj) = value.as_object() else {
            return Some(JsonRpcResponse::err(
                None,
                -32600,
                "invalid request: expected JSON object",
                None,
            ));
        };

        let id = obj.get("id").cloned();
        if obj.get("jsonrpc").and_then(Value::as_str) != Some("2.0") {
            return Some(JsonRpcResponse::err(
                id,
                -32600,
                "invalid request: jsonrpc must be \"2.0\"",
                None,
            ));
        }
        let Some(method) = obj.get("method").and_then(Value::as_str) else {
            return Some(JsonRpcResponse::err(
                id,
                -32600,
                "invalid request: missing string method",
                None,
            ));
        };
        let params = obj.get("params").cloned().unwrap_or_else(|| json!({}));

        // Handle notifications without emitting a JSON-RPC response.
        if method == "notifications/initialized" {
            return None;
        }
        if method == "notifications/cancelled" {
            if let Some(req_id) = params.get("requestId") {
                self.cancelled_ids.insert(req_id.to_string());
            }
            return None;
        }

        self.requests_handled += 1;
        if let Some(ref req_id) = id
            && self.cancelled_ids.contains(&req_id.to_string())
        {
            return Some(JsonRpcResponse::err(
                id,
                -32800,
                "request cancelled by client",
                None,
            ));
        }

        let response = match method {
            "initialize" => self.handle_initialize(id, &params),
            "ping" => JsonRpcResponse::ok(id, json!({ "status": "ok", "daemonless": true })),
            "shutdown" => JsonRpcResponse::ok(id, json!({ "shutdown": true })),
            "tools/list" => self.require_initialized(id, |srv| srv.handle_tools_list()),
            "tools/call" => self.require_initialized(id, |srv| srv.handle_tools_call(&params)),
            "resources/list" => self.require_initialized(id, |srv| srv.handle_resources_list()),
            "resources/read" => {
                self.require_initialized(id, |srv| srv.handle_resources_read(&params))
            }
            _ => JsonRpcResponse::err(id, -32601, format!("method not found: {method}"), None),
        };

        Some(self.enforce_response_budget(response))
    }

    fn require_initialized<F>(&mut self, id: Option<Value>, f: F) -> JsonRpcResponse
    where
        F: FnOnce(&mut Self) -> Result<Value, (i64, String, Option<Value>)>,
    {
        if !self.initialized {
            return JsonRpcResponse::err(
                id,
                -32002,
                "server not initialized; call initialize first",
                None,
            );
        }
        match f(self) {
            Ok(result) => JsonRpcResponse::ok(id, result),
            Err((code, msg, data)) => JsonRpcResponse::err(id, code, msg, data),
        }
    }

    fn handle_initialize(&mut self, id: Option<Value>, params: &Value) -> JsonRpcResponse {
        let requested_version = params
            .get("protocolVersion")
            .and_then(Value::as_str)
            .unwrap_or(SUPPORTED_MCP_PROTOCOL_VERSIONS[0]);
        if !SUPPORTED_MCP_PROTOCOL_VERSIONS.contains(&requested_version) {
            return JsonRpcResponse::err(
                id,
                -32602,
                format!("unsupported MCP protocolVersion '{requested_version}'"),
                Some(json!({
                    "requested": requested_version,
                    "supported": SUPPORTED_MCP_PROTOCOL_VERSIONS,
                })),
            );
        }
        self.initialized = true;
        self.negotiated_protocol = Some(requested_version.to_string());
        JsonRpcResponse::ok(
            id,
            json!({
                "protocolVersion": requested_version,
                "capabilities": {
                    "tools": { "listChanged": false },
                    "resources": { "subscribe": false, "listChanged": false }
                },
                "serverInfo": {
                    "name": "decapod",
                    "version": env!("CARGO_PKG_VERSION")
                },
                "instructions": "Decapod local governance adapter. Read-only discovery never grants authority; mutations require authenticated session and isolated worktree custody."
            }),
        )
    }

    fn handle_tools_list(&self) -> Result<Value, (i64, String, Option<Value>)> {
        Ok(json!({ "tools": mcp_tools() }))
    }

    fn handle_tools_call(&mut self, params: &Value) -> Result<Value, (i64, String, Option<Value>)> {
        let name = params.get("name").and_then(Value::as_str).ok_or_else(|| {
            (
                -32602,
                "tools/call requires string 'name'".to_string(),
                None,
            )
        })?;
        let arguments = params
            .get("arguments")
            .cloned()
            .unwrap_or_else(|| json!({}));

        match name {
            "decapod_interop_manifest" => {
                let validate = arguments
                    .get("validate")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                let manifest = interop::builtin_manifest();
                let validation = if validate {
                    match interop::validate_manifest(&manifest) {
                        Ok(()) => Some(Vec::<interop::ManifestViolation>::new()),
                        Err(v) => Some(v),
                    }
                } else {
                    None
                };
                Ok(mcp_tool_json_result(
                    json!({ "manifest": manifest, "validation": validation }),
                    false,
                ))
            }
            "decapod_interop_negotiate" => {
                let req: interop::NegotiationRequest =
                    serde_json::from_value(arguments).map_err(|e| {
                        (
                            -32602,
                            format!("invalid negotiation arguments: {e}"),
                            Some(json!({ "reason": "malformed_request" })),
                        )
                    })?;
                let manifest = interop::builtin_manifest();
                match interop::negotiate(&manifest, &req) {
                    Ok(outcome) => Ok(mcp_tool_json_result(json!({ "outcome": outcome }), false)),
                    Err(refusal) => Ok(mcp_tool_json_result(
                        json!({
                            "code": refusal.code(),
                            "refusal": refusal
                        }),
                        true,
                    )),
                }
            }
            "decapod_rules_taxonomy" => {
                let matrix = super::rules::coding_rule_taxonomy_matrix();
                Ok(mcp_tool_json_result(
                    serde_json::to_value(matrix).unwrap_or_else(|_| json!({})),
                    false,
                ))
            }
            "decapod_overlap_classify" => {
                let task_a: super::rules::TaskScopeDeclaration = serde_json::from_value(
                    arguments
                        .get("task_a")
                        .cloned()
                        .ok_or_else(|| (-32602, "missing task_a".to_string(), None))?,
                )
                .map_err(|e| (-32602, format!("invalid task_a: {e}"), None))?;
                let task_b: super::rules::TaskScopeDeclaration = serde_json::from_value(
                    arguments
                        .get("task_b")
                        .cloned()
                        .ok_or_else(|| (-32602, "missing task_b".to_string(), None))?,
                )
                .map_err(|e| (-32602, format!("invalid task_b: {e}"), None))?;
                let report = super::rules::classify_task_overlap(&task_a, &task_b);
                Ok(mcp_tool_json_result(
                    serde_json::to_value(report).unwrap_or_else(|_| json!({})),
                    false,
                ))
            }
            "decapod_envelope_verify" => {
                let envelope: GovernanceEnvelopeV1 = serde_json::from_value(
                    arguments
                        .get("envelope")
                        .cloned()
                        .ok_or_else(|| (-32602, "missing envelope".to_string(), None))?,
                )
                .map_err(|e| (-32602, format!("invalid envelope: {e}"), None))?;
                let trust_policy: LocalTrustPolicy = serde_json::from_value(
                    arguments
                        .get("trust_policy")
                        .cloned()
                        .ok_or_else(|| (-32602, "missing trust_policy".to_string(), None))?,
                )
                .map_err(|e| (-32602, format!("invalid trust_policy: {e}"), None))?;
                let now_epoch = arguments
                    .get("now_epoch")
                    .and_then(Value::as_u64)
                    .unwrap_or(0);
                let report: EnvelopeImportReport = import_external_envelope(
                    &envelope,
                    &trust_policy,
                    &mut self.replay_ledger,
                    now_epoch,
                );
                let is_err = !report.accepted_as_external_evidence;
                Ok(mcp_tool_json_result(
                    serde_json::to_value(report).unwrap_or_else(|_| json!({})),
                    is_err,
                ))
            }
            "decapod_governed_mutate" => {
                // Enforce identical authority, session, and worktree isolation boundaries.
                if self.config.read_only_session {
                    return Ok(mcp_denied_result(
                        "mcp.decapod_governed_mutate",
                        "read_only_session_denied",
                        "Read-only MCP session cannot invoke mutating governance operations.",
                    ));
                }
                if !self.config.session_authenticated {
                    return Ok(mcp_denied_result(
                        "mcp.decapod_governed_mutate",
                        "session_required",
                        "Authenticated Decapod session required before mutation.",
                    ));
                }
                let op = arguments
                    .get("op")
                    .and_then(Value::as_str)
                    .ok_or_else(|| (-32602, "missing string 'op'".to_string(), None))?;
                if !interop::RPC_OPS.contains(&op) {
                    return Ok(mcp_denied_result(
                        op,
                        "unknown_op",
                        &format!("Operation '{op}' is not registered in Decapod RPC_OPS."),
                    ));
                }
                if self.config.cloud_backend
                    && crate::core::cloud_parity_matrix::is_cloud_unsupported_rpc_op(op)
                {
                    return Ok(mcp_denied_result(
                        op,
                        "cloud_backend_unavailable",
                        "CLOUD_BACKEND_UNAVAILABLE: non-todo local store operations are unsupported under backend = \"cloud\" without SQLite fallback.",
                    ));
                }
                if !self.config.in_managed_worktree {
                    return Ok(mcp_denied_result(
                        "mcp.decapod_governed_mutate",
                        "worktree_isolation_required",
                        "Mutating operation refused outside an isolated .decapod/workspaces/* worktree.",
                    ));
                }
                Ok(mcp_tool_json_result(
                    json!({
                        "status": "admitted_for_kernel_dispatch",
                        "op": op,
                        "authority_note": "Kernel session and worktree gates satisfied; execution delegates to canonical Decapod RPC handler."
                    }),
                    false,
                ))
            }
            _ => Err((-32602, format!("unknown tool '{name}'"), None)),
        }
    }

    fn handle_resources_list(&self) -> Result<Value, (i64, String, Option<Value>)> {
        Ok(json!({ "resources": mcp_resources() }))
    }

    fn handle_resources_read(&self, params: &Value) -> Result<Value, (i64, String, Option<Value>)> {
        let uri = params.get("uri").and_then(Value::as_str).ok_or_else(|| {
            (
                -32602,
                "resources/read requires string 'uri'".to_string(),
                None,
            )
        })?;

        // Explicitly block path traversal, hidden rubrics, secret files, and raw DB access.
        let lower = uri.to_ascii_lowercase();
        if uri.contains("..")
            || uri.contains('\\')
            || lower.contains("rubric")
            || lower.contains("secret")
            || lower.contains(".env")
            || lower.contains(".db")
            || lower.contains("sqlite")
            || lower.contains("token")
            || !uri.starts_with("decapod://")
        {
            return Err((
                -32602,
                "resource access denied: prohibited or out-of-scope URI".to_string(),
                Some(json!({ "uri": redact_string(uri), "code": "resource_denied" })),
            ));
        }

        let (mime_type, raw_text) = match uri {
            "decapod://manifest" => {
                let m = interop::builtin_manifest();
                (
                    "application/json",
                    serde_json::to_string_pretty(&m).unwrap_or_else(|_| "{}".to_string()),
                )
            }
            "decapod://constitution/core/DECAPOD" => (
                "text/markdown",
                "# Decapod Core Router (Bounded Resource Excerpt)\n\n\
                 - Repo-local state and proof policies are sole authority.\n\
                 - Claim a todo and enter `.decapod/workspaces/*` before mutating.\n\
                 - Run `decapod validate` to establish proof.\n"
                    .to_string(),
            ),
            "decapod://rules/taxonomy" => {
                let matrix = super::rules::coding_rule_taxonomy_matrix();
                (
                    "application/json",
                    serde_json::to_string_pretty(&matrix).unwrap_or_else(|_| "{}".to_string()),
                )
            }
            "decapod://harness/matrix" => {
                let matrix = super::registry::harness_compatibility_matrix();
                (
                    "application/json",
                    serde_json::to_string_pretty(&matrix).unwrap_or_else(|_| "{}".to_string()),
                )
            }
            _ => {
                return Err((
                    -32002,
                    format!("resource not found: {}", redact_string(uri)),
                    None,
                ));
            }
        };

        let safe_text = redact_string(&raw_text);
        Ok(json!({
            "contents": [{
                "uri": uri,
                "mimeType": mime_type,
                "text": safe_text
            }]
        }))
    }

    fn enforce_response_budget(&self, response: JsonRpcResponse) -> JsonRpcResponse {
        let serialized = serde_json::to_vec(&response).unwrap_or_default();
        if serialized.len() > self.config.max_response_bytes {
            return JsonRpcResponse::err(
                response.id,
                -32001,
                "response exceeds configured max_response_bytes limit",
                Some(json!({
                    "max_response_bytes": self.config.max_response_bytes,
                    "observed_bytes": serialized.len(),
                })),
            );
        }
        response
    }

    /// Run a bounded stdio request/response loop over arbitrary `BufRead` / `Write` streams.
    /// Exits cleanly on EOF, `--one-shot`, `--max-requests`, or `shutdown` leaving no daemon.
    pub fn serve_stdio<R: BufRead, W: Write>(
        &mut self,
        mut reader: R,
        mut writer: W,
    ) -> Result<usize, DecapodError> {
        let limit = if self.config.one_shot {
            Some(1usize)
        } else {
            self.config.max_requests
        };
        let mut processed = 0usize;

        while limit.is_none_or(|max| processed < max) {
            let Some((frame, used_content_length)) = read_next_frame(
                &mut reader,
                self.config.framing,
                self.config.max_frame_bytes,
            )?
            else {
                break;
            };

            let is_shutdown = serde_json::from_slice::<Value>(&frame)
                .ok()
                .and_then(|v| v.get("method").and_then(Value::as_str).map(str::to_owned))
                .as_deref()
                == Some("shutdown");

            if let Some(resp) = self.handle_raw_frame(&frame) {
                write_frame(&mut writer, &resp, used_content_length)?;
                processed += 1;
            }
            if is_shutdown {
                break;
            }
        }
        Ok(processed)
    }
}

fn mcp_denied_result(op: &str, code: &str, message: &str) -> Value {
    let recovery = crate::core::rpc::derive_failure_recovery(op, code, message, None, &[]);
    mcp_tool_json_result(
        json!({
            "status": "denied",
            "code": code,
            "message": message,
            "failure_class": recovery.failure_class,
            "recovery_mode": recovery.recovery_mode,
            "blocked_by": [recovery.blocker],
            "allowed_next_ops": recovery.allowed_next_ops,
            "context_capsule": recovery.context_capsule,
        }),
        true,
    )
}

fn mcp_tool_json_result(payload: Value, is_error: bool) -> Value {
    let text = redact_string(&serde_json::to_string(&payload).unwrap_or_else(|_| "{}".to_string()));
    json!({
        "content": [{
            "type": "text",
            "text": text
        }],
        "structuredContent": payload,
        "isError": is_error
    })
}

fn read_next_frame<R: BufRead>(
    reader: &mut R,
    mode: McpFramingMode,
    max_frame_bytes: usize,
) -> Result<Option<(Vec<u8>, bool)>, DecapodError> {
    let mut first_line = String::new();
    loop {
        first_line.clear();
        let n = reader
            .read_line(&mut first_line)
            .map_err(DecapodError::IoError)?;
        if n == 0 {
            return Ok(None);
        }
        if !first_line.trim().is_empty() {
            break;
        }
    }

    let trimmed = first_line.trim();
    let is_header = trimmed.to_ascii_lowercase().starts_with("content-length:");

    if is_header && mode != McpFramingMode::Jsonl {
        let len_str = trimmed
            .split_once(':')
            .map(|(_, v)| v.trim())
            .unwrap_or_default();
        let content_len: usize = len_str.parse().map_err(|_| {
            DecapodError::ValidationError(format!("invalid Content-Length header: '{trimmed}'"))
        })?;
        // Consume remaining headers until empty line.
        let mut header_line = String::new();
        loop {
            header_line.clear();
            let n = reader
                .read_line(&mut header_line)
                .map_err(DecapodError::IoError)?;
            if n == 0 || header_line.trim().is_empty() {
                break;
            }
        }
        if content_len > max_frame_bytes {
            // Drain safely up to cap + 1 and let handler emit structured frame-too-large error.
            let mut oversized = vec![0u8; max_frame_bytes + 1];
            let _ = reader.read(&mut oversized);
            return Ok(Some((oversized, true)));
        }
        let mut body = vec![0u8; content_len];
        reader
            .read_exact(&mut body)
            .map_err(DecapodError::IoError)?;
        return Ok(Some((body, true)));
    }

    Ok(Some((trimmed.as_bytes().to_vec(), false)))
}

fn write_frame<W: Write>(
    writer: &mut W,
    response: &JsonRpcResponse,
    content_length_framing: bool,
) -> Result<(), DecapodError> {
    let bytes = serde_json::to_vec(response)
        .map_err(|e| DecapodError::ValidationError(format!("failed to encode MCP frame: {e}")))?;
    if content_length_framing {
        write!(writer, "Content-Length: {}\r\n\r\n", bytes.len()).map_err(DecapodError::IoError)?;
        writer.write_all(&bytes).map_err(DecapodError::IoError)?;
    } else {
        writer.write_all(&bytes).map_err(DecapodError::IoError)?;
        writer.write_all(b"\n").map_err(DecapodError::IoError)?;
    }
    writer.flush().map_err(DecapodError::IoError)?;
    Ok(())
}

/// Generate a scoped MCP server configuration entry for host bundling.
pub fn generate_mcp_host_config(
    executable_path: &Path,
    repo_root: &Path,
) -> Result<Value, DecapodError> {
    if executable_path.as_os_str().is_empty() {
        return Err(DecapodError::ValidationError(
            "MCP host config requires a non-empty executable path".to_string(),
        ));
    }
    if repo_root.as_os_str().is_empty() {
        return Err(DecapodError::ValidationError(
            "MCP host config requires a non-empty repository root".to_string(),
        ));
    }
    Ok(json!({
        "mcpServers": {
            "decapod": {
                "command": executable_path.to_string_lossy(),
                "args": ["mcp", "serve"],
                "cwd": repo_root.to_string_lossy(),
                "env": {
                    "DECAPOD_MCP_VERSION": env!("CARGO_PKG_VERSION")
                }
            }
        }
    }))
}

/// Conflict-safe merge of Decapod's MCP server entry into an existing host JSON config,
/// preserving all unrelated user keys and servers.
pub fn merge_mcp_host_config(existing: &Value, generated: &Value) -> Result<Value, DecapodError> {
    let Some(mut merged) = existing.as_object().cloned() else {
        return Err(DecapodError::ValidationError(
            "existing host MCP config must be a JSON object".to_string(),
        ));
    };
    let decapod_entry = generated
        .get("mcpServers")
        .and_then(|m| m.get("decapod"))
        .cloned()
        .ok_or_else(|| {
            DecapodError::ValidationError("generated config missing mcpServers.decapod".to_string())
        })?;
    let servers = merged
        .entry("mcpServers".to_string())
        .or_insert_with(|| json!({}));
    let Some(servers_obj) = servers.as_object_mut() else {
        return Err(DecapodError::ValidationError(
            "existing host config 'mcpServers' must be a JSON object".to_string(),
        ));
    };
    servers_obj.insert("decapod".to_string(), decapod_entry);
    Ok(Value::Object(merged))
}

/// Remove only Decapod's entry from a host MCP config, preserving all other user settings.
pub fn remove_mcp_host_config(existing: &Value) -> Result<Value, DecapodError> {
    let Some(mut updated) = existing.as_object().cloned() else {
        return Err(DecapodError::ValidationError(
            "existing host MCP config must be a JSON object".to_string(),
        ));
    };
    if let Some(servers) = updated.get_mut("mcpServers").and_then(Value::as_object_mut) {
        servers.remove("decapod");
    }
    Ok(Value::Object(updated))
}
