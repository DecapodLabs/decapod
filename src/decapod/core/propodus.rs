//! Typed, optional client boundary for the Decapod Cloud todo service.
//!
//! Decapod Cloud is a remote HTTP contract, not a Decapod compile-time dependency.
//! The transport is injectable so default tests remain local and the storage
//! boundary can be exercised without hosted credentials.

use crate::cli::CloudRuntimeConfig;
use crate::core::auth;
use crate::core::cloud_backend::{
    CloudOnboardingEndpoints, CloudOnboardingExchangeResponse, CloudOnboardingStartResponse,
    CloudOnboardingState, CloudOnboardingStatusResponse, CloudSession, CloudSessionExchangeRequest,
    CloudSessionExchangeResponse, CloudSessionRefreshRequest,
};
use crate::core::error::{CloudAuthDiagnostic, CloudAuthStatus};
use crate::core::repo_identity::RepositoryIdentity;
use crate::core::storage::{Task as StorageTask, TodoStore};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::io::IsTerminal;
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant};
use time::OffsetDateTime;

pub const CLOUD_STORE_CONTRACT_ID: &str = "decapod.cloud.store";
pub const CLOUD_STORE_CONTRACT_VERSION: &str = "v1";
pub const PROPODUS_CONTRACT_ID: &str = "decapod.propodus.todo";
pub const PROPODUS_CONTRACT_VERSION: &str = "v1";
pub const CLOUD_HEADER_CONTRACT: &str = "Decapod-Cloud-Contract";
pub const CLOUD_HEADER_CONTRACT_VERSION: &str = "Decapod-Cloud-Contract-Version";
pub const CLOUD_HEADER_DEPRECATION: &str = "Decapod-Cloud-Deprecation";
pub const CLOUD_HEADER_SUNSET: &str = "Decapod-Cloud-Sunset";
pub const CLOUD_HEADER_REQUEST_ID: &str = "Decapod-Cloud-Request-Id";
pub const CLOUD_HEADER_TRACE_ID: &str = "Decapod-Cloud-Trace-Id";
pub const CLOUD_HEADER_STATUS: &str = "Decapod-Cloud-Status";
const MAX_DIAGNOSTIC_HEADER_BYTES: usize = 256;
const MAX_CURSOR_PAGES: usize = 64;
pub const PROPODUS_HEALTH_ROUTE: &str = "/api/health";
pub const PROPODUS_TODOS_ROUTE: &str = "/api/todos";
pub const PROPODUS_STATUS_IN_PROGRESS: &str = "in_progress";
pub const PROPODUS_STATUS_COMPLETED: &str = "completed";
pub const CLOUD_INIT_COMMAND: &str = "decapod init --backend cloud";

fn cloud_init_action(detail: &str) -> String {
    format!("run `{CLOUD_INIT_COMMAND}` {detail}")
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PropodusConfig {
    pub api_url: String,
    pub repo_id: String,
}

impl PropodusConfig {
    pub fn for_repository(config: &CloudRuntimeConfig, identity: &RepositoryIdentity) -> Self {
        Self {
            api_url: config.api_url.clone(),
            repo_id: identity.canonical_name.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PropodusTodo {
    pub id: String,
    pub repo_id: String,
    pub title: String,
    #[serde(default)]
    pub description: Option<String>,
    pub status: String,
    #[serde(default)]
    pub actor: Option<String>,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub updated_at: Option<String>,
    #[serde(default)]
    pub version: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TodoCreateRequest {
    pub repo_id: String,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actor: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TodoUpdateRequest {
    pub status: String,
    pub actor: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PropodusListResponse {
    pub ok: bool,
    #[serde(default)]
    pub todos: Vec<PropodusTodo>,
    #[serde(default)]
    pub error: Option<PropodusError>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PropodusMutationResponse {
    pub ok: bool,
    #[serde(default)]
    pub todo: Option<PropodusTodo>,
    #[serde(default)]
    pub error: Option<PropodusError>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PropodusError {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CloudDiagnosticHeaders {
    pub contract: Option<String>,
    pub contract_version: Option<String>,
    pub deprecation: Option<String>,
    pub sunset: Option<String>,
    pub request_id: Option<String>,
    pub trace_id: Option<String>,
    pub status: Option<String>,
}

fn sanitize_diagnostic_header_value(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed.chars().any(char::is_control) {
        return None;
    }
    let lower = trimmed.to_ascii_lowercase();
    if lower.contains("bearer ")
        || lower.contains("token=")
        || lower.contains("secret=")
        || lower.contains("password=")
    {
        return Some("[REDACTED]".to_string());
    }
    let bounded: String = trimmed.chars().take(MAX_DIAGNOSTIC_HEADER_BYTES).collect();
    Some(bounded)
}

/// Parse bounded `Decapod-Cloud-*` diagnostic and transition headers from an
/// HTTP header block or `(name, value)` pairs without retaining secrets.
pub fn parse_cloud_diagnostic_headers<'a, I>(headers: I) -> CloudDiagnosticHeaders
where
    I: IntoIterator<Item = (&'a str, &'a str)>,
{
    let mut parsed = CloudDiagnosticHeaders::default();
    for (name, value) in headers {
        let Some(clean) = sanitize_diagnostic_header_value(value) else {
            continue;
        };
        match name.trim().to_ascii_lowercase().as_str() {
            "decapod-cloud-contract" => parsed.contract = Some(clean),
            "decapod-cloud-contract-version" => parsed.contract_version = Some(clean),
            "decapod-cloud-deprecation" => parsed.deprecation = Some(clean),
            "deprecation" if parsed.deprecation.is_none() => parsed.deprecation = Some(clean),
            "decapod-cloud-sunset" => parsed.sunset = Some(clean),
            "sunset" if parsed.sunset.is_none() => parsed.sunset = Some(clean),
            "decapod-cloud-request-id" => parsed.request_id = Some(clean),
            "decapod-cloud-trace-id" => parsed.trace_id = Some(clean),
            "decapod-cloud-status" => parsed.status = Some(clean),
            _ => {}
        }
    }
    parsed
}

pub fn parse_cloud_diagnostic_header_block(raw_headers: &str) -> CloudDiagnosticHeaders {
    let mut current = CloudDiagnosticHeaders::default();
    for line in raw_headers.lines() {
        let trimmed = line.trim_end_matches('\r');
        if trimmed.starts_with("HTTP/") {
            // Retain only the final HTTP response's headers across redirect/100-continue hops.
            current = CloudDiagnosticHeaders::default();
            continue;
        }
        let Some((name, value)) = trimmed.split_once(':') else {
            continue;
        };
        let next = parse_cloud_diagnostic_headers([(name, value)]);
        if next.contract.is_some() {
            current.contract = next.contract;
        }
        if next.contract_version.is_some() {
            current.contract_version = next.contract_version;
        }
        if next.deprecation.is_some() {
            current.deprecation = next.deprecation;
        }
        if next.sunset.is_some() {
            current.sunset = next.sunset;
        }
        if next.request_id.is_some() {
            current.request_id = next.request_id;
        }
        if next.trace_id.is_some() {
            current.trace_id = next.trace_id;
        }
        if next.status.is_some() {
            current.status = next.status;
        }
    }
    current
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CloudStoreTodoPage {
    pub contract: String,
    pub contract_version: String,
    pub todos: Vec<PropodusTodo>,
    pub cursor: Option<String>,
    pub next_cursor: Option<String>,
    pub has_more: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct CloudStoreListEnvelope {
    #[serde(default, alias = "contract_id", alias = "schema_version")]
    contract: Option<String>,
    #[serde(default, alias = "contract_version")]
    version: Option<String>,
    #[serde(default)]
    ok: Option<bool>,
    #[serde(default)]
    todos: Option<Vec<PropodusTodo>>,
    #[serde(default)]
    items: Option<Vec<PropodusTodo>>,
    #[serde(default)]
    cursor: Option<String>,
    #[serde(default)]
    next_cursor: Option<String>,
    #[serde(default)]
    has_more: Option<bool>,
    #[serde(default)]
    error: Option<PropodusError>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct CloudStoreMutationEnvelope {
    #[serde(default, alias = "contract_id", alias = "schema_version")]
    contract: Option<String>,
    #[serde(default, alias = "contract_version")]
    version: Option<String>,
    #[serde(default)]
    ok: Option<bool>,
    #[serde(default)]
    todo: Option<PropodusTodo>,
    #[serde(default)]
    item: Option<PropodusTodo>,
    #[serde(default)]
    error: Option<PropodusError>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct PropodusErrorEnvelope {
    #[serde(default, alias = "contract_id", alias = "schema_version")]
    contract: Option<String>,
    #[serde(default, alias = "contract_version")]
    version: Option<String>,
    #[serde(default)]
    error: Option<PropodusError>,
    #[serde(default)]
    code: Option<String>,
    #[serde(default)]
    message: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PropodusHttpResponse {
    pub status: u16,
    pub body: Vec<u8>,
}

pub trait PropodusTransport: Send + Sync + Clone + 'static {
    fn request(
        &self,
        method: &str,
        url: &str,
        bearer: &str,
        body: Option<&[u8]>,
    ) -> Result<PropodusHttpResponse, PropodusClientError>;

    fn request_with_diagnostics(
        &self,
        method: &str,
        url: &str,
        bearer: &str,
        body: Option<&[u8]>,
    ) -> Result<(PropodusHttpResponse, CloudDiagnosticHeaders), PropodusClientError> {
        Ok((
            self.request(method, url, bearer, body)?,
            CloudDiagnosticHeaders::default(),
        ))
    }
}

#[derive(Debug, Clone)]
pub struct CurlTransport {
    pub connect_timeout_seconds: u64,
    pub max_time_seconds: u64,
    pub follow_redirects: bool,
    pub private_stdin: bool,
}

impl Default for CurlTransport {
    fn default() -> Self {
        Self {
            connect_timeout_seconds: 10,
            max_time_seconds: 30,
            follow_redirects: true,
            private_stdin: false,
        }
    }
}

impl CurlTransport {
    /// Supabase authentication uses the same no-redirect boundary as Dactyl
    /// data requests: refresh material must never follow an HTTP redirect.
    pub fn for_datastore(datastore: crate::core::backend::CloudDatastore) -> Self {
        Self {
            follow_redirects: datastore == crate::core::backend::CloudDatastore::Neon,
            private_stdin: datastore != crate::core::backend::CloudDatastore::Neon,
            ..Self::default()
        }
    }
}

fn quote_curl_config(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}

fn private_curl_config(bearer: &str, body: Option<&[u8]>) -> Result<String, PropodusClientError> {
    if bearer
        .chars()
        .any(|value| value.is_whitespace() || value.is_control())
    {
        return Err(PropodusClientError::Configuration(
            "invalid authentication bearer".to_string(),
        ));
    }
    let mut config = String::new();
    if !bearer.is_empty() {
        config.push_str(&format!(
            "header = \"Authorization: Bearer {}\"\n",
            quote_curl_config(bearer)
        ));
    }
    if let Some(body) = body {
        let body = std::str::from_utf8(body).map_err(|_| {
            PropodusClientError::Configuration("authentication body must be UTF-8 JSON".to_string())
        })?;
        // JSON also excludes curl's special @file data input syntax.
        serde_json::from_str::<serde_json::Value>(body).map_err(|_| {
            PropodusClientError::Configuration("authentication body must be JSON".to_string())
        })?;
        config.push_str(&format!("data-binary = \"{}\"\n", quote_curl_config(body)));
    }
    Ok(config)
}

fn safe_auth_error_body(body: &[u8], status: u16) -> Vec<u8> {
    let parsed = serde_json::from_slice::<serde_json::Value>(body).ok();
    let supplied_code = parsed
        .as_ref()
        .and_then(|value| value.get("error")?.get("code")?.as_str());
    let code = supplied_code
        .filter(|code| {
            matches!(
                *code,
                "session_revoked"
                    | "revoked"
                    | "token_expired"
                    | "session_expired"
                    | "identity_not_authorized"
                    | "unauthorized_identity"
                    | "repository_not_authorized"
                    | "authorization_denied"
                    | "invalid_token"
                    | "organization_seat_required"
            )
        })
        .unwrap_or("http_error");
    serde_json::json!({"error": {"code": code, "message": format!("authentication service returned HTTP status {status}")}}).to_string().into_bytes()
}

impl PropodusTransport for CurlTransport {
    fn request(
        &self,
        method: &str,
        url: &str,
        bearer: &str,
        body: Option<&[u8]>,
    ) -> Result<PropodusHttpResponse, PropodusClientError> {
        self.request_with_diagnostics(method, url, bearer, body)
            .map(|(response, _)| response)
    }

    fn request_with_diagnostics(
        &self,
        method: &str,
        url: &str,
        bearer: &str,
        body: Option<&[u8]>,
    ) -> Result<(PropodusHttpResponse, CloudDiagnosticHeaders), PropodusClientError> {
        let header_file = tempfile::NamedTempFile::new().map_err(|error| {
            PropodusClientError::Transport(format!("could not allocate header buffer: {error}"))
        })?;
        let mut command = Command::new("curl");
        if self.private_stdin {
            // Must be the first argument: ignore ambient curlrc directives,
            // including redirects that could forward authentication material.
            command.arg("--disable");
            command.env_remove(auth::CLOUD_ACCESS_TOKEN_ENV);
        }
        command.args([
            "--silent",
            "--show-error",
            "--request",
            method,
            "--header",
            "Accept: application/json",
            "--header",
            "Content-Type: application/json",
            "--connect-timeout",
            &self.connect_timeout_seconds.to_string(),
            "--max-time",
            &self.max_time_seconds.to_string(),
            "--dump-header",
        ]);
        command.arg(header_file.path());
        command.args(["--write-out", "\n%{http_code}", url]);
        if self.follow_redirects {
            command.arg("--location");
        }
        let output = if self.private_stdin {
            use std::io::Write;
            use std::process::Stdio;
            let config = private_curl_config(bearer, body)?;
            command
                .args(["--config", "-"])
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped());
            let mut child = command.spawn().map_err(|_| {
                PropodusClientError::Transport(
                    "could not start authentication transport".to_string(),
                )
            })?;
            let write_result = child
                .stdin
                .take()
                .ok_or_else(|| {
                    std::io::Error::other("authentication transport stdin is unavailable")
                })
                .and_then(|mut stdin| stdin.write_all(config.as_bytes()));
            if write_result.is_err() {
                let _ = child.kill();
                let _ = child.wait();
                return Err(PropodusClientError::Transport(
                    "could not provide authentication request".to_string(),
                ));
            }
            child.wait_with_output().map_err(|_| {
                PropodusClientError::Transport(
                    "authentication transport did not complete".to_string(),
                )
            })?
        } else {
            if !bearer.trim().is_empty() {
                command.args(["--header", &format!("Authorization: Bearer {bearer}")]);
            }
            if let Some(body) = body {
                command.args(["--data-binary", &String::from_utf8_lossy(body)]);
            }
            command
                .output()
                .map_err(|error| PropodusClientError::Transport(error.to_string()))?
        };
        if !output.status.success() {
            return Err(PropodusClientError::Transport(format!(
                "curl failed with exit status {}",
                output.status.code().unwrap_or(-1)
            )));
        }
        let raw_headers = std::fs::read_to_string(header_file.path()).unwrap_or_default();
        let headers = parse_cloud_diagnostic_header_block(&raw_headers);
        let stdout = String::from_utf8(output.stdout)
            .map_err(|_| PropodusClientError::Transport("response was not UTF-8".to_string()))?;
        let (body, status) = stdout.rsplit_once('\n').ok_or_else(|| {
            PropodusClientError::Transport(
                "curl response did not include an HTTP status".to_string(),
            )
        })?;
        let status = status.parse::<u16>().map_err(|_| {
            PropodusClientError::Transport("curl returned an invalid HTTP status".to_string())
        })?;
        Ok((
            PropodusHttpResponse {
                status,
                body: if self.private_stdin && !(200..300).contains(&status) {
                    safe_auth_error_body(body.as_bytes(), status)
                } else {
                    body.as_bytes().to_vec()
                },
            },
            headers,
        ))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PropodusClientError {
    Configuration(String),
    Authentication(CloudAuthDiagnostic),
    Transport(String),
    Decode(String),
    Validation(String),
    Service {
        status: u16,
        code: String,
        message: String,
    },
    NotFound(String),
    Conflict(String),
}

impl fmt::Display for PropodusClientError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Configuration(message) => {
                write!(f, "Decapod Cloud configuration error: {message}")
            }
            Self::Authentication(diagnostic) => write!(
                f,
                "Decapod Cloud authentication error ({:?}): {}",
                diagnostic.status, diagnostic.message
            ),
            Self::Transport(message) => write!(f, "Decapod Cloud transport error: {message}"),
            Self::Decode(message) => write!(f, "Decapod Cloud response decode error: {message}"),
            Self::Validation(message) => write!(f, "Decapod Cloud validation error: {message}"),
            Self::Service {
                status,
                code,
                message,
            } => {
                write!(
                    f,
                    "Decapod Cloud service error ({status}, {code}): {message}"
                )
            }
            Self::NotFound(message) => write!(f, "Decapod Cloud not found: {message}"),
            Self::Conflict(message) => write!(f, "Decapod Cloud conflict: {message}"),
        }
    }
}

impl std::error::Error for PropodusClientError {}

fn cloud_auth_error(
    status: CloudAuthStatus,
    message: impl Into<String>,
    next_action: impl Into<String>,
) -> PropodusClientError {
    PropodusClientError::Authentication(CloudAuthDiagnostic::new(status, message, next_action))
}

fn map_auth_request_error(error: PropodusClientError) -> PropodusClientError {
    match error {
        PropodusClientError::Transport(_) => cloud_auth_error(
            CloudAuthStatus::Offline,
            "the cloud service could not be reached",
            "restore network access, then rerun the original command",
        ),
        PropodusClientError::Service {
            status: 403, code, ..
        } => {
            let status = match code.as_str() {
                "repository_not_authorized" => CloudAuthStatus::RepositoryDenied,
                "identity_not_authorized" | "unauthorized_identity" => {
                    CloudAuthStatus::UnauthorizedIdentity
                }
                _ => CloudAuthStatus::Unauthorized,
            };
            cloud_auth_error(
                status,
                "the cloud service rejected this repository session",
                "verify GitHub authorization for this repository, then rerun the original command",
            )
        }
        PropodusClientError::Service { status, .. } if status >= 500 => cloud_auth_error(
            CloudAuthStatus::ProviderUnavailable,
            "the cloud service is temporarily unavailable",
            "restore provider availability, then rerun the original command",
        ),
        PropodusClientError::Authentication(diagnostic) => {
            PropodusClientError::Authentication(diagnostic)
        }
        other => other,
    }
}

pub fn cloud_auth_diagnostic(error: &PropodusClientError) -> Option<&CloudAuthDiagnostic> {
    match error {
        PropodusClientError::Authentication(diagnostic) => Some(diagnostic),
        _ => None,
    }
}

#[derive(Clone)]
pub struct PropodusClient<T = CurlTransport> {
    api_url: String,
    repo_id: String,
    credential: String,
    transport: T,
}

impl<T> fmt::Debug for PropodusClient<T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PropodusClient")
            .field("api_url", &"[REDACTED]")
            .field("repo_id", &self.repo_id)
            .field("credential", &"[REDACTED]")
            .finish_non_exhaustive()
    }
}

impl PropodusClient<CurlTransport> {
    pub fn from_cloud_config(
        config: &CloudRuntimeConfig,
        identity: &RepositoryIdentity,
    ) -> Result<Self, PropodusClientError> {
        if config.datastore != "neon" {
            return Err(PropodusClientError::Configuration(
                "legacy todo client requires the Neon route; use the Dactyl command boundary for Supabase".to_string(),
            ));
        }
        let credential = auth::load_cloud_credential(None).map_err(|_| {
            cloud_auth_error(
                CloudAuthStatus::Missing,
                "no cloud machine session is configured",
                cloud_init_action(
                    "to complete the browser handoff, then rerun the original command",
                ),
            )
        })?;
        let config = PropodusConfig::for_repository(config, identity);
        Self::with_transport(&config, &credential.token, CurlTransport::default())
    }

    pub fn from_dogfood_cloud_config(
        config: &CloudRuntimeConfig,
        identity: &RepositoryIdentity,
    ) -> Result<Self, PropodusClientError> {
        if config.datastore != "neon" {
            return Err(PropodusClientError::Configuration(
                "legacy todo client requires the Neon route; use the Dactyl command boundary for Supabase".to_string(),
            ));
        }
        let config = PropodusConfig::for_repository(config, identity);
        let credential = ensure_cloud_session(&config, identity, CurlTransport::default())?;
        Self::with_transport(&config, &credential.token, CurlTransport::default())
    }
}

#[derive(Debug, Clone, Serialize)]
struct OnboardingStartRequest {
    repo_id: String,
}

/// Resolve a machine session for the repository-bound cloud path. Explicit
/// environment credentials remain supported for protected proofs, while the
/// normal interactive path starts or resumes the one-time browser exchange.
pub fn ensure_cloud_session<T: PropodusTransport>(
    config: &PropodusConfig,
    identity: &RepositoryIdentity,
    transport: T,
) -> Result<auth::CloudCredential, PropodusClientError> {
    let store = auth::CloudSessionStore::legacy().map_err(|_| {
        PropodusClientError::Configuration("cloud session storage is unavailable".to_string())
    })?;
    ensure_cloud_session_with_store(config, identity, transport, &store)
}

pub fn ensure_cloud_session_for_datastore(
    config: &PropodusConfig,
    identity: &RepositoryIdentity,
    datastore: crate::core::backend::CloudDatastore,
) -> Result<auth::CloudCredential, PropodusClientError> {
    datastore
        .validate_available()
        .map_err(|error| PropodusClientError::Configuration(error.to_string()))?;
    #[cfg(feature = "supabase-cloud")]
    if datastore == crate::core::backend::CloudDatastore::Supabase {
        dactyl_db::DatastoreRoute::supabase(&config.api_url, None)
            .validate()
            .map_err(|error| PropodusClientError::Configuration(error.to_string()))?;
    }
    // This helper owns transport policy as well as credential scope. Callers
    // cannot accidentally enable redirects or credential-bearing arguments.
    let transport = CurlTransport::for_datastore(datastore);
    let store = match datastore {
        crate::core::backend::CloudDatastore::Managed => {
            return Err(PropodusClientError::Configuration(
                "Managed cloud authentication belongs to Dactyl and repository-bound custody"
                    .into(),
            ));
        }
        crate::core::backend::CloudDatastore::Neon => auth::CloudSessionStore::legacy(),
        crate::core::backend::CloudDatastore::Supabase => {
            auth::CloudSessionStore::for_service(&config.api_url)
        }
    }
    .map_err(|_| {
        PropodusClientError::Configuration("cloud session storage is unavailable".to_string())
    })?;
    ensure_cloud_session_with_store(config, identity, transport, &store)
}

fn ensure_cloud_session_with_store<T: PropodusTransport>(
    config: &PropodusConfig,
    identity: &RepositoryIdentity,
    transport: T,
    store: &auth::CloudSessionStore,
) -> Result<auth::CloudCredential, PropodusClientError> {
    if mock_cloud_auth_enabled() {
        return ensure_mock_cloud_session(identity, store);
    }

    let mut expired_machine_session = false;
    if let Ok(credential) = store.load_credential(None) {
        if credential.source != auth::CredentialSource::MachineFile
            || !auth::cloud_session_needs_refresh(&credential)
        {
            return Ok(credential);
        }
        expired_machine_session = true;
        if let (Some(session_id), Some(refresh_token)) = (
            credential.session_id.as_deref(),
            credential.refresh_token.as_deref(),
        ) {
            let session = refresh_cloud_session(config, session_id, refresh_token, &transport)?;
            store.store_machine_session(&session).map_err(|_| {
                cloud_auth_error(
                    CloudAuthStatus::AuthRequired,
                    "the refreshed cloud session could not be stored",
                    cloud_init_action("again after checking machine data-directory permissions"),
                )
            })?;
            return store
                .load_machine_session()
                .map_err(|_| {
                    cloud_auth_error(
                        CloudAuthStatus::AuthRequired,
                        "the refreshed cloud session could not be reloaded",
                        cloud_init_action("again"),
                    )
                })?
                .ok_or_else(|| {
                    cloud_auth_error(
                        CloudAuthStatus::AuthRequired,
                        "the refreshed cloud session was not persisted",
                        cloud_init_action("again"),
                    )
                });
        }
    }

    complete_cloud_onboarding(config, identity, transport, expired_machine_session, store)
}

/// CI and deterministic repository tests need the cloud control-plane path to
/// exercise onboarding/session custody without asking a human or contacting a
/// hosted provider. The validation gate is deliberately required alongside
/// this explicit mode so production commands cannot silently use mock auth.
fn mock_cloud_auth_enabled() -> bool {
    std::env::var("DECAPOD_CLOUD_AUTH_MODE")
        .ok()
        .is_some_and(|mode| mode.eq_ignore_ascii_case("mock"))
        && std::env::var("DECAPOD_VALIDATE_SKIP_GIT_GATES").as_deref() == Ok("1")
}

fn ensure_mock_cloud_session(
    identity: &RepositoryIdentity,
    store: &auth::CloudSessionStore,
) -> Result<auth::CloudCredential, PropodusClientError> {
    let session = CloudSession {
        access_token: "decapod-test-mock-access".to_string(),
        refresh_token: Some("decapod-test-mock-refresh".to_string()),
        session_id: Some(format!("decapod-test-mock-{}", identity.canonical_name)),
        expires_at: Some("2099-01-01T00:00:00Z".to_string()),
    };
    store.store_machine_session(&session).map_err(|_| {
        cloud_auth_error(
            CloudAuthStatus::AuthRequired,
            "the mock cloud session could not be stored",
            "check machine data-directory permissions, then rerun the validation command",
        )
    })?;
    store
        .load_machine_session()
        .map_err(|_| {
            cloud_auth_error(
                CloudAuthStatus::AuthRequired,
                "the mock cloud session could not be reloaded",
                "check machine data-directory permissions, then rerun the validation command",
            )
        })?
        .ok_or_else(|| {
            cloud_auth_error(
                CloudAuthStatus::AuthRequired,
                "the mock cloud session was not persisted",
                "check machine data-directory permissions, then rerun the validation command",
            )
        })
}

fn complete_cloud_onboarding<T: PropodusTransport>(
    config: &PropodusConfig,
    identity: &RepositoryIdentity,
    transport: T,
    expired_machine_session: bool,
    store: &auth::CloudSessionStore,
) -> Result<auth::CloudCredential, PropodusClientError> {
    let interactive = std::io::stdin().is_terminal()
        && std::env::var_os("GITHUB_ACTIONS").is_none()
        && std::env::var_os("DECAPOD_HEADLESS").is_none();
    complete_cloud_onboarding_with_mode(
        config,
        identity,
        transport,
        expired_machine_session,
        interactive,
        store,
    )
}

fn complete_cloud_onboarding_with_mode<T: PropodusTransport>(
    config: &PropodusConfig,
    identity: &RepositoryIdentity,
    transport: T,
    expired_machine_session: bool,
    interactive: bool,
    store: &auth::CloudSessionStore,
) -> Result<auth::CloudCredential, PropodusClientError> {
    let endpoints = CloudOnboardingEndpoints::new(&config.api_url)
        .map_err(|error| PropodusClientError::Configuration(error.to_string()))?;
    let pending = auth::load_pending_cloud_onboarding()
        .map_err(|_| {
            cloud_auth_error(
                CloudAuthStatus::AuthRequired,
                "cloud onboarding state could not be read",
                cloud_init_action("again"),
            )
        })?
        .filter(|pending| {
            pending.api_url == config.api_url && pending.repo_id == identity.canonical_name
        });

    let had_pending_flow = pending.is_some();
    let (flow_id, url, expires_at) = if let Some(pending) = pending {
        (pending.flow_id, pending.url, pending.expires_at)
    } else {
        let request = OnboardingStartRequest {
            repo_id: identity.canonical_name.clone(),
        };
        let body = serde_json::to_vec(&request)
            .map_err(|error| PropodusClientError::Decode(error.to_string()))?;
        let response = public_request(&transport, "POST", &endpoints.start(), Some(&body))
            .map_err(map_auth_request_error)?;
        let started: CloudOnboardingStartResponse = decode_json(&response.body)?;
        let (flow_id, handoff) = started
            .into_handoff()
            .map_err(|error| PropodusClientError::Validation(error.to_string()))?;
        let pending = auth::PendingCloudOnboarding {
            api_url: config.api_url.clone(),
            repo_id: identity.canonical_name.clone(),
            flow_id: flow_id.clone(),
            url: handoff.bootstrap_url.clone(),
            expires_at: handoff.expires_at.clone(),
        };
        auth::store_pending_cloud_onboarding(&pending).map_err(|_| {
            cloud_auth_error(
                CloudAuthStatus::AuthRequired,
                "the cloud onboarding handoff could not be stored",
                cloud_init_action("again after checking machine data-directory permissions"),
            )
        })?;
        (flow_id, handoff.bootstrap_url, handoff.expires_at)
    };

    eprintln!("Cloud onboarding URL: {url}");
    eprintln!(
        "This one-time URL expires at {expires_at}; no credential is stored in the repository."
    );
    if interactive {
        try_open_browser(&url);
    }

    if !interactive {
        return Err(cloud_auth_error(
            if expired_machine_session {
                CloudAuthStatus::Expired
            } else if had_pending_flow {
                CloudAuthStatus::OnboardingPending
            } else {
                CloudAuthStatus::NonInteractive
            },
            if had_pending_flow {
                "browser onboarding is still pending"
            } else {
                "browser onboarding is required before the cloud command can continue"
            },
            "complete the printed onboarding URL, then rerun the original command",
        ));
    }

    let deadline = Instant::now()
        + if interactive {
            Duration::from_secs(120)
        } else {
            Duration::from_secs(0)
        };
    let mut first = true;
    loop {
        let status_url = endpoints
            .status(&flow_id)
            .map_err(|error| PropodusClientError::Validation(error.to_string()))?;
        let response =
            public_request(&transport, "GET", &status_url, None).map_err(map_auth_request_error)?;
        let status: CloudOnboardingStatusResponse = decode_json(&response.body)?;
        match status.state {
            CloudOnboardingState::Authorized => break,
            CloudOnboardingState::Canceled
            | CloudOnboardingState::Expired
            | CloudOnboardingState::Failed => {
                let _ = auth::clear_pending_cloud_onboarding();
                return Err(match status.state {
                    CloudOnboardingState::Expired => cloud_auth_error(
                        CloudAuthStatus::Expired,
                        "the cloud onboarding handoff expired",
                        cloud_init_action("to start a new handoff"),
                    ),
                    _ => cloud_auth_error(
                        CloudAuthStatus::AuthRequired,
                        "the cloud onboarding handoff was not completed",
                        cloud_init_action("to start a new handoff"),
                    ),
                });
            }
            CloudOnboardingState::Pending | CloudOnboardingState::Uncertain => {
                if !interactive || Instant::now() >= deadline {
                    return Err(cloud_auth_error(
                        CloudAuthStatus::OnboardingPending,
                        "browser onboarding is still pending",
                        "complete the printed onboarding URL, then rerun the original command",
                    ));
                }
                if !first {
                    thread::sleep(Duration::from_secs(
                        status.poll_after_seconds.unwrap_or(2).clamp(1, 10),
                    ));
                }
                first = false;
            }
        }
    }

    let exchange_body = serde_json::to_vec(&serde_json::json!({ "flow": flow_id }))
        .map_err(|error| PropodusClientError::Decode(error.to_string()))?;
    let response = public_request(
        &transport,
        "POST",
        &endpoints.exchange(),
        Some(&exchange_body),
    )
    .map_err(map_auth_request_error)?;
    let exchange: CloudOnboardingExchangeResponse = decode_json(&response.body)?;
    let code = exchange.code.trim();
    if code.is_empty() {
        return Err(cloud_auth_error(
            CloudAuthStatus::AuthRequired,
            "cloud onboarding returned no exchange code",
            cloud_init_action("to start a new handoff"),
        ));
    }

    let request = CloudSessionExchangeRequest::new(code).map_err(|_| {
        cloud_auth_error(
            CloudAuthStatus::AuthRequired,
            "cloud onboarding returned an invalid exchange code",
            cloud_init_action("to start a new handoff"),
        )
    })?;
    let body = serde_json::to_vec(&request)
        .map_err(|error| PropodusClientError::Decode(error.to_string()))?;
    let response = public_request(
        &transport,
        "POST",
        &endpoints.session_exchange(),
        Some(&body),
    )
    .map_err(map_auth_request_error)?;
    let session = CloudSessionExchangeResponse::into_session(decode_json(&response.body)?)
        .map_err(|_| {
            cloud_auth_error(
                CloudAuthStatus::AuthRequired,
                "cloud onboarding returned no usable session",
                cloud_init_action("to start a new handoff"),
            )
        })?;
    session.validate().map_err(|_| {
        cloud_auth_error(
            CloudAuthStatus::AuthRequired,
            "cloud onboarding returned an invalid session",
            cloud_init_action("to start a new handoff"),
        )
    })?;
    store.store_machine_session(&session).map_err(|_| {
        cloud_auth_error(
            CloudAuthStatus::AuthRequired,
            "the cloud session could not be stored",
            cloud_init_action("again after checking machine data-directory permissions"),
        )
    })?;
    auth::clear_pending_cloud_onboarding().map_err(|_| {
        cloud_auth_error(
            CloudAuthStatus::AuthRequired,
            "cloud onboarding state could not be cleared",
            cloud_init_action("again"),
        )
    })?;
    store
        .load_machine_session()
        .map_err(|_| {
            cloud_auth_error(
                CloudAuthStatus::AuthRequired,
                "the cloud session could not be reloaded",
                cloud_init_action("again"),
            )
        })?
        .ok_or_else(|| {
            cloud_auth_error(
                CloudAuthStatus::AuthRequired,
                "the cloud session was not persisted",
                cloud_init_action("again"),
            )
        })
}

fn refresh_cloud_session<T: PropodusTransport>(
    config: &PropodusConfig,
    session_id: &str,
    refresh_token: &str,
    transport: &T,
) -> Result<CloudSession, PropodusClientError> {
    let endpoints = CloudOnboardingEndpoints::new(&config.api_url)
        .map_err(|error| PropodusClientError::Configuration(error.to_string()))?;
    let request = CloudSessionRefreshRequest::new(session_id, refresh_token).map_err(|_| {
        cloud_auth_error(
            CloudAuthStatus::RefreshFailed,
            "the machine session refresh credentials are invalid",
            cloud_init_action("to renew the machine session"),
        )
    })?;
    let body = serde_json::to_vec(&request)
        .map_err(|error| PropodusClientError::Decode(error.to_string()))?;
    let response = public_request(transport, "POST", &endpoints.refresh(), Some(&body))
        .map_err(map_auth_request_error)?;
    CloudSessionExchangeResponse::into_session(decode_json(&response.body)?).map_err(|_| {
        cloud_auth_error(
            CloudAuthStatus::RefreshFailed,
            "the machine session could not be refreshed",
            cloud_init_action("to renew the machine session"),
        )
    })
}

fn public_request<T: PropodusTransport>(
    transport: &T,
    method: &str,
    url: &str,
    body: Option<&[u8]>,
) -> Result<PropodusHttpResponse, PropodusClientError> {
    let response = transport.request(method, url, "", body)?;
    if (200..300).contains(&response.status) {
        Ok(response)
    } else {
        Err(map_http_error(response))
    }
}

fn try_open_browser(url: &str) {
    if std::env::var_os("DECAPOD_HEADLESS").is_some() {
        return;
    }
    let (program, args): (&str, &[&str]) = if cfg!(target_os = "macos") {
        ("open", &[url])
    } else if cfg!(target_os = "windows") {
        ("cmd", &["/C", "start", "", url])
    } else {
        ("xdg-open", &[url])
    };
    let _ = Command::new(program)
        .args(args)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
}

impl<T: PropodusTransport> PropodusClient<T> {
    pub fn with_transport(
        config: &PropodusConfig,
        credential: &str,
        transport: T,
    ) -> Result<Self, PropodusClientError> {
        let api_url = config.api_url.trim().trim_end_matches('/');
        if !(api_url.starts_with("https://") || api_url.starts_with("http://")) {
            return Err(PropodusClientError::Configuration(
                "api_url must use http:// or https://".to_string(),
            ));
        }
        if config.repo_id.trim().is_empty() {
            return Err(PropodusClientError::Configuration(
                "repository identity is required for Decapod Cloud todo operations".to_string(),
            ));
        }
        if credential.trim().is_empty() {
            return Err(cloud_auth_error(
                CloudAuthStatus::AuthRequired,
                "a cloud machine session is required",
                cloud_init_action(
                    "to complete the browser handoff, then rerun the original command",
                ),
            ));
        }
        Ok(Self {
            api_url: api_url.to_string(),
            repo_id: config.repo_id.trim().to_string(),
            credential: credential.trim().to_string(),
            transport,
        })
    }

    pub fn repo_id(&self) -> &str {
        &self.repo_id
    }

    /// Verify that the configured Decapod Cloud endpoint is reachable and returns
    /// a successful health response. The endpoint owns the response schema;
    /// the Decapod boundary only requires a successful HTTP status.
    pub fn health_check(&self) -> Result<(), PropodusClientError> {
        let url = format!("{}{}", self.api_url, PROPODUS_HEALTH_ROUTE);
        let (response, _) = self.send_with_diagnostics("GET", &url, None)?;
        if response.body.is_empty() {
            return Err(PropodusClientError::Decode(
                "Decapod Cloud health response was empty".to_string(),
            ));
        }
        Ok(())
    }

    pub fn list_todos_page(
        &self,
        cursor: Option<&str>,
    ) -> Result<CloudStoreTodoPage, PropodusClientError> {
        let mut url = format!(
            "{}{}?repo_id={}",
            self.api_url,
            PROPODUS_TODOS_ROUTE,
            percent_encode(&self.repo_id)
        );
        if let Some(cursor) = cursor.map(str::trim).filter(|c| !c.is_empty()) {
            url.push_str("&cursor=");
            url.push_str(&percent_encode(cursor));
        }
        let (response, headers) = self.send_with_diagnostics("GET", &url, None)?;
        let payload: CloudStoreListEnvelope = decode_json(&response.body)?;
        let (contract, contract_version) = validate_contract_envelope(
            payload.contract.as_deref().or(headers.contract.as_deref()),
            payload
                .version
                .as_deref()
                .or(headers.contract_version.as_deref()),
        )?;
        if payload.ok == Some(false) {
            return Err(payload
                .error
                .map(|error| PropodusClientError::Validation(error.message))
                .unwrap_or_else(|| {
                    PropodusClientError::Validation("list request failed".to_string())
                }));
        }
        let todos = match (payload.items, payload.todos) {
            (Some(items), _) => items,
            (None, Some(todos)) => todos,
            (None, None) => {
                if payload.ok.is_none() {
                    return Err(PropodusClientError::Decode(
                        "response did not match the expected JSON contract".to_string(),
                    ));
                }
                Vec::new()
            }
        };
        let next_cursor = payload
            .next_cursor
            .map(|c| c.trim().to_string())
            .filter(|c| !c.is_empty());
        let has_more = payload.has_more.unwrap_or(next_cursor.is_some());
        Ok(CloudStoreTodoPage {
            contract,
            contract_version,
            todos,
            cursor: payload.cursor,
            next_cursor,
            has_more,
        })
    }

    pub fn list_todos(&self) -> Result<Vec<PropodusTodo>, PropodusClientError> {
        let mut all = Vec::new();
        let mut seen_cursors = std::collections::BTreeSet::new();
        let mut cursor: Option<String> = None;
        for _ in 0..MAX_CURSOR_PAGES {
            let page = self.list_todos_page(cursor.as_deref())?;
            all.extend(page.todos);
            let Some(next) = page.next_cursor else {
                return Ok(all);
            };
            if !page.has_more {
                return Ok(all);
            }
            if !seen_cursors.insert(next.clone()) {
                return Err(PropodusClientError::Decode(
                    "Decapod Cloud pagination cursor repeated".to_string(),
                ));
            }
            cursor = Some(next);
        }
        Err(PropodusClientError::Validation(
            "Decapod Cloud pagination exceeded bounded page limit".to_string(),
        ))
    }

    pub fn create_todo(
        &self,
        title: &str,
        description: Option<&str>,
        actor: Option<&str>,
    ) -> Result<PropodusTodo, PropodusClientError> {
        if title.trim().is_empty() {
            return Err(PropodusClientError::Validation(
                "todo title must not be empty".to_string(),
            ));
        }
        let request = TodoCreateRequest {
            repo_id: self.repo_id.clone(),
            title: title.trim().to_string(),
            description: description.map(str::to_string),
            actor: actor.map(str::to_string),
        };
        let body = serde_json::to_vec(&request)
            .map_err(|error| PropodusClientError::Decode(error.to_string()))?;
        let url = format!("{}{}", self.api_url, PROPODUS_TODOS_ROUTE);
        let (response, headers) = self.send_with_diagnostics("POST", &url, Some(&body))?;
        let payload: CloudStoreMutationEnvelope = decode_json(&response.body)?;
        validate_contract_envelope(
            payload.contract.as_deref().or(headers.contract.as_deref()),
            payload
                .version
                .as_deref()
                .or(headers.contract_version.as_deref()),
        )?;
        if payload.ok == Some(false) {
            return Err(payload
                .error
                .map(|error| PropodusClientError::Validation(error.message))
                .unwrap_or_else(|| {
                    PropodusClientError::Validation("create request failed".to_string())
                }));
        }
        payload.item.or(payload.todo).ok_or_else(|| {
            PropodusClientError::Decode("successful create response omitted todo".to_string())
        })
    }

    pub fn claim_todo(&self, id: &str, actor: &str) -> Result<PropodusTodo, PropodusClientError> {
        self.update_todo(id, PROPODUS_STATUS_IN_PROGRESS, actor)
    }

    pub fn complete_todo(
        &self,
        id: &str,
        actor: &str,
    ) -> Result<PropodusTodo, PropodusClientError> {
        self.update_todo(id, PROPODUS_STATUS_COMPLETED, actor)
    }

    fn update_todo(
        &self,
        id: &str,
        status: &str,
        actor: &str,
    ) -> Result<PropodusTodo, PropodusClientError> {
        if id.trim().is_empty() || actor.trim().is_empty() {
            return Err(PropodusClientError::Validation(
                "todo id and actor are required".to_string(),
            ));
        }
        let request = TodoUpdateRequest {
            status: status.to_string(),
            actor: actor.trim().to_string(),
        };
        let body = serde_json::to_vec(&request)
            .map_err(|error| PropodusClientError::Decode(error.to_string()))?;
        let url = format!(
            "{}{}?id={}",
            self.api_url,
            PROPODUS_TODOS_ROUTE,
            percent_encode(id)
        );
        let (response, headers) = self.send_with_diagnostics("PATCH", &url, Some(&body))?;
        let payload: CloudStoreMutationEnvelope = decode_json(&response.body)?;
        validate_contract_envelope(
            payload.contract.as_deref().or(headers.contract.as_deref()),
            payload
                .version
                .as_deref()
                .or(headers.contract_version.as_deref()),
        )?;
        if payload.ok != Some(false) {
            payload.item.or(payload.todo).ok_or_else(|| {
                PropodusClientError::Decode("successful update response omitted todo".to_string())
            })
        } else {
            Err(payload
                .error
                .map(|error| PropodusClientError::Validation(error.message))
                .unwrap_or_else(|| {
                    PropodusClientError::Validation("update request failed".to_string())
                }))
        }
    }

    fn send_with_diagnostics(
        &self,
        method: &str,
        url: &str,
        body: Option<&[u8]>,
    ) -> Result<(PropodusHttpResponse, CloudDiagnosticHeaders), PropodusClientError> {
        let (response, headers) =
            self.transport
                .request_with_diagnostics(method, url, &self.credential, body)?;
        if (200..300).contains(&response.status) {
            return Ok((response, headers));
        }
        Err(map_http_error_with_headers(response, &headers))
    }
}

/// Adapter for the existing storage abstraction.  The local SQLite store
/// remains the default; this type is selected only by a future explicit cloud
/// mode composition and therefore cannot silently replace local authority.
pub struct PropodusTodoStore<T: PropodusTransport> {
    client: PropodusClient<T>,
}

impl<T: PropodusTransport> PropodusTodoStore<T> {
    pub fn new(client: PropodusClient<T>) -> Self {
        Self { client }
    }
}

#[async_trait]
impl<T: PropodusTransport> TodoStore for PropodusTodoStore<T> {
    async fn list_tasks(&self) -> anyhow::Result<Vec<StorageTask>> {
        self.client
            .list_todos()
            .map(|todos| todos.into_iter().map(to_storage_task).collect())
            .map_err(|error| anyhow::anyhow!(error.to_string()))
    }

    async fn add_task(
        &self,
        task: StorageTask,
        actor: String,
        _intent: String,
    ) -> anyhow::Result<StorageTask> {
        self.client
            .create_todo(&task.title, task.description.as_deref(), Some(&actor))
            .map(to_storage_task)
            .map_err(|error| anyhow::anyhow!(error.to_string()))
    }

    async fn claim_task(&self, id: &str, actor: String) -> anyhow::Result<StorageTask> {
        self.client
            .claim_todo(id, &actor)
            .map(to_storage_task)
            .map_err(|error| anyhow::anyhow!(error.to_string()))
    }

    async fn complete_task(
        &self,
        id: &str,
        actor: String,
        _resolution: String,
    ) -> anyhow::Result<StorageTask> {
        self.client
            .complete_todo(id, &actor)
            .map(to_storage_task)
            .map_err(|error| anyhow::anyhow!(error.to_string()))
    }
}

fn to_storage_task(todo: PropodusTodo) -> StorageTask {
    let now = OffsetDateTime::now_utc();
    StorageTask {
        id: todo.id.clone(),
        repo_id: todo.repo_id,
        hash: todo.id,
        title: todo.title,
        description: todo.description,
        status: todo.status,
        assignee: todo.actor,
        scope: "repo".to_string(),
        dir_path: String::new(),
        priority: "normal".to_string(),
        category: "cloud".to_string(),
        tags: Vec::new(),
        created_at: parse_timestamp(todo.created_at.as_deref()).unwrap_or(now),
        updated_at: parse_timestamp(todo.updated_at.as_deref()).unwrap_or(now),
        version: todo.version.unwrap_or(1),
    }
}

fn parse_timestamp(value: Option<&str>) -> Option<OffsetDateTime> {
    value.and_then(|value| crate::core::time::parse_rfc3339(value).ok())
}

fn validate_contract_envelope(
    contract: Option<&str>,
    version: Option<&str>,
) -> Result<(String, String), PropodusClientError> {
    let resolved_contract = match contract.map(str::trim).filter(|s| !s.is_empty()) {
        None => PROPODUS_CONTRACT_ID.to_string(),
        Some("decapod.cloud.store") | Some("decapod.cloud.store.v1") => {
            CLOUD_STORE_CONTRACT_ID.to_string()
        }
        Some("decapod.propodus.todo") | Some("decapod.propodus.todo.v1") => {
            PROPODUS_CONTRACT_ID.to_string()
        }
        Some(other) => {
            return Err(PropodusClientError::Decode(format!(
                "unsupported Decapod Cloud contract envelope: {other}"
            )));
        }
    };
    let resolved_version = match version.map(str::trim).filter(|s| !s.is_empty()) {
        None | Some("v1") => CLOUD_STORE_CONTRACT_VERSION.to_string(),
        Some(other) => {
            return Err(PropodusClientError::Decode(format!(
                "unsupported Decapod Cloud contract version: {other}"
            )));
        }
    };
    Ok((resolved_contract, resolved_version))
}

fn decode_json<T: for<'de> Deserialize<'de>>(body: &[u8]) -> Result<T, PropodusClientError> {
    // Serde type/enum errors can quote values from an untrusted response.
    // Authentication responses may echo session material even with HTTP 200.
    serde_json::from_slice(body).map_err(|_| {
        PropodusClientError::Decode("response did not match the expected JSON contract".to_string())
    })
}

fn map_http_error(response: PropodusHttpResponse) -> PropodusClientError {
    map_http_error_with_headers(response, &CloudDiagnosticHeaders::default())
}

fn map_http_error_with_headers(
    response: PropodusHttpResponse,
    headers: &CloudDiagnosticHeaders,
) -> PropodusClientError {
    let envelope = serde_json::from_slice::<PropodusErrorEnvelope>(&response.body).ok();
    let (code, mut message) = envelope
        .and_then(|envelope| {
            envelope
                .error
                .map(|error| (error.code, error.message))
                .or_else(|| Some((envelope.code?, envelope.message?)))
        })
        .unwrap_or_else(|| {
            (
                "http_error".to_string(),
                "Decapod Cloud request failed".to_string(),
            )
        });
    if let Some(request_id) = headers.request_id.as_deref()
        && !message.contains(request_id)
    {
        message = format!("{message} [request_id={request_id}]");
    }
    match response.status {
        401 => {
            let status = match code.as_str() {
                "session_revoked" | "revoked" => CloudAuthStatus::Revoked,
                "token_expired" | "session_expired" => CloudAuthStatus::Expired,
                "identity_not_authorized" | "unauthorized_identity" => {
                    CloudAuthStatus::UnauthorizedIdentity
                }
                _ => CloudAuthStatus::Unauthorized,
            };
            cloud_auth_error(
                status,
                "the cloud service rejected the machine session",
                cloud_init_action("to renew the machine session, then rerun the original command"),
            )
        }
        403 => PropodusClientError::Service {
            status: response.status,
            code,
            message,
        },
        404 => PropodusClientError::NotFound(message),
        409 => PropodusClientError::Conflict(message),
        500..=599 => cloud_auth_error(
            CloudAuthStatus::ProviderUnavailable,
            "the cloud service is temporarily unavailable",
            "restore provider availability, then rerun the original command",
        ),
        _ => PropodusClientError::Service {
            status: response.status,
            code,
            message,
        },
    }
}

fn percent_encode(value: &str) -> String {
    let mut encoded = String::new();
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            encoded.push(byte as char);
        } else {
            encoded.push('%');
            encoded.push_str(&format!("{byte:02X}"));
        }
    }
    encoded
}

#[cfg(test)]
#[path = "../../../tests/unit/core/propodus_tests.rs"]
mod tests;
