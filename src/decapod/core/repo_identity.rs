//! Canonical GitHub repository gate for explicit Propodus cloud mode.
//!
//! The project file may describe a desired backend, but it must not be allowed
//! to choose which repository slice receives cloud state. The active cloud
//! path therefore derives the canonical owner/name from the Git remote and
//! requires an explicit dogfood gate. The gate is a temporary local opt-in,
//! not an authenticated immutable GitHub identity.

use crate::core::error::DecapodError;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::process::Command;

pub const DOGFOOD_REPOSITORY: &str = "DecapodLabs/decapod";
/// Temporary local dogfood gate. It is not sent to Propodus or presented as
/// authenticated repository identity until the service contract defines that
/// binding.
pub const PROPODUS_DOGFOOD_GATE_ENV: &str = "DECAPOD_PROPODUS_DOGFOOD";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepositoryIdentity {
    pub canonical_name: String,
    pub remote_url: String,
}

pub fn resolve_dogfood_repository_identity(
    repo_root: &Path,
) -> Result<RepositoryIdentity, DecapodError> {
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
            "cloud mode requires a configured origin Git remote".to_string(),
        ));
    }
    let remote_url = String::from_utf8_lossy(&output.stdout).trim().to_string();
    resolve_dogfood_repository_from_remote_with_gate(
        &remote_url,
        std::env::var(PROPODUS_DOGFOOD_GATE_ENV).ok().as_deref() == Some("1"),
    )
}

pub fn resolve_dogfood_repository_from_remote(
    remote_url: &str,
) -> Result<RepositoryIdentity, DecapodError> {
    resolve_dogfood_repository_from_remote_with_gate(
        remote_url,
        std::env::var(PROPODUS_DOGFOOD_GATE_ENV).ok().as_deref() == Some("1"),
    )
}

pub fn resolve_dogfood_repository_from_remote_with_gate(
    remote_url: &str,
    dogfood_gate_enabled: bool,
) -> Result<RepositoryIdentity, DecapodError> {
    let canonical_name = parse_github_repository(remote_url).ok_or_else(|| {
        DecapodError::ValidationError(format!(
            "cloud mode requires a GitHub origin remote; got unsupported remote `{remote_url}`"
        ))
    })?;
    if canonical_name != DOGFOOD_REPOSITORY {
        return Err(DecapodError::ValidationError(format!(
            "cloud dogfood is restricted to {DOGFOOD_REPOSITORY}; resolved {canonical_name}"
        )));
    }
    if !dogfood_gate_enabled {
        return Err(DecapodError::ValidationError(format!(
            "cloud dogfood requires {PROPODUS_DOGFOOD_GATE_ENV}=1; this is a temporary explicit gate, not authenticated repository identity"
        )));
    }

    Ok(RepositoryIdentity {
        canonical_name,
        remote_url: remote_url.trim().to_string(),
    })
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
mod tests {
    use super::parse_github_repository;

    #[test]
    fn parses_supported_github_remote_forms() {
        for remote in [
            "git@github.com:DecapodLabs/decapod.git",
            "ssh://git@github.com/DecapodLabs/decapod.git",
            "https://github.com/DecapodLabs/decapod",
        ] {
            assert_eq!(
                parse_github_repository(remote).as_deref(),
                Some("DecapodLabs/decapod")
            );
        }
    }

    #[test]
    fn rejects_non_github_and_ambiguous_remotes() {
        for remote in [
            "git@gitlab.com:DecapodLabs/decapod.git",
            "https://github.com/DecapodLabs/decapod/issues",
            "https://github.com/DecapodLabs/decapod/fork.git",
        ] {
            assert!(parse_github_repository(remote).is_none());
        }
    }

    #[test]
    fn dogfood_identity_is_canonical_and_explicitly_gated() {
        let identity = super::resolve_dogfood_repository_from_remote_with_gate(
            "git@github.com:DecapodLabs/decapod.git",
            true,
        )
        .expect("dogfood identity");
        assert_eq!(identity.canonical_name, "DecapodLabs/decapod");
    }

    #[test]
    fn dogfood_identity_rejects_other_repositories_and_missing_gate() {
        assert!(
            super::resolve_dogfood_repository_from_remote_with_gate(
                "git@github.com:DecapodLabs/decapod.git",
                false,
            )
            .is_err()
        );
        assert!(
            super::resolve_dogfood_repository_from_remote_with_gate(
                "git@github.com:DecapodLabs/propodus.git",
                true,
            )
            .is_err()
        );
    }
}
