//! Optional authenticated read-only loopback HTTP server for observability
//! projections (#874, #1719, #1720).
//!
//! Strictly read-only, daemonless-bounded (`--max-requests`), constant-time
//! Bearer authentication, loopback-by-default binding guard, foreign Origin /
//! Host rejection, tenant/repository scope checks, revocation list, and
//! refusal of every mutating operation (`403 read_only_enforced` / `405`).

use crate::core::error::DecapodError;
use crate::core::http_transport::{
    self, HttpRequest, HttpResponse, HttpTransportConfig, bind_checked_listener, error_code_json,
    extract_rpc_op, serve_on_listener,
};
use crate::core::observability_projections::pr_manifest::{PrManifestQuery, project_pr_manifest};
use crate::core::observability_projections::provenance::{
    ContextProvenanceQuery, project_context_provenance,
};
use crate::core::observability_projections::snapshot::compute_repo_id;
use crate::core::observability_projections::specs::{
    SpecsProjectionQuery, project_specs_and_governance,
};
use crate::core::observability_projections::timeline::{
    TimelineProjectionQuery, project_activity_timeline,
};
use crate::core::observability_projections::validation_plan::{
    ValidationPlanQuery, project_validation_plan,
};
use crate::core::observability_projections::workspace::{
    WorkspaceProjectionQuery, project_workspace_observation,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::{SocketAddr, TcpListener};
use std::path::Path;
use std::time::Duration;

/// Strictly allowlisted read-only RPC operations permitted on the observability
/// HTTP surface (#1719, #1720).
pub const ALLOWLISTED_OBSERVE_RPC_OPS: &[&str] = &[
    "observe.specs",
    "observe.timeline",
    "observe.workspace",
    "observe.validation_plan",
    "observe.pr_manifest",
    "observe.context_provenance",
];

/// Data contract and least-privilege authorization policy for optional
/// authenticated read projections (#1719).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReadProjectionAccessPolicy {
    pub schema_version: String,
    pub credential_scope: String,
    pub allow_mutations: bool,
    pub allow_browser_mutations: bool,
    pub require_loopback: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tenant_id: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub allowed_repo_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub revoked_token_ids: Vec<String>,
    pub offline_local_without_login: bool,
}

impl Default for ReadProjectionAccessPolicy {
    fn default() -> Self {
        Self {
            schema_version: "decapod.observe-access-policy.v1".to_string(),
            credential_scope: "read_only".to_string(),
            allow_mutations: false,
            allow_browser_mutations: false,
            require_loopback: true,
            tenant_id: None,
            allowed_repo_ids: Vec::new(),
            revoked_token_ids: Vec::new(),
            offline_local_without_login: true,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ObserveServeOptions {
    pub bind: String,
    pub auth_token: String,
    pub allow_remote: bool,
    pub max_requests: Option<usize>,
    pub max_body_bytes: usize,
    pub max_response_bytes: usize,
    pub read_timeout: Duration,
    pub write_timeout: Duration,
    pub policy: ReadProjectionAccessPolicy,
    pub allowed_origins: Vec<String>,
}

impl ObserveServeOptions {
    pub fn loopback_default(auth_token: impl Into<String>) -> Self {
        Self {
            bind: "127.0.0.1:0".to_string(),
            auth_token: auth_token.into(),
            allow_remote: false,
            max_requests: None,
            max_body_bytes: 256 * 1024,
            max_response_bytes: http_transport::DEFAULT_MAX_RESPONSE_BYTES,
            read_timeout: Duration::from_secs(5),
            write_timeout: Duration::from_secs(5),
            policy: ReadProjectionAccessPolicy::default(),
            allowed_origins: Vec::new(),
        }
    }
}

fn build_transport_config(repo_root: &Path, options: &ObserveServeOptions) -> HttpTransportConfig {
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
    cfg.read_only_only = true;
    cfg.allowed_read_ops = ALLOWLISTED_OBSERVE_RPC_OPS
        .iter()
        .map(|s| (*s).to_string())
        .collect();
    cfg.required_tenant_id = options.policy.tenant_id.clone();
    cfg.allowed_repo_ids = options.policy.allowed_repo_ids.clone();
    cfg.revoked_token_ids = options.policy.revoked_token_ids.clone();
    cfg
}

/// Bind and serve the strictly read-only observability HTTP endpoint.
pub fn serve_observe_http(
    repo_root: &Path,
    options: ObserveServeOptions,
) -> Result<SocketAddr, DecapodError> {
    let (listener, addr) =
        bind_checked_listener(&options.bind, &options.auth_token, options.allow_remote)
            .map_err(DecapodError::IoError)?;
    eprintln!("decapod observe HTTP listening on {addr} (read_only=true)");
    serve_observe_on_listener(repo_root, listener, options)?;
    Ok(addr)
}

/// Serve read-only observability projections on a pre-bound `TcpListener`
/// (zero port races in integration tests and operator wrappers).
pub fn serve_observe_on_listener(
    repo_root: &Path,
    listener: TcpListener,
    options: ObserveServeOptions,
) -> Result<(), DecapodError> {
    let config = build_transport_config(repo_root, &options);
    let root = repo_root.to_path_buf();
    let repo_id = compute_repo_id(&root);
    serve_on_listener(listener, config, move |req| {
        dispatch_observe_http_request(&root, &repo_id, req)
    })
    .map_err(DecapodError::IoError)
}

fn parse_query_params(query: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for pair in query.split('&') {
        if pair.is_empty() {
            continue;
        }
        let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
        map.insert(k.to_string(), percent_decode(v));
    }
    map
}

fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let Ok(hex) = std::str::from_utf8(&bytes[i + 1..i + 3])
            && let Ok(val) = u8::from_str_radix(hex, 16)
        {
            out.push(val);
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).to_string()
}

/// Dispatch a single already-authenticated, read-only HTTP request.
pub fn dispatch_observe_http_request(
    repo_root: &Path,
    repo_id: &str,
    req: &HttpRequest,
) -> HttpResponse {
    if req.method == "GET" {
        let params = parse_query_params(&req.query);
        let offset = params
            .get("offset")
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(0);
        let limit = params
            .get("limit")
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(64);
        return match req.path.as_str() {
            "/observe/v1/specs" => {
                let q = SpecsProjectionQuery {
                    offset,
                    limit,
                    include_excerpts: params
                        .get("include_excerpts")
                        .map(|v| v == "true" || v == "1")
                        .unwrap_or(false),
                    anchor_path: params.get("path").cloned(),
                    start_line: params
                        .get("start_line")
                        .and_then(|v| v.parse::<usize>().ok())
                        .unwrap_or(1),
                    end_line: params
                        .get("end_line")
                        .and_then(|v| v.parse::<usize>().ok())
                        .unwrap_or(0),
                };
                encode_projection_result(project_specs_and_governance(repo_root, &q), |p| {
                    p.snapshot.snapshot_id.clone()
                })
            }
            "/observe/v1/timeline" => {
                let q = TimelineProjectionQuery {
                    after_seq: params
                        .get("after_seq")
                        .and_then(|v| v.parse::<u64>().ok())
                        .unwrap_or(0),
                    limit,
                    stream: params.get("stream").cloned(),
                };
                encode_projection_result(project_activity_timeline(repo_root, &q), |p| {
                    p.snapshot.snapshot_id.clone()
                })
            }
            "/observe/v1/workspace" => {
                let q = WorkspaceProjectionQuery {
                    offset,
                    limit,
                    anchor_path: params.get("path").cloned(),
                    start_line: params
                        .get("start_line")
                        .and_then(|v| v.parse::<usize>().ok())
                        .unwrap_or(1),
                    end_line: params
                        .get("end_line")
                        .and_then(|v| v.parse::<usize>().ok())
                        .unwrap_or(0),
                    ..WorkspaceProjectionQuery::default()
                };
                encode_projection_result(project_workspace_observation(repo_root, &q), |p| {
                    p.snapshot.snapshot_id.clone()
                })
            }
            "/observe/v1/validation-plan" => {
                let q = ValidationPlanQuery::default();
                encode_projection_result(project_validation_plan(repo_root, &q), |p| {
                    p.snapshot.snapshot_id.clone()
                })
            }
            "/observe/v1/pr-manifest" => {
                let q = PrManifestQuery::default();
                encode_projection_result(project_pr_manifest(repo_root, &q), |p| {
                    p.snapshot.snapshot_id.clone()
                })
            }
            "/observe/v1/context-provenance" => {
                let q = ContextProvenanceQuery {
                    offset,
                    limit,
                    task_id: params.get("task_id").cloned(),
                    expected_repo_id: params
                        .get("repo_id")
                        .cloned()
                        .or_else(|| Some(repo_id.to_string())),
                };
                encode_projection_result(project_context_provenance(repo_root, &q), |p| {
                    p.snapshot.snapshot_id.clone()
                })
            }
            _ => HttpResponse::json(
                404,
                error_code_json("unknown_projection", "unknown /observe/v1 projection route"),
            ),
        };
    }

    // POST /rpc/v1 (already allowlist-checked by http_transport)
    let Some(op) = extract_rpc_op(&req.body) else {
        return HttpResponse::json(
            400,
            error_code_json("invalid_request", "missing 'op' field"),
        );
    };
    let parsed_body: serde_json::Value = match serde_json::from_slice(&req.body) {
        Ok(v) => v,
        Err(e) => {
            return HttpResponse::json(
                400,
                error_code_json("invalid_json", &format!("malformed JSON body: {e}")),
            );
        }
    };
    let params = parsed_body
        .get("params")
        .cloned()
        .unwrap_or_else(|| serde_json::json!({}));

    match dispatch_observe_rpc_op(repo_root, &op, &params) {
        Ok((value, snapshot_id)) => {
            let body = serde_json::to_vec(&serde_json::json!({
                "success": true,
                "read_only": true,
                "authoritative": false,
                "op": op,
                "data": value,
            }))
            .unwrap_or_else(|_| b"{\"success\":false}".to_vec());
            HttpResponse::json(200, body)
                .with_header("X-Decapod-Read-Only", "true")
                .with_header("X-Decapod-Snapshot-Id", snapshot_id)
        }
        Err(err) => {
            let status = if err.to_string().contains("BOUNDARY_VIOLATION")
                || err.to_string().contains("SECRET_PATH_DENIED")
                || err.to_string().contains("CROSS_SCOPE_DENIED")
            {
                403
            } else {
                400
            };
            HttpResponse::json(status, error_code_json("observe_error", &err.to_string()))
                .with_header("X-Decapod-Read-Only", "true")
        }
    }
}

fn encode_projection_result<T, F>(res: Result<T, DecapodError>, get_snap: F) -> HttpResponse
where
    T: Serialize,
    F: FnOnce(&T) -> String,
{
    match res {
        Ok(val) => {
            let snap_id = get_snap(&val);
            match serde_json::to_vec(&val) {
                Ok(body) => HttpResponse::json(200, body)
                    .with_header("X-Decapod-Read-Only", "true")
                    .with_header("X-Decapod-Snapshot-Id", snap_id),
                Err(e) => {
                    HttpResponse::json(500, error_code_json("serialization_error", &e.to_string()))
                }
            }
        }
        Err(err) => {
            let status = if err.to_string().contains("BOUNDARY_VIOLATION")
                || err.to_string().contains("SECRET_PATH_DENIED")
                || err.to_string().contains("CROSS_SCOPE_DENIED")
            {
                403
            } else {
                400
            };
            HttpResponse::json(status, error_code_json("observe_error", &err.to_string()))
                .with_header("X-Decapod-Read-Only", "true")
        }
    }
}

/// Pure read-only dispatcher shared by `decapod rpc --op observe.*` and
/// `decapod observe serve`.
pub fn dispatch_observe_rpc_op(
    repo_root: &Path,
    op: &str,
    params: &serde_json::Value,
) -> Result<(serde_json::Value, String), DecapodError> {
    match op {
        "observe.specs" => {
            let q: SpecsProjectionQuery =
                serde_json::from_value(params.clone()).unwrap_or_default();
            let proj = project_specs_and_governance(repo_root, &q)?;
            let snap = proj.snapshot.snapshot_id.clone();
            Ok((
                serde_json::to_value(proj)
                    .map_err(|e| DecapodError::ValidationError(e.to_string()))?,
                snap,
            ))
        }
        "observe.timeline" => {
            let q: TimelineProjectionQuery =
                serde_json::from_value(params.clone()).unwrap_or_default();
            let proj = project_activity_timeline(repo_root, &q)?;
            let snap = proj.snapshot.snapshot_id.clone();
            Ok((
                serde_json::to_value(proj)
                    .map_err(|e| DecapodError::ValidationError(e.to_string()))?,
                snap,
            ))
        }
        "observe.workspace" => {
            let q: WorkspaceProjectionQuery =
                serde_json::from_value(params.clone()).unwrap_or_default();
            let proj = project_workspace_observation(repo_root, &q)?;
            let snap = proj.snapshot.snapshot_id.clone();
            Ok((
                serde_json::to_value(proj)
                    .map_err(|e| DecapodError::ValidationError(e.to_string()))?,
                snap,
            ))
        }
        "observe.validation_plan" => {
            let q: ValidationPlanQuery = serde_json::from_value(params.clone()).unwrap_or_default();
            let proj = project_validation_plan(repo_root, &q)?;
            let snap = proj.snapshot.snapshot_id.clone();
            Ok((
                serde_json::to_value(proj)
                    .map_err(|e| DecapodError::ValidationError(e.to_string()))?,
                snap,
            ))
        }
        "observe.pr_manifest" => {
            let q: PrManifestQuery = serde_json::from_value(params.clone()).unwrap_or_default();
            let proj = project_pr_manifest(repo_root, &q)?;
            let snap = proj.snapshot.snapshot_id.clone();
            Ok((
                serde_json::to_value(proj)
                    .map_err(|e| DecapodError::ValidationError(e.to_string()))?,
                snap,
            ))
        }
        "observe.context_provenance" => {
            let q: ContextProvenanceQuery =
                serde_json::from_value(params.clone()).unwrap_or_default();
            let proj = project_context_provenance(repo_root, &q)?;
            let snap = proj.snapshot.snapshot_id.clone();
            Ok((
                serde_json::to_value(proj)
                    .map_err(|e| DecapodError::ValidationError(e.to_string()))?,
                snap,
            ))
        }
        other => Err(DecapodError::ValidationError(format!(
            "READ_ONLY_ENFORCED: operation '{other}' is not an allowlisted read-only observe.* projection"
        ))),
    }
}
