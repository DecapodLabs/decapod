use crate::core::error::DecapodError;
use crate::core::repo_identity::RepositoryIdentity;
use crate::core::time;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

pub const PUBLIC_CLOUD_BACKEND_UNAVAILABLE: &str = "Cloud todo persistence is not selected automatically. Use the optional Propodus HTTP adapter explicitly; local SQLite remains the default and no private backend dependency is required.";

pub const PROPODUS_TODO_ROUTE_SUMMARY: &str =
    "GET /api/health; GET /api/todos?repo_id=<repo>; POST /api/todos; PATCH /api/todos?id=<todo>";
pub const CLOUD_ONBOARDING_CONTRACT_VERSION: &str = "v1";
pub const CLOUD_ONBOARDING_START_ROUTE: &str = "/api/onboarding/start";
pub const CLOUD_ONBOARDING_STATUS_ROUTE: &str = "/api/onboarding/status";
pub const CLOUD_ONBOARDING_EXCHANGE_ROUTE: &str = "/api/onboarding/exchange";
pub const CLOUD_SESSION_EXCHANGE_ROUTE: &str = "/api/auth/session/exchange";
pub const CLOUD_SESSION_REFRESH_ROUTE: &str = "/api/auth/session/refresh";

/// Provider-neutral states returned while an external onboarding handoff is
/// being completed. Decapod does not interpret provider identity or policy
/// claims; it only carries this bounded state to the CLI/session boundary.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CloudOnboardingState {
    Pending,
    #[serde(rename = "ready", alias = "authorized")]
    Authorized,
    Canceled,
    Expired,
    #[serde(rename = "denied", alias = "failed")]
    Failed,
    Uncertain,
}

/// A trusted, one-time browser handoff that can also be printed for a
/// headless terminal. The URL is intentionally not persisted in repository
/// configuration and this contract carries no access or refresh credential.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CloudOnboardingHandoff {
    pub bootstrap_url: String,
    pub expires_at: String,
    pub poll_after_seconds: u64,
    pub state: CloudOnboardingState,
}

/// The repository binding sent to an external cloud service. The owner/name
/// comes from the Git remote; no local config field or fork allowlist is
/// treated as authenticated identity.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CloudRepositoryBinding {
    pub canonical_name: String,
    pub owner: String,
    pub repository: String,
}

impl From<&RepositoryIdentity> for CloudRepositoryBinding {
    fn from(identity: &RepositoryIdentity) -> Self {
        Self {
            canonical_name: identity.canonical_name.clone(),
            owner: identity.owner.clone(),
            repository: identity.repository.clone(),
        }
    }
}

/// Public, provider-neutral payload for starting the browser/loopback
/// handoff. Provider policy and identity verification happen after this
/// boundary.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CloudOnboardingStartRequest {
    pub contract_version: String,
    pub repository: CloudRepositoryBinding,
}

impl CloudOnboardingStartRequest {
    pub fn for_repository(identity: &RepositoryIdentity) -> Self {
        Self {
            contract_version: CLOUD_ONBOARDING_CONTRACT_VERSION.to_string(),
            repository: identity.into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CloudOnboardingStartResponse {
    #[serde(alias = "flow", alias = "id")]
    pub flow_id: String,
    #[serde(alias = "url")]
    pub bootstrap_url: String,
    pub expires_at: String,
    #[serde(default)]
    pub poll_after_seconds: Option<u64>,
}

impl CloudOnboardingStartResponse {
    pub fn into_handoff(self) -> Result<(String, CloudOnboardingHandoff), DecapodError> {
        let mut handoff = CloudOnboardingHandoff::new(&self.bootstrap_url, &self.expires_at)?;
        if let Some(poll_after_seconds) = self.poll_after_seconds {
            if poll_after_seconds == 0 {
                return Err(DecapodError::ValidationError(
                    "cloud onboarding poll interval must be greater than zero".to_string(),
                ));
            }
            handoff.poll_after_seconds = poll_after_seconds;
        }
        Ok((self.flow_id, handoff))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CloudOnboardingStatusResponse {
    #[serde(alias = "flow", alias = "id")]
    pub flow_id: String,
    #[serde(alias = "status")]
    pub state: CloudOnboardingState,
    #[serde(default)]
    pub expires_at: Option<String>,
    #[serde(default)]
    pub poll_after_seconds: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CloudOnboardingExchangeResponse {
    pub transaction_id: String,
    pub repository_id: String,
    pub code: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CloudSessionExchangeRequest {
    pub code: String,
}

impl CloudSessionExchangeRequest {
    pub fn new(code: &str) -> Result<Self, DecapodError> {
        let code = code.trim();
        if code.is_empty() || code.chars().any(|c| c.is_control() || c.is_whitespace()) {
            return Err(DecapodError::ValidationError(
                "cloud session exchange code must be a non-empty opaque value".to_string(),
            ));
        }
        Ok(Self {
            code: code.to_string(),
        })
    }
}

/// Machine-local session material returned by the cloud exchange. It is never
/// written to repository configuration or included in onboarding URLs.
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CloudSession {
    pub access_token: String,
    #[serde(default)]
    pub refresh_token: Option<String>,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub expires_at: Option<String>,
}

impl CloudSession {
    pub fn validate(&self) -> Result<(), DecapodError> {
        if self.access_token.trim().is_empty()
            || self
                .access_token
                .chars()
                .any(|c| c.is_control() || c.is_whitespace())
        {
            return Err(DecapodError::SessionError(
                "cloud exchange returned an invalid access credential".to_string(),
            ));
        }
        if self.refresh_token.as_deref().is_some_and(|token| {
            token.trim().is_empty() || token.chars().any(|c| c.is_control() || c.is_whitespace())
        }) {
            return Err(DecapodError::SessionError(
                "cloud exchange returned an invalid refresh credential".to_string(),
            ));
        }
        Ok(())
    }

    pub fn redacted_summary(&self) -> String {
        format!(
            "cloud session configured (refresh={}, expires_at={})",
            self.refresh_token.is_some(),
            self.expires_at.as_deref().unwrap_or("unspecified")
        )
    }
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CloudSessionExchangeResponse {
    #[serde(default)]
    pub credentials: Option<CloudSession>,
    #[serde(default)]
    pub session: Option<CloudSession>,
}

impl CloudSessionExchangeResponse {
    pub fn into_session(self) -> Result<CloudSession, DecapodError> {
        self.credentials.or(self.session).ok_or_else(|| {
            DecapodError::SessionError("cloud session exchange returned no credentials".to_string())
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CloudSessionRefreshRequest {
    pub session_id: String,
    pub refresh_token: String,
}

impl CloudSessionRefreshRequest {
    pub fn new(session_id: &str, refresh_token: &str) -> Result<Self, DecapodError> {
        let session_id = session_id.trim();
        let refresh_token = refresh_token.trim();
        if session_id.is_empty()
            || session_id
                .chars()
                .any(|c| c.is_control() || c.is_whitespace())
            || refresh_token.is_empty()
            || refresh_token
                .chars()
                .any(|c| c.is_control() || c.is_whitespace())
        {
            return Err(DecapodError::SessionError(
                "cloud refresh requires non-empty opaque credentials".to_string(),
            ));
        }
        Ok(Self {
            session_id: session_id.to_string(),
            refresh_token: refresh_token.to_string(),
        })
    }
}

/// Deterministic endpoint builder shared by a future live adapter and the
/// offline contract tests. Credentials are carried in request bodies/headers,
/// never in URLs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CloudOnboardingEndpoints {
    base_url: String,
}

impl CloudOnboardingEndpoints {
    pub fn new(base_url: &str) -> Result<Self, DecapodError> {
        let base_url = base_url.trim().trim_end_matches('/');
        if !(base_url.starts_with("https://") || base_url.starts_with("http://")) {
            return Err(DecapodError::Config(
                "cloud API URL must use http:// or https://".to_string(),
            ));
        }
        if base_url
            .chars()
            .any(|c| c.is_control() || c.is_whitespace())
        {
            return Err(DecapodError::Config(
                "cloud API URL must not contain whitespace or control characters".to_string(),
            ));
        }
        Ok(Self {
            base_url: base_url.to_string(),
        })
    }

    pub fn start(&self) -> String {
        format!("{}{}", self.base_url, CLOUD_ONBOARDING_START_ROUTE)
    }

    pub fn status(&self, flow_id: &str) -> Result<String, DecapodError> {
        let flow_id = validate_opaque_component(flow_id, "flow ID")?;
        Ok(format!(
            "{}{}?flow={}",
            self.base_url,
            CLOUD_ONBOARDING_STATUS_ROUTE,
            percent_encode_component(flow_id)
        ))
    }

    pub fn exchange(&self) -> String {
        format!("{}{}", self.base_url, CLOUD_ONBOARDING_EXCHANGE_ROUTE)
    }

    pub fn session_exchange(&self) -> String {
        format!("{}{}", self.base_url, CLOUD_SESSION_EXCHANGE_ROUTE)
    }

    pub fn refresh(&self) -> String {
        format!("{}{}", self.base_url, CLOUD_SESSION_REFRESH_ROUTE)
    }
}

fn validate_opaque_component<'a>(value: &'a str, label: &str) -> Result<&'a str, DecapodError> {
    let value = value.trim();
    if value.is_empty() || value.chars().any(|c| c.is_control() || c.is_whitespace()) {
        return Err(DecapodError::ValidationError(format!(
            "cloud {label} must be a non-empty opaque value"
        )));
    }
    Ok(value)
}

fn percent_encode_component(value: &str) -> String {
    value
        .bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (byte as char).to_string()
            }
            byte => format!("%{byte:02X}"),
        })
        .collect()
}

impl CloudOnboardingHandoff {
    pub const DEFAULT_POLL_AFTER_SECONDS: u64 = 2;

    pub fn new(bootstrap_url: &str, expires_at: &str) -> Result<Self, DecapodError> {
        let bootstrap_url = bootstrap_url.trim();
        let authority = bootstrap_url
            .strip_prefix("https://")
            .and_then(|value| value.split('/').next())
            .unwrap_or_default();
        let lower_url = bootstrap_url.to_ascii_lowercase();
        if !bootstrap_url.starts_with("https://")
            || bootstrap_url.chars().any(char::is_control)
            || bootstrap_url.chars().any(char::is_whitespace)
            || authority.contains('@')
            || lower_url.contains("access_token=")
            || lower_url.contains("refresh_token=")
            || lower_url.contains("session_token=")
        {
            return Err(DecapodError::ValidationError(
                "cloud onboarding handoff must be a trusted HTTPS URL without embedded credentials"
                    .to_string(),
            ));
        }
        let expires_at = expires_at.trim();
        if expires_at.is_empty()
            || expires_at.chars().any(char::is_control)
            || expires_at.chars().any(char::is_whitespace)
        {
            return Err(DecapodError::ValidationError(
                "cloud onboarding handoff requires a bounded expiration timestamp".to_string(),
            ));
        }
        Ok(Self {
            bootstrap_url: bootstrap_url.to_string(),
            expires_at: expires_at.to_string(),
            poll_after_seconds: Self::DEFAULT_POLL_AFTER_SECONDS,
            state: CloudOnboardingState::Pending,
        })
    }

    /// Safe terminal guidance for interactive and headless callers.
    pub fn terminal_instruction(&self) -> String {
        format!(
            "Open this one-time cloud onboarding URL, then resume the command: {} (expires {})",
            self.bootstrap_url, self.expires_at
        )
    }
}

pub fn unavailable_error() -> DecapodError {
    DecapodError::NotImplemented(PUBLIC_CLOUD_BACKEND_UNAVAILABLE.to_string())
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CloudInitRegistration {
    pub schema_version: String,
    pub provider: String,
    pub api_url: String,
    pub route: String,
    pub repo_id: String,
    pub repo_root_hint: String,
    pub created_at: String,
    pub writes: Vec<CloudWriteIntent>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CloudWriteIntent {
    pub table: String,
    pub operation: String,
    pub key: String,
}

impl CloudInitRegistration {
    pub fn for_init(provider: &str, api_url: &str, repo_id: &str, repo_root: &Path) -> Self {
        Self {
            schema_version: "1.0.0".to_string(),
            provider: provider.to_string(),
            api_url: api_url.trim_end_matches('/').to_string(),
            route: PROPODUS_TODO_ROUTE_SUMMARY.to_string(),
            repo_id: repo_id.to_string(),
            repo_root_hint: repo_root.display().to_string(),
            created_at: time::now_epoch_z(),
            writes: vec![
                CloudWriteIntent {
                    table: "todos".to_string(),
                    operation: "list/create".to_string(),
                    key: "repo_id".to_string(),
                },
                CloudWriteIntent {
                    table: "todos".to_string(),
                    operation: "claim/complete".to_string(),
                    key: "todo_id".to_string(),
                },
            ],
        }
    }
}

pub fn init_registration_outbox_path(repo_root: &Path) -> PathBuf {
    repo_root
        .join(".decapod")
        .join("managed")
        .join("cloud")
        .join("init-registration.json")
}

pub fn write_mock_init_registration(
    repo_root: &Path,
    registration: &CloudInitRegistration,
    dry_run: bool,
) -> Result<Option<PathBuf>, DecapodError> {
    if dry_run {
        return Ok(None);
    }

    let path = init_registration_outbox_path(repo_root);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(DecapodError::IoError)?;
    }
    let bytes = serde_json::to_vec_pretty(registration).map_err(|e| {
        DecapodError::ValidationError(format!(
            "Failed to serialize cloud init registration payload: {e}"
        ))
    })?;
    fs::write(&path, bytes).map_err(DecapodError::IoError)?;
    Ok(Some(path))
}

#[cfg(test)]
#[path = "../../../tests/unit/core/cloud_backend_tests.rs"]
mod tests;
