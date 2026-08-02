//! Canonical GitHub repository identity for the cloud backend.
//!
//! The project file may describe a desired backend, but it must not be allowed
//! to choose which repository slice receives cloud state. The active cloud
//! path therefore derives the canonical owner/name from the Git remote and
//! The external service remains responsible for authenticating and
//! authorizing that identity; this module only produces the client binding.

use crate::core::error::DecapodError;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::process::Command;

pub const DOGFOOD_REPOSITORY: &str = "DecapodLabs/decapod";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepositoryIdentity {
    pub canonical_name: String,
    pub owner: String,
    pub repository: String,
    pub remote_url: String,
}

pub fn resolve_repository_identity(repo_root: &Path) -> Result<RepositoryIdentity, DecapodError> {
    let output = Command::new("git")
        .current_dir(repo_root)
        .args(["remote", "get-url", "origin"])
        .output()
        .map_err(|error| {
            DecapodError::ValidationError(format!(
                "unable to resolve the GitHub repository remote: {error}"
            ))
        })?;
    if !output.status.success() {
        return Err(DecapodError::ValidationError(
            "cloud backend requires a configured origin Git remote".to_string(),
        ));
    }
    let remote_url = String::from_utf8_lossy(&output.stdout).trim().to_string();
    resolve_repository_identity_from_remote(&remote_url)
}

pub fn resolve_repository_identity_from_remote(
    remote_url: &str,
) -> Result<RepositoryIdentity, DecapodError> {
    let canonical_name = parse_github_repository(remote_url).ok_or_else(|| {
        DecapodError::ValidationError(format!(
            "cloud backend requires a GitHub origin remote; got unsupported remote `{remote_url}`"
        ))
    })?;
    let (owner, repository) = canonical_name.split_once('/').ok_or_else(|| {
        DecapodError::ValidationError("GitHub remote did not resolve to owner/name".to_string())
    })?;
    let owner = owner.to_string();
    let repository = repository.to_string();

    Ok(RepositoryIdentity {
        canonical_name,
        owner,
        repository,
        remote_url: remote_url.trim().to_string(),
    })
}

/// Compatibility alias for callers that adopted the earlier dogfood naming.
/// Runtime identity is no longer restricted to this repository; authorization
/// belongs to the cloud service after it verifies the authenticated identity.
pub fn resolve_dogfood_repository_identity(
    repo_root: &Path,
) -> Result<RepositoryIdentity, DecapodError> {
    resolve_repository_identity(repo_root)
}

/// Compatibility alias for the earlier dogfood-specific API.
pub fn resolve_dogfood_repository_from_remote(
    remote_url: &str,
) -> Result<RepositoryIdentity, DecapodError> {
    resolve_repository_identity_from_remote(remote_url)
}

pub fn parse_github_repository(remote_url: &str) -> Option<String> {
    let value = remote_url.trim().trim_end_matches('/');
    let path = value
        .strip_prefix("git@github.com:")
        .or_else(|| value.strip_prefix("ssh://git@github.com/"))
        .or_else(|| value.strip_prefix("https://github.com/"))
        .or_else(|| value.strip_prefix("http://github.com/"))?;
    let path = path.strip_suffix(".git").unwrap_or(path);
    let mut parts = path.split('/');
    let owner = parts.next()?.trim();
    let repo = parts.next()?.trim();
    if owner.is_empty() || repo.is_empty() || parts.next().is_some() {
        return None;
    }
    Some(format!("{owner}/{repo}"))
}

#[cfg(test)]
#[path = "../../../tests/unit/core/repo_identity_tests.rs"]
mod tests;
