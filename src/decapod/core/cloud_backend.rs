use crate::core::error::DecapodError;
use crate::core::time;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

pub const PUBLIC_CLOUD_BACKEND_UNAVAILABLE: &str = "Cloud backend is not included in the public Decapod crate. Use local mode; future cloud integrations must attach through the Vercel backend boundary without private git/path dependencies.";

pub const INIT_REGISTRATION_ROUTE: &str = "POST /api/decapod/init/register";

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
            route: INIT_REGISTRATION_ROUTE.to_string(),
            project_id: project_id.to_string(),
            repo_id: repo_id.to_string(),
            repo_root_hint: repo_root.display().to_string(),
            created_at: time::now_epoch_z(),
            writes: vec![
                CloudWriteIntent {
                    table: "repositories".to_string(),
                    operation: "upsert".to_string(),
                    key: "repo_id".to_string(),
                },
                CloudWriteIntent {
                    table: "init_events".to_string(),
                    operation: "insert".to_string(),
                    key: "event_id".to_string(),
                },
            ],
        }
    }
}

pub fn init_registration_outbox_path(repo_root: &Path) -> PathBuf {
    repo_root
        .join(".decapod")
        .join("generated")
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
