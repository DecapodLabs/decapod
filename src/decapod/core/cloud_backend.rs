use crate::core::error::DecapodError;
use crate::core::time;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

pub const PUBLIC_CLOUD_BACKEND_UNAVAILABLE: &str = "Cloud todo persistence is not selected automatically. Use the optional Propodus HTTP adapter explicitly; local SQLite remains the default and no private backend dependency is required.";

pub const PROPODUS_TODO_ROUTE_SUMMARY: &str =
    "GET /api/health; GET /api/todos?repo_id=<repo>; POST /api/todos; PATCH /api/todos?id=<todo>";

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
