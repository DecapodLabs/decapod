//! Capability-scoped host-side credential & DPoP request-signer broker for
//! isolated agent and container workspaces (#1066, #1778).
//!
//! Enforces strict capability separation (`git_transport`, `github_api`,
//! `propodus_cloud`), never exports device private keys, refresh credentials,
//! or cross-repository signing authority into containers, and validates
//! repository scope, actor identity, device revocation, and target origin on
//! every signing request.

use crate::core::cloud_dpop::{DeviceKeyMaterial, DeviceKeyStore, dpop_access_token_hash};
use crate::core::error::DecapodError;
use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceCredentialsConfig {
    /// Declared Git transport capability mode (e.g. `"ssh-agent"`, `"https-token"`, `"none"`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub git_transport: Option<String>,
    /// Declared GitHub API capability mode (e.g. `"gh-token"`, `"app-token"`, `"none"`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub github_api: Option<String>,
    /// Declared Propodus Cloud capability mode (e.g. `"signer-broker"`, `"propodus-token"`, `"none"`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub propodus_cloud: Option<String>,
}

impl WorkspaceCredentialsConfig {
    pub fn is_default(&self) -> bool {
        self.git_transport.is_none() && self.github_api.is_none() && self.propodus_cloud.is_none()
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CredentialCapabilityStatus {
    Available,
    Unconfigured,
    Expired,
    WrongRepository,
    Unauthorized,
    Absent,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CredentialCapabilityReportEntry {
    pub capability: String,
    pub status: CredentialCapabilityStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkspaceCredentialsReport {
    pub git_transport: CredentialCapabilityReportEntry,
    pub github_api: CredentialCapabilityReportEntry,
    pub propodus_cloud: CredentialCapabilityReportEntry,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SignerCapabilityKind {
    GitTransport,
    GithubApi,
    PropodusCloud,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContainerSignRequest {
    pub capability: SignerCapabilityKind,
    pub repository: String,
    pub agent_id: String,
    pub method: String,
    pub url: String,
    #[serde(default)]
    pub nonce: Option<String>,
    #[serde(default)]
    pub aud: Option<String>,
    #[serde(default)]
    pub access_token_sha256: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContainerSignResponse {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dpop_proof: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub jkt: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ath: Option<String>,
}

#[derive(Clone)]
pub struct ContainerSignerPolicy {
    pub allowed_repository: String,
    pub allowed_origin: String,
    pub allowed_agent_id: String,
    pub credentials: WorkspaceCredentialsConfig,
    pub session_expires_at: i64,
    pub revoked: Arc<AtomicBool>,
    pub device_key_store: Option<DeviceKeyStore>,
    pub access_token_digest: Option<String>,
}

impl std::fmt::Debug for ContainerSignerPolicy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ContainerSignerPolicy")
            .field("allowed_repository", &self.allowed_repository)
            .field("allowed_origin", &self.allowed_origin)
            .field("allowed_agent_id", &self.allowed_agent_id)
            .field("credentials", &self.credentials)
            .field("session_expires_at", &self.session_expires_at)
            .field("revoked", &self.revoked.load(Ordering::SeqCst))
            .field("has_device_key_store", &self.device_key_store.is_some())
            .field(
                "has_access_token_digest",
                &self.access_token_digest.is_some(),
            )
            .finish()
    }
}

impl ContainerSignerPolicy {
    pub fn evaluate_report(&self, now_secs: i64) -> WorkspaceCredentialsReport {
        let git_status = match self.credentials.git_transport.as_deref() {
            None | Some("none") | Some("") => CredentialCapabilityStatus::Unconfigured,
            Some("ssh-agent") => {
                if std::env::var_os("SSH_AUTH_SOCK")
                    .is_some_and(|v| !v.is_empty() && Path::new(&v).exists())
                {
                    CredentialCapabilityStatus::Available
                } else {
                    CredentialCapabilityStatus::Absent
                }
            }
            Some("https-token") | Some("gh-token") => CredentialCapabilityStatus::Available,
            Some(_) => CredentialCapabilityStatus::Unauthorized,
        };

        let gh_status = match self.credentials.github_api.as_deref() {
            None | Some("none") | Some("") => CredentialCapabilityStatus::Unconfigured,
            Some("gh-token") | Some("app-token") => CredentialCapabilityStatus::Available,
            Some(_) => CredentialCapabilityStatus::Unauthorized,
        };

        let propodus_status = match self.credentials.propodus_cloud.as_deref() {
            None | Some("none") | Some("") => CredentialCapabilityStatus::Unconfigured,
            Some("signer-broker") | Some("propodus-token") => {
                if self.revoked.load(Ordering::SeqCst) {
                    CredentialCapabilityStatus::Unauthorized
                } else if self.session_expires_at > 0 && now_secs >= self.session_expires_at {
                    CredentialCapabilityStatus::Expired
                } else if self
                    .device_key_store
                    .as_ref()
                    .and_then(|s| s.load().ok().flatten())
                    .is_some()
                {
                    CredentialCapabilityStatus::Available
                } else {
                    CredentialCapabilityStatus::Absent
                }
            }
            Some(_) => CredentialCapabilityStatus::Unauthorized,
        };

        WorkspaceCredentialsReport {
            git_transport: CredentialCapabilityReportEntry {
                capability: self
                    .credentials
                    .git_transport
                    .clone()
                    .unwrap_or_else(|| "none".to_string()),
                status: git_status,
            },
            github_api: CredentialCapabilityReportEntry {
                capability: self
                    .credentials
                    .github_api
                    .clone()
                    .unwrap_or_else(|| "none".to_string()),
                status: gh_status,
            },
            propodus_cloud: CredentialCapabilityReportEntry {
                capability: self
                    .credentials
                    .propodus_cloud
                    .clone()
                    .unwrap_or_else(|| "none".to_string()),
                status: propodus_status,
            },
        }
    }

    pub fn sign_request(
        &self,
        req: &ContainerSignRequest,
        now_secs: i64,
        key_override: Option<&DeviceKeyMaterial>,
    ) -> ContainerSignResponse {
        let deny = |code: &str, message: &str| ContainerSignResponse {
            ok: false,
            code: Some(code.to_string()),
            message: Some(message.to_string()),
            dpop_proof: None,
            jkt: None,
            ath: None,
        };

        if self.revoked.load(Ordering::SeqCst) {
            return deny(
                "CONTAINER_SIGNER_DEVICE_REVOKED",
                "host device session has been revoked; signing refused",
            );
        }
        if req.repository.trim() != self.allowed_repository {
            return deny(
                "CONTAINER_SIGNER_WRONG_REPOSITORY",
                "container signer is bound strictly to its configured repository",
            );
        }
        if req.agent_id.trim() != self.allowed_agent_id {
            return deny(
                "CONTAINER_SIGNER_AGENT_MISMATCH",
                "container signer refused signature for rotated or mismatched agent identity",
            );
        }
        if self.session_expires_at > 0 && now_secs >= self.session_expires_at {
            return deny(
                "CONTAINER_SIGNER_SESSION_EXPIRED",
                "bounded session has expired; host must re-authorize",
            );
        }

        // Strict independent capability separation: git/github grants never
        // grant Propodus Cloud signing and vice versa.
        match req.capability {
            SignerCapabilityKind::PropodusCloud => {
                let configured = self
                    .credentials
                    .propodus_cloud
                    .as_deref()
                    .is_some_and(|v| v == "signer-broker" || v == "propodus-token");
                if !configured {
                    return deny(
                        "CONTAINER_SIGNER_CAPABILITY_DENIED",
                        "propodus_cloud signer capability is not enabled for this workspace",
                    );
                }
            }
            SignerCapabilityKind::GitTransport => {
                return deny(
                    "CONTAINER_SIGNER_UNSUPPORTED_FOR_DPOP",
                    "git_transport does not mint Propodus Cloud DPoP proofs",
                );
            }
            SignerCapabilityKind::GithubApi => {
                return deny(
                    "CONTAINER_SIGNER_UNSUPPORTED_FOR_DPOP",
                    "github_api does not mint Propodus Cloud DPoP proofs",
                );
            }
        }

        // Enforce exact origin & canonical path allowlist: containers cannot
        // enroll new devices, revoke host keys, or smuggle foreign hosts/ports/paths.
        let allowed_origin = self.allowed_origin.trim_end_matches('/');
        let Some(path_part) = extract_origin_relative_path(&req.url, allowed_origin) else {
            return deny(
                "CONTAINER_SIGNER_ORIGIN_DENIED",
                "request URL does not match the bound Cloud origin",
            );
        };
        let path_only = path_part.split('?').next().unwrap_or(path_part);
        if !is_container_allowed_cloud_operation(req.method.as_str(), path_only) {
            return deny(
                "CONTAINER_SIGNER_OPERATION_DENIED",
                "container workspace is not allowed to enroll, revoke, or escalate device sessions",
            );
        }

        let loaded_key;
        let key = match key_override {
            Some(k) => k,
            None => {
                let Some(store) = &self.device_key_store else {
                    return deny(
                        "CONTAINER_SIGNER_KEY_ABSENT",
                        "host device signing key store is not configured",
                    );
                };
                match store.load() {
                    Ok(Some(k)) => {
                        loaded_key = k;
                        &loaded_key
                    }
                    Ok(None) => {
                        return deny(
                            "CONTAINER_SIGNER_KEY_ABSENT",
                            "host device signing key does not exist or was revoked",
                        );
                    }
                    Err(e) => {
                        return deny("CONTAINER_SIGNER_KEY_UNAVAILABLE", &e.to_string());
                    }
                }
            }
        };

        let ath_value = req.access_token_sha256.clone().or_else(|| {
            self.access_token_digest
                .as_deref()
                .and_then(|tok| dpop_access_token_hash(tok).ok())
        });
        match key.sign_dpop_proof_with_ath(
            &req.method,
            &req.url,
            ath_value.as_deref(),
            req.nonce.as_deref(),
            req.aud.as_deref(),
            Some(now_secs),
        ) {
            Ok(proof) => ContainerSignResponse {
                ok: true,
                code: None,
                message: None,
                dpop_proof: Some(proof),
                jkt: Some(key.jkt.clone()),
                ath: ath_value,
            },
            Err(e) => deny("CONTAINER_SIGNER_SIGN_FAILED", &e.to_string()),
        }
    }
}

fn extract_origin_relative_path<'a>(url: &'a str, allowed_origin: &str) -> Option<&'a str> {
    if allowed_origin.is_empty()
        || url.chars().any(|c| c.is_control() || c.is_whitespace())
        || url.contains('#')
    {
        return None;
    }
    let scheme_sep = allowed_origin.find("://")?;
    let origin_scheme = &allowed_origin[..scheme_sep];
    let origin_authority = &allowed_origin[scheme_sep + 3..];
    if origin_authority.is_empty()
        || origin_authority.contains(['/', '?', '#', '@'])
        || url.len() <= allowed_origin.len()
    {
        return None;
    }
    let url_scheme_sep = url.find("://")?;
    let url_scheme = &url[..url_scheme_sep];
    if !url_scheme.eq_ignore_ascii_case(origin_scheme) {
        return None;
    }
    let after_scheme = &url[url_scheme_sep + 3..];
    let authority_end = after_scheme.find(['/', '?']).unwrap_or(after_scheme.len());
    let url_authority = &after_scheme[..authority_end];
    if url_authority.contains('@') || !url_authority.eq_ignore_ascii_case(origin_authority) {
        return None;
    }
    let path_and_query = &after_scheme[authority_end..];
    if !path_and_query.starts_with('/') {
        return None;
    }
    Some(path_and_query)
}

fn is_container_allowed_cloud_operation(method: &str, path: &str) -> bool {
    let m = method.trim().to_ascii_uppercase();
    if m != "POST" {
        return false;
    }
    // Containers may sign normal scoped query/batch data & proof transitions,
    // never device enrollment, revocation, session minting, org/policy admin,
    // billing endpoints, or traversal/prefixed paths.
    matches!(
        path,
        "/functions/v1/decapod-onboarding/cloud-query"
            | "/functions/v1/decapod-onboarding/cloud-batch"
            | "/functions/v1/decapod-onboarding/cloud-control"
            | "/rest/v1/rpc/dactyl_query_v1"
            | "/rest/v1/rpc/dactyl_batch_v1"
            | "/rest/v1/rpc/propodus_proof_acceptance_v1"
            | "/rest/v1/rpc/propodus_proof_acceptance_get_v1"
    )
}

pub struct HostContainerSignerServer {
    socket_path: PathBuf,
    stop: Arc<AtomicBool>,
    handle: Option<std::thread::JoinHandle<()>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ContainerRelayOperation {
    DactylQuery,
    DactylBatch,
    ProofControlRead,
    ProofControlWrite,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ContainerRelayRequest {
    pub kind: String,
    pub invocation_id: String,
    pub workspace_id: String,
    pub project_id: String,
    pub task_id: String,
    pub custody_id: String,
    pub operation: ContainerRelayOperation,
    pub rpc_op: String,
    pub payload: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ContainerRelayResponse {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
}

pub trait ContainerCloudRelayHandler: Send + Sync {
    fn handle_relay(&self, request: &ContainerRelayRequest) -> ContainerRelayResponse;
}

impl HostContainerSignerServer {
    pub fn bind(socket_path: &Path, policy: ContainerSignerPolicy) -> Result<Self, DecapodError> {
        Self::bind_with_relay(socket_path, policy, None)
    }

    pub fn bind_with_relay(
        socket_path: &Path,
        policy: ContainerSignerPolicy,
        relay: Option<Arc<dyn ContainerCloudRelayHandler>>,
    ) -> Result<Self, DecapodError> {
        if let Some(parent) = socket_path.parent() {
            crate::core::fs_permissions::ensure_private_dir(parent)
                .map_err(DecapodError::IoError)?;
        }
        let _ = std::fs::remove_file(socket_path);
        let listener = UnixListener::bind(socket_path).map_err(DecapodError::IoError)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(socket_path, std::fs::Permissions::from_mode(0o600));
        }
        listener
            .set_nonblocking(true)
            .map_err(DecapodError::IoError)?;

        let stop = Arc::new(AtomicBool::new(false));
        let stop_flag = Arc::clone(&stop);
        let path_owned = socket_path.to_path_buf();

        let handle = std::thread::spawn(move || {
            while !stop_flag.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        let _ = handle_signer_connection(stream, &policy, relay.as_deref());
                    }
                    Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(std::time::Duration::from_millis(10));
                    }
                    Err(_) => break,
                }
            }
            let _ = std::fs::remove_file(&path_owned);
        });

        Ok(Self {
            socket_path: socket_path.to_path_buf(),
            stop,
            handle: Some(handle),
        })
    }

    pub fn socket_path(&self) -> &Path {
        &self.socket_path
    }
}

impl Drop for HostContainerSignerServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
        let _ = std::fs::remove_file(&self.socket_path);
    }
}

fn handle_signer_connection(
    mut stream: UnixStream,
    policy: &ContainerSignerPolicy,
    relay: Option<&dyn ContainerCloudRelayHandler>,
) -> std::io::Result<()> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut line = String::new();
    reader.read_line(&mut line)?;
    let trimmed = line.trim();
    let encoded = if let Ok(raw) = serde_json::from_str::<serde_json::Value>(trimmed)
        && raw.get("kind").and_then(serde_json::Value::as_str) == Some("cloud_relay")
    {
        let response = match serde_json::from_value::<ContainerRelayRequest>(raw) {
            Ok(req) => match relay {
                Some(handler) => handler.handle_relay(&req),
                None => ContainerRelayResponse {
                    ok: false,
                    code: Some("CLOUD_CONTAINER_RELAY_UNAVAILABLE".to_string()),
                    message: Some(
                        "typed cloud host relay is not configured on this signer socket"
                            .to_string(),
                    ),
                    result: None,
                },
            },
            Err(err) => ContainerRelayResponse {
                ok: false,
                code: Some("CONTAINER_RELAY_INVALID_REQUEST".to_string()),
                message: Some(format!("malformed relay request: {err}")),
                result: None,
            },
        };
        serde_json::to_string(&response).unwrap_or_default()
    } else {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        let response = match serde_json::from_str::<ContainerSignRequest>(trimmed) {
            Ok(req) => policy.sign_request(&req, now, None),
            Err(err) => ContainerSignResponse {
                ok: false,
                code: Some("CONTAINER_SIGNER_INVALID_REQUEST".to_string()),
                message: Some(format!("malformed signing request: {err}")),
                dpop_proof: None,
                jkt: None,
                ath: None,
            },
        };
        serde_json::to_string(&response).unwrap_or_default()
    };
    stream.write_all(encoded.as_bytes())?;
    stream.write_all(b"\n")?;
    stream.flush()
}

fn send_socket_json_line<Req: Serialize, Resp: for<'de> Deserialize<'de>>(
    socket_path: &Path,
    request: &Req,
    label: &str,
) -> Result<Resp, DecapodError> {
    let mut stream = UnixStream::connect(socket_path).map_err(DecapodError::IoError)?;
    let encoded = serde_json::to_string(request).map_err(|e| {
        DecapodError::ValidationError(format!("failed serializing {label} request: {e}"))
    })?;
    stream
        .write_all(encoded.as_bytes())
        .map_err(DecapodError::IoError)?;
    stream.write_all(b"\n").map_err(DecapodError::IoError)?;
    stream.flush().map_err(DecapodError::IoError)?;
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    reader.read_line(&mut line).map_err(DecapodError::IoError)?;
    serde_json::from_str(line.trim())
        .map_err(|e| DecapodError::ValidationError(format!("invalid {label} response: {e}")))
}

pub fn request_signature_via_socket(
    socket_path: &Path,
    request: &ContainerSignRequest,
) -> Result<ContainerSignResponse, DecapodError> {
    send_socket_json_line(socket_path, request, "container signer")
}

pub fn request_relay_via_socket(
    socket_path: &Path,
    request: &ContainerRelayRequest,
) -> Result<ContainerRelayResponse, DecapodError> {
    send_socket_json_line(socket_path, request, "container cloud relay")
}

/// Strip raw bearer, refresh, and provider secrets from inherited container
/// environment maps so containers only receive bounded capability handles.
pub fn filter_container_environment<'a>(
    vars: impl IntoIterator<Item = (&'a str, &'a str)>,
) -> Vec<(String, String)> {
    vars.into_iter()
        .filter(|(k, _)| !is_secret_env_key(k))
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

pub fn is_secret_env_key(key: &str) -> bool {
    let upper = key.trim().to_ascii_uppercase();
    upper.contains("SERVICE_ROLE")
        || upper.contains("PRIVATE_KEY")
        || upper.contains("REFRESH_TOKEN")
        || upper == "DECAPOD_CLOUD_TOKEN"
        || upper == "PROPODUS_TOKEN"
        || upper == "SUPABASE_ANON_KEY"
        || upper == "SUPABASE_SERVICE_ROLE_KEY"
        || upper == "GH_TOKEN"
        || upper == "GITHUB_TOKEN"
        || upper == "SSH_AUTH_SOCK"
}

#[cfg(test)]
#[path = "../../../tests/unit/core/container_signer_tests.rs"]
mod tests;
