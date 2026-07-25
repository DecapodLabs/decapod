//! Typed, optional client boundary for the external Propodus todo service.
//!
//! Propodus is a remote HTTP contract, not a Decapod compile-time dependency.
//! The transport is injectable so default tests remain local and the storage
//! boundary can be exercised without Vercel, Neon, or hosted credentials.

use crate::cli::CloudConfigSection;
use crate::core::auth;
use crate::core::storage::{Task as StorageTask, TodoStore};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::process::Command;

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
            "--header",
            &format!("Authorization: Bearer {bearer}"),
            "--connect-timeout",
            &self.connect_timeout_seconds.to_string(),
            "--max-time",
            &self.max_time_seconds.to_string(),
            "--write-out",
            "\n%{http_code}",
            url,
        ]);
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

    pub fn claim_todo(&self, id: &str, actor: &str) -> Result<(), PropodusClientError> {
        self.update_todo(id, PROPODUS_STATUS_IN_PROGRESS, actor)
    }

    pub fn complete_todo(&self, id: &str, actor: &str) -> Result<(), PropodusClientError> {
        self.update_todo(id, PROPODUS_STATUS_COMPLETED, actor)
    }

    fn update_todo(&self, id: &str, status: &str, actor: &str) -> Result<(), PropodusClientError> {
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
            Ok(())
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
    ) -> anyhow::Result<()> {
        self.client
            .create_todo(&task.title, task.description.as_deref(), Some(&actor))
            .map(|_| ())
            .map_err(|error| anyhow::anyhow!(error.to_string()))
    }

    async fn claim_task(&self, id: &str, actor: String) -> anyhow::Result<()> {
        self.client
            .claim_todo(id, &actor)
            .map_err(|error| anyhow::anyhow!(error.to_string()))
    }

    async fn complete_task(
        &self,
        id: &str,
        actor: String,
        _resolution: String,
    ) -> anyhow::Result<()> {
        self.client
            .complete_todo(id, &actor)
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
