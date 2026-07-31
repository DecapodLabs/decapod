//! Typed, optional client boundary for the external Propodus todo service.
//!
//! Propodus is a remote HTTP contract, not a Decapod compile-time dependency.
//! The transport is injectable so default tests remain local and the storage
//! boundary can be exercised without Vercel, Neon, or hosted credentials.

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
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::io::IsTerminal;
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant};

pub const PROPODUS_CONTRACT_ID: &str = "decapod.propodus.todo";
pub const PROPODUS_CONTRACT_VERSION: &str = "v1";
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct PropodusErrorEnvelope {
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
}

#[derive(Debug, Clone)]
pub struct CurlTransport {
    pub connect_timeout_seconds: u64,
    pub max_time_seconds: u64,
}

impl Default for CurlTransport {
    fn default() -> Self {
        Self {
            connect_timeout_seconds: 10,
            max_time_seconds: 30,
        }
    }
}

impl PropodusTransport for CurlTransport {
    fn request(
        &self,
        method: &str,
        url: &str,
        bearer: &str,
        body: Option<&[u8]>,
    ) -> Result<PropodusHttpResponse, PropodusClientError> {
        let mut command = Command::new("curl");
        command.args([
            "--silent",
            "--show-error",
            "--location",
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
            "--write-out",
            "\n%{http_code}",
            url,
        ]);
        if !bearer.trim().is_empty() {
            command.args(["--header", &format!("Authorization: Bearer {bearer}")]);
        }
        if let Some(body) = body {
            command.args(["--data-binary", &String::from_utf8_lossy(body)]);
        }

        let output = command
            .output()
            .map_err(|error| PropodusClientError::Transport(error.to_string()))?;
        if !output.status.success() {
            return Err(PropodusClientError::Transport(format!(
                "curl failed with exit status {}",
                output.status.code().unwrap_or(-1)
            )));
        }
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
        Ok(PropodusHttpResponse {
            status,
            body: body.as_bytes().to_vec(),
        })
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
            Self::Configuration(message) => write!(f, "Propodus configuration error: {message}"),
            Self::Authentication(diagnostic) => write!(
                f,
                "Propodus authentication error ({:?}): {}",
                diagnostic.status, diagnostic.message
            ),
            Self::Transport(message) => write!(f, "Propodus transport error: {message}"),
            Self::Decode(message) => write!(f, "Propodus response decode error: {message}"),
            Self::Validation(message) => write!(f, "Propodus validation error: {message}"),
            Self::Service {
                status,
                code,
                message,
            } => {
                write!(f, "Propodus service error ({status}, {code}): {message}")
            }
            Self::NotFound(message) => write!(f, "Propodus not found: {message}"),
            Self::Conflict(message) => write!(f, "Propodus conflict: {message}"),
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

#[derive(Debug, Clone)]
pub struct PropodusClient<T = CurlTransport> {
    api_url: String,
    repo_id: String,
    credential: String,
    transport: T,
}

impl PropodusClient<CurlTransport> {
    pub fn from_cloud_config(
        config: &CloudRuntimeConfig,
        identity: &RepositoryIdentity,
    ) -> Result<Self, PropodusClientError> {
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
    if mock_cloud_auth_enabled() {
        return ensure_mock_cloud_session(identity);
    }

    let mut expired_machine_session = false;
    if let Ok(credential) = auth::load_cloud_credential(None) {
        if credential.source != auth::CredentialSource::MachineFile
            || !auth::cloud_session_is_expired(&credential)
        {
            return Ok(credential);
        }
        expired_machine_session = true;
        if let (Some(session_id), Some(refresh_token)) = (
            credential.session_id.as_deref(),
            credential.refresh_token.as_deref(),
        ) {
            let session = refresh_cloud_session(config, session_id, refresh_token, &transport)?;
            auth::store_machine_session(&session).map_err(|_| {
                cloud_auth_error(
                    CloudAuthStatus::AuthRequired,
                    "the refreshed cloud session could not be stored",
                    cloud_init_action("again after checking machine data-directory permissions"),
                )
            })?;
            return auth::load_machine_session()
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

    complete_cloud_onboarding(config, identity, transport, expired_machine_session)
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
) -> Result<auth::CloudCredential, PropodusClientError> {
    let session = CloudSession {
        access_token: "decapod-test-mock-access".to_string(),
        refresh_token: Some("decapod-test-mock-refresh".to_string()),
        session_id: Some(format!("decapod-test-mock-{}", identity.canonical_name)),
        expires_at: Some("2099-01-01T00:00:00Z".to_string()),
    };
    auth::store_machine_session(&session).map_err(|_| {
        cloud_auth_error(
            CloudAuthStatus::AuthRequired,
            "the mock cloud session could not be stored",
            "check machine data-directory permissions, then rerun the validation command",
        )
    })?;
    auth::load_machine_session()
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
    )
}

fn complete_cloud_onboarding_with_mode<T: PropodusTransport>(
    config: &PropodusConfig,
    identity: &RepositoryIdentity,
    transport: T,
    expired_machine_session: bool,
    interactive: bool,
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
    auth::store_machine_session(&session).map_err(|_| {
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
    auth::load_machine_session()
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
                "repository identity is required for Propodus todo operations".to_string(),
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

    /// Verify that the configured Propodus endpoint is reachable and returns
    /// a successful health response. The endpoint owns the response schema;
    /// the Decapod boundary only requires a successful HTTP status.
    pub fn health_check(&self) -> Result<(), PropodusClientError> {
        let url = format!("{}{}", self.api_url, PROPODUS_HEALTH_ROUTE);
        let response = self.send("GET", &url, None)?;
        if response.body.is_empty() {
            return Err(PropodusClientError::Decode(
                "Propodus health response was empty".to_string(),
            ));
        }
        Ok(())
    }

    pub fn list_todos(&self) -> Result<Vec<PropodusTodo>, PropodusClientError> {
        let url = format!(
            "{}{}?repo_id={}",
            self.api_url,
            PROPODUS_TODOS_ROUTE,
            percent_encode(&self.repo_id)
        );
        let response = self.send("GET", &url, None)?;
        let payload: PropodusListResponse = decode_json(&response.body)?;
        if !payload.ok {
            return Err(payload
                .error
                .map(|error| PropodusClientError::Validation(error.message))
                .unwrap_or_else(|| {
                    PropodusClientError::Validation("list request failed".to_string())
                }));
        }
        Ok(payload.todos)
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
        let response = self.send("POST", &url, Some(&body))?;
        let payload: PropodusMutationResponse = decode_json(&response.body)?;
        if !payload.ok {
            return Err(payload
                .error
                .map(|error| PropodusClientError::Validation(error.message))
                .unwrap_or_else(|| {
                    PropodusClientError::Validation("create request failed".to_string())
                }));
        }
        payload.todo.ok_or_else(|| {
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
        let response = self.send("PATCH", &url, Some(&body))?;
        let payload: PropodusMutationResponse = decode_json(&response.body)?;
        if payload.ok {
            payload.todo.ok_or_else(|| {
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

    fn send(
        &self,
        method: &str,
        url: &str,
        body: Option<&[u8]>,
    ) -> Result<PropodusHttpResponse, PropodusClientError> {
        let response = self
            .transport
            .request(method, url, &self.credential, body)?;
        if (200..300).contains(&response.status) {
            return Ok(response);
        }
        Err(map_http_error(response))
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
    let now = Utc::now();
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
        category: "propodus".to_string(),
        tags: Vec::new(),
        created_at: parse_timestamp(todo.created_at.as_deref()).unwrap_or(now),
        updated_at: parse_timestamp(todo.updated_at.as_deref()).unwrap_or(now),
        version: todo.version.unwrap_or(1),
    }
}

fn parse_timestamp(value: Option<&str>) -> Option<DateTime<Utc>> {
    value
        .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
        .map(|value| value.with_timezone(&Utc))
}

fn decode_json<T: for<'de> Deserialize<'de>>(body: &[u8]) -> Result<T, PropodusClientError> {
    serde_json::from_slice(body).map_err(|error| PropodusClientError::Decode(error.to_string()))
}

fn map_http_error(response: PropodusHttpResponse) -> PropodusClientError {
    let envelope = serde_json::from_slice::<PropodusErrorEnvelope>(&response.body).ok();
    let (code, message) = envelope
        .and_then(|envelope| {
            envelope
                .error
                .map(|error| (error.code, error.message))
                .or_else(|| Some((envelope.code?, envelope.message?)))
        })
        .unwrap_or_else(|| {
            (
                "http_error".to_string(),
                "Propodus request failed".to_string(),
            )
        });
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
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex, OnceLock};

    #[derive(Clone)]
    struct OnboardingStartTransport;

    impl PropodusTransport for OnboardingStartTransport {
        fn request(
            &self,
            method: &str,
            url: &str,
            _bearer: &str,
            _body: Option<&[u8]>,
        ) -> Result<PropodusHttpResponse, PropodusClientError> {
            assert_eq!(method, "POST");
            assert!(url.ends_with("/api/onboarding/start"));
            Ok(PropodusHttpResponse {
                status: 200,
                body: br#"{"flow_id":"flow-opaque","bootstrap_url":"https://propodus.example.test/handoff?flow=opaque","expires_at":"2099-01-01T00:00:00Z","poll_after_seconds":2}"#.to_vec(),
            })
        }
    }

    #[derive(Clone)]
    struct RefreshTransport;

    impl PropodusTransport for RefreshTransport {
        fn request(
            &self,
            method: &str,
            url: &str,
            _bearer: &str,
            _body: Option<&[u8]>,
        ) -> Result<PropodusHttpResponse, PropodusClientError> {
            assert_eq!(method, "POST");
            assert!(url.ends_with("/api/auth/session/refresh"));
            let response = CloudSessionExchangeResponse {
                credentials: Some(CloudSession {
                    access_token: "refreshed-access".to_string(),
                    refresh_token: Some("refreshed-refresh".to_string()),
                    session_id: Some("refreshed-session".to_string()),
                    expires_at: Some("2099-01-01T00:00:00Z".to_string()),
                }),
                session: None,
            };
            Ok(PropodusHttpResponse {
                status: 200,
                body: serde_json::to_vec(&response).unwrap(),
            })
        }
    }

    #[derive(Clone, Default)]
    struct NoRequestTransport {
        calls: Arc<Mutex<usize>>,
    }

    impl PropodusTransport for NoRequestTransport {
        fn request(
            &self,
            _method: &str,
            _url: &str,
            _bearer: &str,
            _body: Option<&[u8]>,
        ) -> Result<PropodusHttpResponse, PropodusClientError> {
            *self.calls.lock().unwrap() += 1;
            Err(PropodusClientError::Validation(
                "unexpected provider request".to_string(),
            ))
        }
    }

    #[derive(Clone, Default)]
    struct AuthorizedExchangeTransport {
        calls: Arc<Mutex<Vec<String>>>,
    }

    impl PropodusTransport for AuthorizedExchangeTransport {
        fn request(
            &self,
            method: &str,
            url: &str,
            _bearer: &str,
            _body: Option<&[u8]>,
        ) -> Result<PropodusHttpResponse, PropodusClientError> {
            self.calls.lock().unwrap().push(format!("{method} {url}"));
            if method == "POST" && url.ends_with("/api/onboarding/start") {
                return Ok(PropodusHttpResponse {
                    status: 200,
                    body: br#"{"flow_id":"flow-authorized","bootstrap_url":"https://propodus.example.test/handoff?flow=authorized","expires_at":"2099-01-01T00:00:00Z","poll_after_seconds":1}"#.to_vec(),
                });
            }
            if method == "GET" && url.contains("/api/onboarding/status") {
                return Ok(PropodusHttpResponse {
                    status: 200,
                    body: br#"{"flow_id":"flow-authorized","state":"authorized"}"#.to_vec(),
                });
            }
            if method == "POST" && url.ends_with("/api/onboarding/exchange") {
                return Ok(PropodusHttpResponse {
                    status: 200,
                    body: br#"{"transaction_id":"tx-authorized","repository_id":"DecapodLabs/decapod","code":"opaque-exchange-code"}"#.to_vec(),
                });
            }
            if method == "POST" && url.ends_with("/api/auth/session/exchange") {
                let response = CloudSessionExchangeResponse {
                    credentials: Some(CloudSession {
                        access_token: "access-after-exchange".to_string(),
                        refresh_token: Some("refresh-after-exchange".to_string()),
                        session_id: Some("session-after-exchange".to_string()),
                        expires_at: Some("2099-01-01T00:00:00Z".to_string()),
                    }),
                    session: None,
                };
                return Ok(PropodusHttpResponse {
                    status: 200,
                    body: serde_json::to_vec(&response).unwrap(),
                });
            }
            Err(PropodusClientError::Validation(
                "unexpected provider request".to_string(),
            ))
        }
    }

    fn auth_env_lock() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(())).lock().unwrap()
    }

    #[test]
    fn auth_diagnostic_schema_is_stable_and_secret_free() {
        let diagnostic = CloudAuthDiagnostic::new(
            CloudAuthStatus::Unauthorized,
            "the cloud service rejected the machine session",
            cloud_init_action("to renew the machine session"),
        );
        let value = serde_json::to_value(&diagnostic).expect("serialize diagnostic");
        assert_eq!(
            value,
            serde_json::json!({
                "schema_version": "decapod.cloud.auth.v1",
                "status": "unauthorized",
                "message": "the cloud service rejected the machine session",
                "next_action": "run `decapod init --backend cloud` to renew the machine session"
            })
        );
        let rendered = serde_json::to_string(&value).unwrap();
        for secret in ["access-token", "refresh-token", "Bearer", "flow-opaque"] {
            assert!(!rendered.contains(secret), "diagnostic leaked {secret}");
        }
    }

    #[test]
    fn auth_request_failures_have_actionable_distinct_states() {
        let offline = map_auth_request_error(PropodusClientError::Transport(
            "connection refused".to_string(),
        ));
        assert_eq!(
            cloud_auth_diagnostic(&offline).unwrap().status,
            CloudAuthStatus::Offline
        );

        let unauthorized = map_auth_request_error(PropodusClientError::Service {
            status: 403,
            code: "repository_not_authorized".to_string(),
            message: "provider message is intentionally discarded".to_string(),
        });
        assert_eq!(
            cloud_auth_diagnostic(&unauthorized).unwrap().status,
            CloudAuthStatus::RepositoryDenied
        );
        assert!(!unauthorized.to_string().contains("provider message"));

        let revoked = map_http_error(PropodusHttpResponse {
            status: 401,
            body: br#"{"error":{"code":"session_revoked","message":"secret provider detail"}}"#
                .to_vec(),
        });
        assert_eq!(
            cloud_auth_diagnostic(&revoked).unwrap().status,
            CloudAuthStatus::Revoked
        );
        assert!(!revoked.to_string().contains("secret provider detail"));

        let provider = map_http_error(PropodusHttpResponse {
            status: 503,
            body: br#"{"error":{"code":"provider_down","message":"secret provider detail"}}"#
                .to_vec(),
        });
        assert_eq!(
            cloud_auth_diagnostic(&provider).unwrap().status,
            CloudAuthStatus::ProviderUnavailable
        );
        assert!(!provider.to_string().contains("secret provider detail"));

        let identity = map_auth_request_error(PropodusClientError::Service {
            status: 403,
            code: "identity_not_authorized".to_string(),
            message: "secret provider detail".to_string(),
        });
        assert_eq!(
            cloud_auth_diagnostic(&identity).unwrap().status,
            CloudAuthStatus::UnauthorizedIdentity
        );
        assert!(!identity.to_string().contains("secret provider detail"));
    }

    #[test]
    fn valid_machine_session_is_reused_without_onboarding_or_provider_requests() {
        let _lock = auth_env_lock();
        let data_home = tempfile::tempdir().expect("data home");
        let old_data_home = std::env::var_os("XDG_DATA_HOME");
        let old_token = std::env::var_os(auth::CLOUD_ACCESS_TOKEN_ENV);
        unsafe {
            std::env::set_var("XDG_DATA_HOME", data_home.path());
            std::env::remove_var(auth::CLOUD_ACCESS_TOKEN_ENV);
        }

        auth::store_machine_session(&CloudSession {
            access_token: "valid-access".to_string(),
            refresh_token: Some("valid-refresh".to_string()),
            session_id: Some("valid-session".to_string()),
            expires_at: Some("2099-01-01T00:00:00Z".to_string()),
        })
        .expect("store valid machine session");
        let identity = RepositoryIdentity {
            canonical_name: "DecapodLabs/decapod".to_string(),
            owner: "DecapodLabs".to_string(),
            repository: "decapod".to_string(),
            remote_url: "git@github.com:DecapodLabs/decapod.git".to_string(),
        };
        let config = PropodusConfig {
            api_url: "https://propodus.example.test".to_string(),
            repo_id: identity.canonical_name.clone(),
        };
        let transport = NoRequestTransport::default();
        let first = ensure_cloud_session(&config, &identity, transport.clone())
            .expect("valid machine session should be reusable");
        let second = ensure_cloud_session(&config, &identity, transport.clone())
            .expect("restart should reuse the same machine session");
        assert_eq!(first.token, "valid-access");
        assert_eq!(second.token, "valid-access");
        assert_eq!(*transport.calls.lock().unwrap(), 0);

        unsafe {
            match old_data_home {
                Some(value) => std::env::set_var("XDG_DATA_HOME", value),
                None => std::env::remove_var("XDG_DATA_HOME"),
            }
            match old_token {
                Some(value) => std::env::set_var(auth::CLOUD_ACCESS_TOKEN_ENV, value),
                None => std::env::remove_var(auth::CLOUD_ACCESS_TOKEN_ENV),
            }
        }
    }

    #[test]
    fn matching_pending_onboarding_is_reused_without_duplicate_start() {
        let _lock = auth_env_lock();
        let data_home = tempfile::tempdir().expect("data home");
        let old_data_home = std::env::var_os("XDG_DATA_HOME");
        let old_token = std::env::var_os(auth::CLOUD_ACCESS_TOKEN_ENV);
        let old_headless = std::env::var_os("DECAPOD_HEADLESS");
        unsafe {
            std::env::set_var("XDG_DATA_HOME", data_home.path());
            std::env::remove_var(auth::CLOUD_ACCESS_TOKEN_ENV);
            std::env::set_var("DECAPOD_HEADLESS", "1");
        }

        let identity = RepositoryIdentity {
            canonical_name: "DecapodLabs/decapod".to_string(),
            owner: "DecapodLabs".to_string(),
            repository: "decapod".to_string(),
            remote_url: "git@github.com:DecapodLabs/decapod.git".to_string(),
        };
        let config = PropodusConfig {
            api_url: "https://propodus.example.test".to_string(),
            repo_id: identity.canonical_name.clone(),
        };
        auth::store_pending_cloud_onboarding(&auth::PendingCloudOnboarding {
            api_url: config.api_url.clone(),
            repo_id: identity.canonical_name.clone(),
            flow_id: "flow-pending".to_string(),
            url: "https://propodus.example.test/handoff?flow=pending".to_string(),
            expires_at: "2099-01-01T00:00:00Z".to_string(),
        })
        .expect("store pending onboarding");

        let transport = NoRequestTransport::default();
        let error = ensure_cloud_session(&config, &identity, transport.clone())
            .expect_err("headless pending onboarding should return a resumable state");
        assert_eq!(
            cloud_auth_diagnostic(&error).unwrap().status,
            CloudAuthStatus::OnboardingPending
        );
        assert_eq!(*transport.calls.lock().unwrap(), 0);
        assert!(auth::load_pending_cloud_onboarding().unwrap().is_some());

        unsafe {
            match old_data_home {
                Some(value) => std::env::set_var("XDG_DATA_HOME", value),
                None => std::env::remove_var("XDG_DATA_HOME"),
            }
            match old_token {
                Some(value) => std::env::set_var(auth::CLOUD_ACCESS_TOKEN_ENV, value),
                None => std::env::remove_var(auth::CLOUD_ACCESS_TOKEN_ENV),
            }
            match old_headless {
                Some(value) => std::env::set_var("DECAPOD_HEADLESS", value),
                None => std::env::remove_var("DECAPOD_HEADLESS"),
            }
        }
    }

    #[test]
    fn one_time_onboarding_and_session_exchange_persist_machine_session() {
        let _lock = auth_env_lock();
        let data_home = tempfile::tempdir().expect("data home");
        let old_data_home = std::env::var_os("XDG_DATA_HOME");
        let old_token = std::env::var_os(auth::CLOUD_ACCESS_TOKEN_ENV);
        unsafe {
            std::env::set_var("XDG_DATA_HOME", data_home.path());
            std::env::remove_var(auth::CLOUD_ACCESS_TOKEN_ENV);
        }

        let identity = RepositoryIdentity {
            canonical_name: "DecapodLabs/decapod".to_string(),
            owner: "DecapodLabs".to_string(),
            repository: "decapod".to_string(),
            remote_url: "git@github.com:DecapodLabs/decapod.git".to_string(),
        };
        let config = PropodusConfig {
            api_url: "https://propodus.example.test".to_string(),
            repo_id: identity.canonical_name.clone(),
        };
        let transport = AuthorizedExchangeTransport::default();
        let credential =
            complete_cloud_onboarding_with_mode(&config, &identity, transport.clone(), false, true)
                .expect("authorized onboarding should complete");
        assert_eq!(credential.token, "access-after-exchange");
        assert_eq!(
            auth::load_machine_session().unwrap().unwrap().token,
            "access-after-exchange"
        );
        assert!(auth::load_pending_cloud_onboarding().unwrap().is_none());
        let calls = transport.calls.lock().unwrap().clone();
        assert_eq!(calls.len(), 4);
        assert!(calls[0].ends_with("POST https://propodus.example.test/api/onboarding/start"));
        assert!(calls[1].contains("GET https://propodus.example.test/api/onboarding/status"));
        assert!(calls[2].ends_with("POST https://propodus.example.test/api/onboarding/exchange"));
        assert!(calls[3].ends_with("POST https://propodus.example.test/api/auth/session/exchange"));
        let files: Vec<_> = std::fs::read_dir(data_home.path().join("decapod"))
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        assert!(
            !files
                .iter()
                .any(|name| name.to_string_lossy().contains("tmp"))
        );

        unsafe {
            match old_data_home {
                Some(value) => std::env::set_var("XDG_DATA_HOME", value),
                None => std::env::remove_var("XDG_DATA_HOME"),
            }
            match old_token {
                Some(value) => std::env::set_var(auth::CLOUD_ACCESS_TOKEN_ENV, value),
                None => std::env::remove_var(auth::CLOUD_ACCESS_TOKEN_ENV),
            }
        }
    }

    #[test]
    fn noninteractive_first_use_persists_handoff_and_returns_pending() {
        let _lock = auth_env_lock();
        let data_home = tempfile::tempdir().expect("data home");
        let old_data_home = std::env::var_os("XDG_DATA_HOME");
        let old_token = std::env::var_os(auth::CLOUD_ACCESS_TOKEN_ENV);
        let old_headless = std::env::var_os("DECAPOD_HEADLESS");
        unsafe {
            std::env::set_var("XDG_DATA_HOME", data_home.path());
            std::env::remove_var(auth::CLOUD_ACCESS_TOKEN_ENV);
            std::env::set_var("DECAPOD_HEADLESS", "1");
        }

        let identity = RepositoryIdentity {
            canonical_name: "DecapodLabs/decapod".to_string(),
            owner: "DecapodLabs".to_string(),
            repository: "decapod".to_string(),
            remote_url: "git@github.com:DecapodLabs/decapod.git".to_string(),
        };
        let config = PropodusConfig {
            api_url: "https://propodus.example.test".to_string(),
            repo_id: identity.canonical_name.clone(),
        };
        let error = ensure_cloud_session(&config, &identity, OnboardingStartTransport)
            .expect_err("noninteractive onboarding must pause for the human handoff");
        assert_eq!(
            cloud_auth_diagnostic(&error).unwrap().status,
            CloudAuthStatus::NonInteractive
        );
        assert!(
            data_home.path().join("decapod/onboarding.json").is_file(),
            "handoff custody must be machine-local"
        );
        assert!(!error.to_string().contains("flow-opaque"));
        assert!(!error.to_string().contains("Bearer"));

        unsafe {
            match old_data_home {
                Some(value) => std::env::set_var("XDG_DATA_HOME", value),
                None => std::env::remove_var("XDG_DATA_HOME"),
            }
            match old_token {
                Some(value) => std::env::set_var(auth::CLOUD_ACCESS_TOKEN_ENV, value),
                None => std::env::remove_var(auth::CLOUD_ACCESS_TOKEN_ENV),
            }
            match old_headless {
                Some(value) => std::env::set_var("DECAPOD_HEADLESS", value),
                None => std::env::remove_var("DECAPOD_HEADLESS"),
            }
        }
    }

    #[test]
    fn expired_machine_session_refreshes_without_human_interaction() {
        let _lock = auth_env_lock();
        let data_home = tempfile::tempdir().expect("data home");
        let old_data_home = std::env::var_os("XDG_DATA_HOME");
        let old_token = std::env::var_os(auth::CLOUD_ACCESS_TOKEN_ENV);
        unsafe {
            std::env::set_var("XDG_DATA_HOME", data_home.path());
            std::env::remove_var(auth::CLOUD_ACCESS_TOKEN_ENV);
        }

        auth::store_machine_session(&CloudSession {
            access_token: "expired-access".to_string(),
            refresh_token: Some("refresh-token".to_string()),
            session_id: Some("session-id".to_string()),
            expires_at: Some("2000-01-01T00:00:00Z".to_string()),
        })
        .expect("store expired machine session");
        let identity = RepositoryIdentity {
            canonical_name: "DecapodLabs/decapod".to_string(),
            owner: "DecapodLabs".to_string(),
            repository: "decapod".to_string(),
            remote_url: "git@github.com:DecapodLabs/decapod.git".to_string(),
        };
        let config = PropodusConfig {
            api_url: "https://propodus.example.test".to_string(),
            repo_id: identity.canonical_name.clone(),
        };
        let credential = ensure_cloud_session(&config, &identity, RefreshTransport)
            .expect("expired machine session should refresh");
        assert_eq!(credential.token, "refreshed-access");
        assert_eq!(credential.source, auth::CredentialSource::MachineFile);
        assert_eq!(auth::load_pending_cloud_onboarding().unwrap(), None);
        let stored = std::fs::read_to_string(data_home.path().join("decapod/session_token.json"))
            .expect("rotated machine session");
        assert!(stored.contains("refreshed-access"));
        assert!(stored.contains("refreshed-refresh"));
        assert!(
            !std::fs::read_dir(data_home.path().join("decapod"))
                .unwrap()
                .any(|entry| entry.unwrap().file_name().to_string_lossy().contains("tmp"))
        );

        unsafe {
            match old_data_home {
                Some(value) => std::env::set_var("XDG_DATA_HOME", value),
                None => std::env::remove_var("XDG_DATA_HOME"),
            }
            match old_token {
                Some(value) => std::env::set_var(auth::CLOUD_ACCESS_TOKEN_ENV, value),
                None => std::env::remove_var(auth::CLOUD_ACCESS_TOKEN_ENV),
            }
        }
    }
}
