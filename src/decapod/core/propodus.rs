//! Typed, optional client boundary for the external Propodus todo service.
//!
//! Propodus is a remote HTTP contract, not a Decapod compile-time dependency.
//! The transport is injectable so default tests remain local and the storage
//! boundary can be exercised without Vercel, Neon, or hosted credentials.

use crate::cli::CloudConfigSection;
use crate::core::auth;
use crate::core::cloud_backend::{
    CloudOnboardingEndpoints, CloudOnboardingExchangeResponse, CloudOnboardingStartResponse,
    CloudOnboardingState, CloudOnboardingStatusResponse, CloudSession, CloudSessionExchangeRequest,
    CloudSessionExchangeResponse, CloudSessionRefreshRequest,
};
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PropodusConfig {
    pub api_url: String,
    pub project_id: String,
    pub repo_id: String,
}

impl From<&CloudConfigSection> for PropodusConfig {
    fn from(config: &CloudConfigSection) -> Self {
        Self {
            api_url: config.api_url.clone(),
            project_id: config.project_id.clone(),
            repo_id: config.repo_id.clone(),
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
    Authentication(String),
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
            Self::Authentication(message) => write!(f, "Propodus authentication error: {message}"),
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

#[derive(Debug, Clone)]
pub struct PropodusClient<T = CurlTransport> {
    api_url: String,
    repo_id: String,
    credential: String,
    transport: T,
}

impl PropodusClient<CurlTransport> {
    pub fn from_cloud_config(config: &CloudConfigSection) -> Result<Self, PropodusClientError> {
        let credential = auth::load_cloud_credential(None)
            .map_err(|error| PropodusClientError::Authentication(error.to_string()))?;
        Self::with_transport(&config.into(), &credential.token, CurlTransport::default())
    }

    pub fn from_dogfood_cloud_config(
        config: &CloudConfigSection,
        identity: &RepositoryIdentity,
    ) -> Result<Self, PropodusClientError> {
        let config = PropodusConfig {
            api_url: config.api_url.clone(),
            project_id: config.project_id.clone(),
            repo_id: identity.canonical_name.clone(),
        };
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
    if let Ok(credential) = auth::load_cloud_credential(None) {
        if credential.source != auth::CredentialSource::MachineFile
            || !auth::cloud_session_is_expired(&credential)
        {
            return Ok(credential);
        }
        if let (Some(session_id), Some(refresh_token)) = (
            credential.session_id.as_deref(),
            credential.refresh_token.as_deref(),
        ) {
            let session = refresh_cloud_session(config, session_id, refresh_token, &transport)?;
            auth::store_machine_session(&session)
                .map_err(|error| PropodusClientError::Authentication(error.to_string()))?;
            return auth::load_machine_session()
                .map_err(|error| PropodusClientError::Authentication(error.to_string()))?
                .ok_or_else(|| {
                    PropodusClientError::Authentication(
                        "cloud session was not persisted".to_string(),
                    )
                });
        }
    }

    complete_cloud_onboarding(config, identity, transport)
}

fn complete_cloud_onboarding<T: PropodusTransport>(
    config: &PropodusConfig,
    identity: &RepositoryIdentity,
    transport: T,
) -> Result<auth::CloudCredential, PropodusClientError> {
    let endpoints = CloudOnboardingEndpoints::new(&config.api_url)
        .map_err(|error| PropodusClientError::Configuration(error.to_string()))?;
    let interactive = std::io::stdin().is_terminal()
        && std::env::var_os("GITHUB_ACTIONS").is_none()
        && std::env::var_os("DECAPOD_HEADLESS").is_none();
    let pending = auth::load_pending_cloud_onboarding()
        .map_err(|error| PropodusClientError::Authentication(error.to_string()))?
        .filter(|pending| {
            pending.api_url == config.api_url && pending.repo_id == identity.canonical_name
        });

    let (flow_id, url, expires_at) = if let Some(pending) = pending {
        (pending.flow_id, pending.url, pending.expires_at)
    } else {
        if !interactive {
            return Err(PropodusClientError::Authentication(
                "no cloud credential is configured; run this command in an interactive terminal to start browser onboarding, then rerun it here".to_string(),
            ));
        }
        let request = OnboardingStartRequest {
            repo_id: identity.canonical_name.clone(),
        };
        let body = serde_json::to_vec(&request)
            .map_err(|error| PropodusClientError::Decode(error.to_string()))?;
        let response = public_request(&transport, "POST", &endpoints.start(), Some(&body))?;
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
        auth::store_pending_cloud_onboarding(&pending)
            .map_err(|error| PropodusClientError::Authentication(error.to_string()))?;
        (flow_id, handoff.bootstrap_url, handoff.expires_at)
    };

    println!("Cloud onboarding URL: {url}");
    println!(
        "This one-time URL expires at {expires_at}; no credential is stored in the repository."
    );
    try_open_browser(&url);

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
        let response = public_request(&transport, "GET", &status_url, None)?;
        let status: CloudOnboardingStatusResponse = decode_json(&response.body)?;
        match status.state {
            CloudOnboardingState::Authorized => break,
            CloudOnboardingState::Canceled
            | CloudOnboardingState::Expired
            | CloudOnboardingState::Failed => {
                let _ = auth::clear_pending_cloud_onboarding();
                return Err(PropodusClientError::Authentication(format!(
                    "cloud onboarding ended with status {:?}",
                    status.state
                )));
            }
            CloudOnboardingState::Pending | CloudOnboardingState::Uncertain => {
                if !interactive || Instant::now() >= deadline {
                    return Err(PropodusClientError::Authentication(
                        "cloud onboarding is pending; finish the printed URL and rerun the command to resume".to_string(),
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
    )?;
    let exchange: CloudOnboardingExchangeResponse = decode_json(&response.body)?;
    let code = exchange.code.trim();
    if code.is_empty() {
        return Err(PropodusClientError::Authentication(
            "cloud onboarding exchange returned no code".to_string(),
        ));
    }

    let request = CloudSessionExchangeRequest::new(code)
        .map_err(|error| PropodusClientError::Authentication(error.to_string()))?;
    let body = serde_json::to_vec(&request)
        .map_err(|error| PropodusClientError::Decode(error.to_string()))?;
    let response = public_request(
        &transport,
        "POST",
        &endpoints.session_exchange(),
        Some(&body),
    )?;
    let session = CloudSessionExchangeResponse::into_session(decode_json(&response.body)?)
        .map_err(|error| PropodusClientError::Authentication(error.to_string()))?;
    session
        .validate()
        .map_err(|error| PropodusClientError::Authentication(error.to_string()))?;
    auth::store_machine_session(&session)
        .map_err(|error| PropodusClientError::Authentication(error.to_string()))?;
    auth::clear_pending_cloud_onboarding()
        .map_err(|error| PropodusClientError::Authentication(error.to_string()))?;
    auth::load_machine_session()
        .map_err(|error| PropodusClientError::Authentication(error.to_string()))?
        .ok_or_else(|| {
            PropodusClientError::Authentication("cloud session was not persisted".to_string())
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
    let request = CloudSessionRefreshRequest::new(session_id, refresh_token)
        .map_err(|error| PropodusClientError::Authentication(error.to_string()))?;
    let body = serde_json::to_vec(&request)
        .map_err(|error| PropodusClientError::Decode(error.to_string()))?;
    let response = public_request(transport, "POST", &endpoints.refresh(), Some(&body))?;
    CloudSessionExchangeResponse::into_session(decode_json(&response.body)?)
        .map_err(|error| PropodusClientError::Authentication(error.to_string()))
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
                "cloud.repo_id is required for Propodus todo operations".to_string(),
            ));
        }
        if credential.trim().is_empty() {
            return Err(PropodusClientError::Authentication(
                "a bearer credential is required".to_string(),
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
        401 => PropodusClientError::Authentication(message),
        403 => PropodusClientError::Service {
            status: response.status,
            code,
            message,
        },
        404 => PropodusClientError::NotFound(message),
        409 => PropodusClientError::Conflict(message),
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
