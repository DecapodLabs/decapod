use crate::core::error::DecapodError;
use crate::core::time;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

pub const PUBLIC_CLOUD_BACKEND_UNAVAILABLE: &str = "Cloud todo persistence is not selected automatically. Use the optional Propodus HTTP adapter explicitly; local SQLite remains the default and no private backend dependency is required.";

pub const PROPODUS_TODO_ROUTE_SUMMARY: &str =
    "GET /api/health; GET /api/todos?repo_id=<repo>; POST /api/todos; PATCH /api/todos?id=<todo>";

/// Provider-neutral states returned while an external onboarding handoff is
/// being completed. Decapod does not interpret provider identity or policy
/// claims; it only carries this bounded state to the CLI/session boundary.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CloudOnboardingState {
    Pending,
    Authorized,
    Canceled,
    Expired,
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
    pub project_id: String,
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
    pub fn for_init(
        provider: &str,
        api_url: &str,
        project_id: &str,
        repo_id: &str,
        repo_root: &Path,
    ) -> Self {
        Self {
            schema_version: "1.0.0".to_string(),
            provider: provider.to_string(),
            api_url: api_url.trim_end_matches('/').to_string(),
            route: PROPODUS_TODO_ROUTE_SUMMARY.to_string(),
            project_id: project_id.to_string(),
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
mod tests {
    use super::{CloudOnboardingHandoff, CloudOnboardingState};

    #[test]
    fn onboarding_handoff_is_provider_neutral_and_headless_safe() {
        let handoff = CloudOnboardingHandoff::new(
            "https://cloud.example.test/onboard/one-time",
            "2030-01-01T00:00:00Z",
        )
        .expect("valid handoff");

        assert_eq!(handoff.state, CloudOnboardingState::Pending);
        assert_eq!(handoff.poll_after_seconds, 2);
        assert!(handoff.terminal_instruction().contains("one-time"));
        assert!(!handoff.terminal_instruction().contains("token"));
    }

    #[test]
    fn onboarding_handoff_rejects_untrusted_or_unbounded_values() {
        assert!(
            CloudOnboardingHandoff::new(
                "http://cloud.example.test/onboard",
                "2030-01-01T00:00:00Z",
            )
            .is_err()
        );
        assert!(
            CloudOnboardingHandoff::new(
                "https://cloud.example.test/onboard?access_token=raw",
                "2030-01-01T00:00:00Z",
            )
            .is_err()
        );
        assert!(CloudOnboardingHandoff::new("https://cloud.example.test/onboard", "",).is_err());
    }
}
