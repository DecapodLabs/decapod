//! Small, opt-in HTTP adapter for the transport-neutral RPC profile and
//! read-only observability projections (#874, #1719, #1720).
//!
//! This module deliberately owns only HTTP framing, constant-time bearer
//! authentication, loopback/origin/host enforcement, request-bound idempotency
//! and replay protection, tenant/repository scope checks, and bounded I/O.
//! Semantic authority remains in the existing local RPC/projection kernel.

#![allow(clippy::disallowed_types)]

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{IpAddr, SocketAddr, TcpListener, TcpStream};
use std::time::{Duration, Instant};

pub const MAX_HEADER_BYTES: usize = 32 * 1024;
pub const DEFAULT_MAX_BODY_BYTES: usize = 1_048_576;
pub const DEFAULT_MAX_RESPONSE_BYTES: usize = 4 * 1024 * 1024;
pub const DEFAULT_MAX_CACHE_ENTRIES: usize = 256;
pub const DEFAULT_MAX_CACHE_BYTES: usize = 4 * 1024 * 1024;
pub const DEFAULT_REPLAY_WINDOW_SECS: u64 = 300;
pub const DEFAULT_READ_TIMEOUT_SECS: u64 = 5;
pub const DEFAULT_WRITE_TIMEOUT_SECS: u64 = 5;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpResponse {
    pub status: u16,
    pub body: Vec<u8>,
    pub content_type: &'static str,
    pub headers: Vec<(String, String)>,
    /// Marks responses whose underlying mutation outcome is uncertain (e.g.,
    /// subprocess timeout or cancellation) so retries reconcile instead of
    /// blindly re-executing.
    pub uncertain_outcome: bool,
}

impl HttpResponse {
    pub fn json(status: u16, body: Vec<u8>) -> Self {
        Self {
            status,
            body,
            content_type: "application/json",
            headers: Vec::new(),
            uncertain_outcome: false,
        }
    }

    pub fn json_uncertain(status: u16, body: Vec<u8>) -> Self {
        Self {
            status,
            body,
            content_type: "application/json",
            headers: Vec::new(),
            uncertain_outcome: true,
        }
    }

    pub fn with_header(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.push((name.into(), value.into()));
        self
    }
}

#[derive(Debug, Clone)]
pub struct HttpRequest {
    pub method: String,
    pub path: String,
    pub query: String,
    pub headers: HashMap<String, String>,
    pub body: Vec<u8>,
}

/// Explicit, machine-readable replay contract surfaced in error envelopes.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReplayContract {
    pub restart_scope: String,
    pub eviction_policy: String,
    pub replay_window_secs: u64,
    pub max_cache_entries: usize,
    pub max_cache_bytes: usize,
    pub reconciliation_required_on_unknown: bool,
}

#[derive(Debug, Clone)]
pub struct HttpTransportConfig {
    pub bind: String,
    pub auth_token: String,
    pub allow_remote: bool,
    pub max_body_bytes: usize,
    pub max_response_bytes: usize,
    pub max_cache_entries: usize,
    pub max_cache_bytes: usize,
    pub replay_window: Duration,
    pub read_timeout: Duration,
    pub write_timeout: Duration,
    /// Optional bounded request count (`INV-DAEMONLESS` cap for tests/operators).
    pub max_requests: Option<usize>,
    /// Canonical repository context included in every idempotency binding digest.
    pub repository_context: String,
    /// Explicit additional allowed Origin values when remote mode is enabled.
    pub allowed_origins: Vec<String>,
    /// When true, strictly enforce read-only access (GET /observe/v1/* and
    /// allowlisted read-only POST /rpc/v1 operations only).
    pub read_only_only: bool,
    /// Allowlisted RPC operations when `read_only_only` is true.
    pub allowed_read_ops: Vec<String>,
    /// Optional tenant scope enforcement (#1719).
    pub required_tenant_id: Option<String>,
    /// Optional allowed repository IDs enforcement (#1719).
    pub allowed_repo_ids: Vec<String>,
    /// Explicitly revoked token IDs denied on `X-Decapod-Token-Id` (#1719).
    pub revoked_token_ids: Vec<String>,
}

impl HttpTransportConfig {
    pub fn for_rpc(
        bind: impl Into<String>,
        auth_token: impl Into<String>,
        allow_remote: bool,
        max_body_bytes: usize,
        repository_context: impl Into<String>,
    ) -> Self {
        Self {
            bind: bind.into(),
            auth_token: auth_token.into(),
            allow_remote,
            max_body_bytes: max_body_bytes.max(1024),
            max_response_bytes: DEFAULT_MAX_RESPONSE_BYTES,
            max_cache_entries: DEFAULT_MAX_CACHE_ENTRIES,
            max_cache_bytes: DEFAULT_MAX_CACHE_BYTES,
            replay_window: Duration::from_secs(DEFAULT_REPLAY_WINDOW_SECS),
            read_timeout: Duration::from_secs(DEFAULT_READ_TIMEOUT_SECS),
            write_timeout: Duration::from_secs(DEFAULT_WRITE_TIMEOUT_SECS),
            max_requests: None,
            repository_context: repository_context.into(),
            allowed_origins: Vec::new(),
            read_only_only: false,
            allowed_read_ops: Vec::new(),
            required_tenant_id: None,
            allowed_repo_ids: Vec::new(),
            revoked_token_ids: Vec::new(),
        }
    }

    pub fn replay_contract(&self) -> ReplayContract {
        ReplayContract {
            restart_scope: "process_local_volatile".to_string(),
            eviction_policy: "fail_closed_within_window".to_string(),
            replay_window_secs: self.replay_window.as_secs(),
            max_cache_entries: self.max_cache_entries,
            max_cache_bytes: self.max_cache_bytes,
            reconciliation_required_on_unknown: true,
        }
    }
}

fn constant_time_eq_bytes(a: &[u8], b: &[u8]) -> bool {
    let mut diff: u8 = if a.len() == b.len() { 0 } else { 1 };
    let max_len = a.len().max(b.len());
    for i in 0..max_len {
        let ab = a.get(i).copied().unwrap_or(0);
        let bb = b.get(i).copied().unwrap_or(0);
        diff |= std::hint::black_box(ab ^ bb);
    }
    std::hint::black_box(diff) == 0
}

/// Constant-time Bearer token check using SHA-256 digest equality plus exact
/// bytewise constant-time comparison.
pub fn verify_bearer_token(header: Option<&str>, expected_token: &str) -> bool {
    if expected_token.trim().is_empty() {
        return false;
    }
    let Some(raw) = header else {
        return false;
    };
    let Some((scheme, provided)) = raw.split_once(' ') else {
        return false;
    };
    if !scheme.eq_ignore_ascii_case("bearer") || provided.is_empty() {
        return false;
    }
    let provided_digest = Sha256::digest(provided.as_bytes());
    let expected_digest = Sha256::digest(expected_token.as_bytes());
    let digest_ok = constant_time_eq_bytes(&provided_digest, &expected_digest);
    let exact_ok = constant_time_eq_bytes(provided.as_bytes(), expected_token.as_bytes());
    digest_ok && exact_ok
}

/// Check whether a Host or Origin host[:port] strictly refers to loopback.
pub fn is_loopback_host(host_header: &str) -> bool {
    let trimmed = host_header.trim();
    if trimmed.is_empty() {
        return false;
    }
    let host_only = if let Some(rest) = trimmed.strip_prefix('[') {
        let Some((addr, tail)) = rest.split_once(']') else {
            return false;
        };
        if !tail.is_empty() {
            let Some(port) = tail.strip_prefix(':') else {
                return false;
            };
            if port.is_empty() || !port.bytes().all(|b| b.is_ascii_digit()) {
                return false;
            }
        }
        addr
    } else if let Some((addr, port)) = trimmed.rsplit_once(':') {
        if port.is_empty() || !port.bytes().all(|b| b.is_ascii_digit()) {
            return false;
        }
        addr
    } else {
        trimmed
    };

    if host_only.eq_ignore_ascii_case("localhost") {
        return true;
    }
    host_only
        .parse::<IpAddr>()
        .map(|ip| ip.is_loopback())
        .unwrap_or(false)
}

/// Reject foreign `Origin` or `Host` headers when running in loopback mode
/// (protects local operators from DNS rebinding and cross-site browser probes).
pub fn validate_origin_and_host(
    headers: &HashMap<String, String>,
    allow_remote: bool,
    allowed_origins: &[String],
) -> Result<(), HttpResponse> {
    if let Some(origin) = headers.get("origin") {
        let trimmed = origin.trim();
        let explicitly_allowed = allowed_origins.iter().any(|allowed| allowed == trimmed);
        if !explicitly_allowed {
            let origin_host = trimmed
                .strip_prefix("http://")
                .or_else(|| trimmed.strip_prefix("https://"))
                .filter(|rest| !rest.contains('/') && !rest.contains('@') && !rest.contains('?'));
            let is_loopback_origin = origin_host.is_some_and(is_loopback_host);
            if !is_loopback_origin {
                return Err(HttpResponse::json(
                    403,
                    error_code_json(
                        "forbidden_origin",
                        "foreign Origin header rejected by transport security boundary",
                    ),
                ));
            }
        }
    }
    if !allow_remote
        && let Some(host) = headers.get("host")
        && !is_loopback_host(host)
    {
        return Err(HttpResponse::json(
            403,
            error_code_json(
                "forbidden_host",
                "non-loopback Host header rejected in loopback mode",
            ),
        ));
    }
    Ok(())
}

/// Bind an `TcpListener` enforcing non-empty token and loopback-by-default.
pub fn bind_checked_listener(
    bind: &str,
    auth_token: &str,
    allow_remote: bool,
) -> std::io::Result<(TcpListener, SocketAddr)> {
    if auth_token.trim().is_empty() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "authenticated HTTP transport requires a non-empty bearer token",
        ));
    }
    let listener = TcpListener::bind(bind)?;
    let address = listener.local_addr()?;
    if !allow_remote && !address.ip().is_loopback() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "remote HTTP binding requires --allow-remote",
        ));
    }
    Ok((listener, address))
}

#[derive(Debug, Clone)]
enum CachedOutcome {
    Final(HttpResponse),
    Unknown { status: u16, reason: String },
}

#[derive(Debug, Clone)]
struct IdempotencyEntry {
    request_binding_digest: String,
    outcome: CachedOutcome,
    byte_weight: usize,
    expires_at: Instant,
}

#[derive(Debug)]
struct ReplayStore {
    max_entries: usize,
    max_bytes: usize,
    window: Duration,
    idempotency: HashMap<String, IdempotencyEntry>,
    request_ids: HashMap<String, Instant>,
    total_cached_bytes: usize,
}

impl ReplayStore {
    fn new(max_entries: usize, max_bytes: usize, window: Duration) -> Self {
        Self {
            max_entries: max_entries.max(1),
            max_bytes: max_bytes.max(1024),
            window,
            idempotency: HashMap::new(),
            request_ids: HashMap::new(),
            total_cached_bytes: 0,
        }
    }

    fn prune_expired(&mut self, now: Instant) {
        self.request_ids.retain(|_, exp| *exp > now);
        let expired_keys: Vec<String> = self
            .idempotency
            .iter()
            .filter(|(_, entry)| entry.expires_at <= now)
            .map(|(k, _)| k.clone())
            .collect();
        for key in expired_keys {
            if let Some(entry) = self.idempotency.remove(&key) {
                self.total_cached_bytes = self.total_cached_bytes.saturating_sub(entry.byte_weight);
            }
        }
    }

    fn has_capacity_for_request_id(&self) -> bool {
        self.request_ids.len() < self.max_entries
    }

    fn has_capacity_for_idempotency(&self, new_weight: usize) -> bool {
        self.idempotency.len() < self.max_entries
            && self.total_cached_bytes.saturating_add(new_weight) <= self.max_bytes
    }

    fn insert_idempotency(
        &mut self,
        key: String,
        binding: String,
        outcome: CachedOutcome,
        now: Instant,
    ) -> Result<(), ()> {
        let payload_bytes = match &outcome {
            CachedOutcome::Final(resp) => resp.body.len(),
            CachedOutcome::Unknown { reason, .. } => reason.len(),
        };
        let weight = key.len() + binding.len() + payload_bytes + 64;
        if !self.has_capacity_for_idempotency(weight) {
            return Err(());
        }
        self.total_cached_bytes = self.total_cached_bytes.saturating_add(weight);
        self.idempotency.insert(
            key,
            IdempotencyEntry {
                request_binding_digest: binding,
                outcome,
                byte_weight: weight,
                expires_at: now + self.window,
            },
        );
        Ok(())
    }
}

/// Compute a canonical request-binding SHA-256 digest linking method, path,
/// query, RPC operation, full body bytes, repository context, caller/session/
/// tenant/repo headers, and hashed bearer credential.
pub fn compute_request_binding_digest(
    request: &HttpRequest,
    repository_context: &str,
    auth_token: &str,
) -> String {
    let op = extract_rpc_op(&request.body).unwrap_or_default();
    let body_hash = format!("{:x}", Sha256::digest(&request.body));
    let token_hash = format!("{:x}", Sha256::digest(auth_token.as_bytes()));
    let agent = request
        .headers
        .get("x-decapod-agent-id")
        .or_else(|| request.headers.get("x-decapod-actor"))
        .map(String::as_str)
        .unwrap_or("");
    let session = request
        .headers
        .get("x-decapod-session-id")
        .map(String::as_str)
        .unwrap_or("");
    let tenant = request
        .headers
        .get("x-decapod-tenant")
        .map(String::as_str)
        .unwrap_or("");
    let repo_id = request
        .headers
        .get("x-decapod-repo-id")
        .map(String::as_str)
        .unwrap_or("");
    let policy_ctx = request
        .headers
        .get("x-decapod-policy-context")
        .map(String::as_str)
        .unwrap_or("");

    let mut hasher = Sha256::new();
    for part in [
        request.method.as_str(),
        "\0",
        request.path.as_str(),
        "\0",
        request.query.as_str(),
        "\0",
        op.as_str(),
        "\0",
        body_hash.as_str(),
        "\0",
        repository_context,
        "\0",
        agent,
        "\0",
        session,
        "\0",
        tenant,
        "\0",
        repo_id,
        "\0",
        policy_ctx,
        "\0",
        token_hash.as_str(),
    ] {
        hasher.update(part.as_bytes());
    }
    format!("sha256:{:x}", hasher.finalize())
}

pub fn extract_rpc_op(body: &[u8]) -> Option<String> {
    let value: serde_json::Value = serde_json::from_slice(body).ok()?;
    value
        .get("op")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToOwned::to_owned)
}

/// Backward-compatible entry point for `decapod rpc --http-server`.
pub fn serve<F>(
    bind: &str,
    auth_token: &str,
    allow_remote: bool,
    max_body_bytes: usize,
    mut handler: F,
) -> std::io::Result<()>
where
    F: FnMut(&[u8]) -> HttpResponse,
{
    let config = HttpTransportConfig::for_rpc(bind, auth_token, allow_remote, max_body_bytes, "");
    serve_with_config(config, move |req| handler(&req.body))
}

/// Serve with full security, bounded replay cache, and request-bound idempotency.
pub fn serve_with_config<F>(config: HttpTransportConfig, handler: F) -> std::io::Result<()>
where
    F: FnMut(&HttpRequest) -> HttpResponse,
{
    let (listener, address) =
        bind_checked_listener(&config.bind, &config.auth_token, config.allow_remote)?;
    eprintln!("decapod HTTP RPC listening on {address}");
    serve_on_listener(listener, config, handler)
}

/// Serve on an already-bound `TcpListener` (ideal for `127.0.0.1:0` tests and
/// `decapod observe serve` where the bound address is printed before serving).
pub fn serve_on_listener<F>(
    listener: TcpListener,
    config: HttpTransportConfig,
    mut handler: F,
) -> std::io::Result<()>
where
    F: FnMut(&HttpRequest) -> HttpResponse,
{
    let address = listener.local_addr()?;
    if !config.allow_remote && !address.ip().is_loopback() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "remote HTTP binding requires --allow-remote",
        ));
    }

    let mut store = ReplayStore::new(
        config.max_cache_entries,
        config.max_cache_bytes,
        config.replay_window,
    );
    let contract = config.replay_contract();
    let mut handled: usize = 0;

    for incoming in listener.incoming() {
        if let Some(limit) = config.max_requests
            && handled >= limit
        {
            break;
        }
        let mut stream = match incoming {
            Ok(stream) => stream,
            Err(_) => continue,
        };
        handled += 1;
        let _ = stream.set_read_timeout(Some(config.read_timeout));
        let _ = stream.set_write_timeout(Some(config.write_timeout));

        let response =
            handle_single_connection(&mut stream, &config, &mut store, &contract, &mut handler);
        let _ = write_response(&mut stream, response);

        if let Some(limit) = config.max_requests
            && handled >= limit
        {
            break;
        }
    }
    Ok(())
}

fn handle_single_connection<F>(
    stream: &mut TcpStream,
    config: &HttpTransportConfig,
    store: &mut ReplayStore,
    contract: &ReplayContract,
    handler: &mut F,
) -> HttpResponse
where
    F: FnMut(&HttpRequest) -> HttpResponse,
{
    let request = match read_request(stream, config.max_body_bytes) {
        Ok(request) => request,
        Err(response) => return response,
    };

    // 1. Origin and Host protection (loopback / DNS rebinding / CSRF defense)
    if let Err(response) = validate_origin_and_host(
        &request.headers,
        config.allow_remote,
        &config.allowed_origins,
    ) {
        return response;
    }

    // 2. Constant-time Bearer token authentication
    if !verify_bearer_token(
        request.headers.get("authorization").map(String::as_str),
        &config.auth_token,
    ) {
        return HttpResponse::json(
            401,
            error_code_json("unauthorized", "invalid or missing bearer token"),
        );
    }

    // 3. Token revocation check (#1719)
    if let Some(token_id) = request.headers.get("x-decapod-token-id")
        && config
            .revoked_token_ids
            .iter()
            .any(|revoked| revoked == token_id.trim())
    {
        return HttpResponse::json(
            401,
            error_code_json("token_revoked", "credential has been revoked"),
        );
    }

    // 4. Tenant and repository scope enforcement (#1719)
    if let Some(required_tenant) = config.required_tenant_id.as_deref() {
        let provided = request
            .headers
            .get("x-decapod-tenant")
            .map(|v| v.trim())
            .unwrap_or("");
        if provided != required_tenant {
            return HttpResponse::json(
                403,
                error_code_json(
                    "cross_tenant_denied",
                    "request tenant does not match authorized repository tenant scope",
                ),
            );
        }
    }
    if !config.allowed_repo_ids.is_empty() {
        let provided = request
            .headers
            .get("x-decapod-repo-id")
            .map(|v| v.trim())
            .unwrap_or("");
        if !config
            .allowed_repo_ids
            .iter()
            .any(|allowed| allowed == provided)
        {
            return HttpResponse::json(
                403,
                error_code_json(
                    "cross_repo_denied",
                    "request repository scope is not authorized for this credential",
                ),
            );
        }
    }

    // 5. Method & read-only enforcement (#1719, #1720)
    if config.read_only_only {
        match request.method.as_str() {
            "GET" => {
                if !request.path.starts_with("/observe/v1/") {
                    return HttpResponse::json(
                        404,
                        error_code_json(
                            "not_found",
                            "use GET /observe/v1/<projection> or POST /rpc/v1 with allowlisted observe.* op",
                        ),
                    );
                }
            }
            "POST" => {
                if request.path != "/rpc/v1" {
                    return HttpResponse::json(
                        404,
                        error_code_json(
                            "not_found",
                            "use POST /rpc/v1 or GET /observe/v1/<projection>",
                        ),
                    );
                }
                let Some(op) = extract_rpc_op(&request.body) else {
                    return HttpResponse::json(
                        400,
                        error_code_json(
                            "invalid_request",
                            "POST /rpc/v1 body must be JSON with an 'op' field",
                        ),
                    );
                };
                if !config.allowed_read_ops.iter().any(|allowed| allowed == &op) {
                    return HttpResponse::json(
                        403,
                        serde_json::to_vec(&serde_json::json!({
                            "success": false,
                            "error_code": "read_only_enforced",
                            "error": format!("operation '{op}' is not permitted on a read-only observability credential/endpoint"),
                            "read_only": true,
                            "allowed_ops": config.allowed_read_ops,
                        }))
                        .unwrap_or_else(|_| b"{\"success\":false}".to_vec()),
                    )
                    .with_header("X-Decapod-Read-Only", "true");
                }
            }
            _ => {
                return HttpResponse::json(
                    405,
                    error_code_json(
                        "method_not_allowed",
                        "read-only observability transport refuses mutating HTTP methods",
                    ),
                )
                .with_header("Allow", "GET, POST")
                .with_header("X-Decapod-Read-Only", "true");
            }
        }
    } else {
        if request.method != "POST" {
            return HttpResponse::json(
                405,
                error_code_json("method_not_allowed", "use POST /rpc/v1"),
            )
            .with_header("Allow", "POST");
        }
        if request.path != "/rpc/v1" {
            return HttpResponse::json(404, error_code_json("not_found", "use POST /rpc/v1"));
        }
    }

    // 6. Idempotency & Replay enforcement (#874) — mandatory on POST /rpc/v1
    let now = Instant::now();
    store.prune_expired(now);

    let idempotency_key = request
        .headers
        .get("idempotency-key")
        .map(|v| v.trim())
        .filter(|v| !v.is_empty())
        .map(ToOwned::to_owned);
    let request_id = request
        .headers
        .get("x-decapod-request-id")
        .map(|v| v.trim())
        .filter(|v| !v.is_empty())
        .map(ToOwned::to_owned);

    if request.method == "POST" && idempotency_key.is_none() && request_id.is_none() {
        return HttpResponse::json(
            400,
            error_code_json(
                "missing_idempotency_key",
                "POST requests require Idempotency-Key or X-Decapod-Request-Id",
            ),
        );
    }

    let binding_digest =
        compute_request_binding_digest(&request, &config.repository_context, &config.auth_token);

    if let Some(ref idem_key) = idempotency_key
        && let Some(existing) = store.idempotency.get(idem_key)
    {
        if existing.request_binding_digest != binding_digest {
            return HttpResponse::json(
                409,
                serde_json::to_vec(&serde_json::json!({
                    "success": false,
                    "error_code": "idempotency_conflict",
                    "error": "Idempotency-Key is already bound to a different operation, payload, or caller context",
                    "replay_contract": contract,
                }))
                .unwrap_or_else(|_| b"{\"success\":false}".to_vec()),
            );
        }
        return match &existing.outcome {
            CachedOutcome::Final(resp) => resp
                .clone()
                .with_header("X-Decapod-Idempotency-Replayed", "true")
                .with_header("X-Decapod-Replay-Scope", "process_local_volatile"),
            CachedOutcome::Unknown { status, reason } => HttpResponse::json(
                409,
                serde_json::to_vec(&serde_json::json!({
                    "success": false,
                    "error_code": "unknown_outcome_reconcile_required",
                    "error": format!("prior attempt for this Idempotency-Key terminated with uncertain outcome (status={status}): {reason}; reconcile canonical repository state before submitting a new key"),
                    "replay_contract": contract,
                }))
                .unwrap_or_else(|_| b"{\"success\":false}".to_vec()),
            ),
        };
    }

    if let Some(ref req_id) = request_id {
        if store.request_ids.contains_key(req_id) {
            return HttpResponse::json(
                409,
                serde_json::to_vec(&serde_json::json!({
                    "success": false,
                    "error_code": "replayed_request",
                    "error": "X-Decapod-Request-Id was already processed within the active replay window",
                    "replay_contract": contract,
                }))
                .unwrap_or_else(|_| b"{\"success\":false}".to_vec()),
            );
        }
        if !store.has_capacity_for_request_id() {
            return HttpResponse::json(
                503,
                serde_json::to_vec(&serde_json::json!({
                    "success": false,
                    "error_code": "replay_capacity_exceeded",
                    "error": "active replay-protection table is at capacity; retry after replay window expires",
                    "replay_contract": contract,
                }))
                .unwrap_or_else(|_| b"{\"success\":false}".to_vec()),
            );
        }
    }

    // Pre-check idempotency entry cap BEFORE executing handler so overload fails
    // safely without executing uncacheable mutations!
    if let Some(ref idem_key) = idempotency_key {
        let min_weight = idem_key.len() + binding_digest.len() + 64;
        if !store.has_capacity_for_idempotency(min_weight) {
            return HttpResponse::json(
                503,
                serde_json::to_vec(&serde_json::json!({
                    "success": false,
                    "error_code": "idempotency_capacity_exceeded",
                    "error": "active idempotency cache reached entry or byte limit; failing closed to protect replay guarantees",
                    "replay_contract": contract,
                }))
                .unwrap_or_else(|_| b"{\"success\":false}".to_vec()),
            );
        }
    }

    // Reserve X-Decapod-Request-Id before execution so duplicate concurrent or
    // immediate retries cannot bypass replay protection.
    if let Some(ref req_id) = request_id {
        store.request_ids.insert(req_id.clone(), now + store.window);
    }

    // 7. Execute handler & enforce response size limit
    let mut response = handler(&request);
    if response.body.len() > config.max_response_bytes {
        response = HttpResponse::json_uncertain(
            502,
            error_code_json(
                "response_too_large",
                "handler response exceeded configured max_response_bytes",
            ),
        );
    }
    if config.read_only_only {
        response = response.with_header("X-Decapod-Read-Only", "true");
    }

    // 8. Record idempotency outcome (bounded by entries AND bytes)
    if let Some(idem_key) = idempotency_key {
        let outcome = if response.uncertain_outcome || response.status == 504 {
            CachedOutcome::Unknown {
                status: response.status,
                reason: "execution timed out, exceeded output limit, or was cancelled".to_string(),
            }
        } else {
            CachedOutcome::Final(response.clone())
        };
        if store
            .insert_idempotency(
                idem_key.clone(),
                binding_digest.clone(),
                outcome,
                Instant::now(),
            )
            .is_err()
        {
            // Response body alone exceeded remaining cache byte budget: record a
            // compact Unknown marker if possible, or fail closed with 503 if not
            // even the compact marker fits.
            let compact = CachedOutcome::Unknown {
                status: response.status,
                reason: "response exceeded idempotency cache byte budget; reconcile state before retrying"
                    .to_string(),
            };
            let _ = store.insert_idempotency(idem_key, binding_digest, compact, Instant::now());
            return HttpResponse::json(
                503,
                serde_json::to_vec(&serde_json::json!({
                    "success": false,
                    "error_code": "idempotency_cache_byte_limit_exceeded",
                    "error": "response exceeded bounded idempotency cache byte budget",
                    "replay_contract": contract,
                }))
                .unwrap_or_else(|_| b"{\"success\":false}".to_vec()),
            );
        }
    }

    response
}

fn read_request(
    stream: &mut TcpStream,
    max_body_bytes: usize,
) -> Result<HttpRequest, HttpResponse> {
    let mut bytes = Vec::new();
    let mut chunk = [0_u8; 4096];
    let header_end = loop {
        let count = stream.read(&mut chunk).map_err(read_io_error)?;
        if count == 0 {
            return Err(HttpResponse::json(
                400,
                error_code_json("incomplete_request", "incomplete HTTP request"),
            ));
        }
        bytes.extend_from_slice(&chunk[..count]);
        if bytes.len() > MAX_HEADER_BYTES + max_body_bytes {
            return Err(HttpResponse::json(
                413,
                error_code_json("payload_too_large", "request too large"),
            ));
        }
        if let Some(end) = bytes.windows(4).position(|window| window == b"\r\n\r\n") {
            break end + 4;
        }
        if bytes.len() > MAX_HEADER_BYTES {
            return Err(HttpResponse::json(
                431,
                error_code_json("headers_too_large", "HTTP headers too large"),
            ));
        }
    };
    let header = std::str::from_utf8(&bytes[..header_end]).map_err(|_| {
        HttpResponse::json(
            400,
            error_code_json("invalid_headers", "HTTP headers are not UTF-8"),
        )
    })?;
    let mut lines = header.split("\r\n");
    let request_line = lines.next().unwrap_or_default();
    let mut request_parts = request_line.split_whitespace();
    let method = request_parts.next().unwrap_or_default().to_string();
    let raw_target = request_parts.next().unwrap_or_default().to_string();
    let version = request_parts.next();
    if !matches!(version, Some("HTTP/1.1") | Some("HTTP/1.0")) {
        return Err(HttpResponse::json(
            400,
            error_code_json("invalid_request_line", "unsupported HTTP request line"),
        ));
    }
    let (path, query) = match raw_target.split_once('?') {
        Some((p, q)) => (p.to_string(), q.to_string()),
        None => (raw_target, String::new()),
    };
    let mut headers = HashMap::new();
    for line in lines.filter(|line| !line.is_empty()) {
        let Some((name, value)) = line.split_once(':') else {
            return Err(HttpResponse::json(
                400,
                error_code_json("malformed_header", "malformed HTTP header"),
            ));
        };
        headers.insert(name.trim().to_ascii_lowercase(), value.trim().to_string());
    }
    if headers
        .get("transfer-encoding")
        .is_some_and(|v| !v.trim().is_empty())
    {
        return Err(HttpResponse::json(
            400,
            error_code_json(
                "unsupported_transfer_encoding",
                "chunked transfer-encoding is not supported; supply Content-Length",
            ),
        ));
    }
    let length = if let Some(raw_len) = headers.get("content-length") {
        raw_len.parse::<usize>().map_err(|_| {
            HttpResponse::json(
                400,
                error_code_json("invalid_content_length", "invalid Content-Length header"),
            )
        })?
    } else if method == "GET" {
        0
    } else {
        return Err(HttpResponse::json(
            400,
            error_code_json("missing_content_length", "Content-Length is required"),
        ));
    };
    if length > max_body_bytes {
        return Err(HttpResponse::json(
            413,
            error_code_json("payload_too_large", "request body too large"),
        ));
    }
    while bytes.len() - header_end < length {
        let count = stream.read(&mut chunk).map_err(read_io_error)?;
        if count == 0 {
            return Err(HttpResponse::json(
                400,
                error_code_json("incomplete_body", "incomplete HTTP body"),
            ));
        }
        bytes.extend_from_slice(&chunk[..count]);
        if bytes.len() - header_end > max_body_bytes {
            return Err(HttpResponse::json(
                413,
                error_code_json("payload_too_large", "request body too large"),
            ));
        }
    }
    Ok(HttpRequest {
        method,
        path,
        query,
        headers,
        body: bytes[header_end..header_end + length].to_vec(),
    })
}

fn write_response(stream: &mut TcpStream, response: HttpResponse) -> std::io::Result<()> {
    let reason = match response.status {
        200 => "OK",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        408 => "Request Timeout",
        409 => "Conflict",
        413 => "Payload Too Large",
        431 => "Request Header Fields Too Large",
        502 => "Bad Gateway",
        503 => "Service Unavailable",
        504 => "Gateway Timeout",
        _ => "Internal Server Error",
    };
    let mut head = format!(
        "HTTP/1.1 {} {reason}\r\nContent-Type: {}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\nConnection: close\r\n",
        response.status,
        response.content_type,
        response.body.len()
    );
    for (name, value) in &response.headers {
        // Strip CR/LF defensively against header injection.
        let safe_name = name.replace(['\r', '\n'], "");
        let safe_val = value.replace(['\r', '\n'], "");
        head.push_str(&format!("{safe_name}: {safe_val}\r\n"));
    }
    head.push_str("\r\n");
    stream.write_all(head.as_bytes())?;
    stream.write_all(&response.body)
}

pub fn error_code_json(code: &str, message: &str) -> Vec<u8> {
    serde_json::to_vec(&serde_json::json!({
        "success": false,
        "error_code": code,
        "error": message,
    }))
    .unwrap_or_else(|_| b"{\"success\":false}".to_vec())
}

fn read_io_error(error: std::io::Error) -> HttpResponse {
    if matches!(
        error.kind(),
        std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
    ) {
        HttpResponse::json(
            408,
            error_code_json("request_timeout", "HTTP read timed out"),
        )
    } else {
        HttpResponse::json(
            400,
            error_code_json("io_error", &format!("HTTP read failed: {error}")),
        )
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/core/http_transport_tests.rs"]
mod tests;
