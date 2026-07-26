//! Verified GitHub repository identity for explicit Propodus cloud mode.
//!
//! The project file may describe a desired backend, but it must not be allowed
//! to choose which repository slice receives cloud state. The active cloud
//! path therefore derives the canonical owner/name from the Git remote and
//! requires an immutable repository identifier from an authenticated resolver
//! boundary.

use crate::core::error::DecapodError;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::process::Command;

pub const DOGFOOD_REPOSITORY: &str = "DecapodLabs/decapod";
pub const GITHUB_REPOSITORY_ID_ENV: &str = "DECAPOD_GITHUB_REPOSITORY_ID";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepositoryIdentity {
    pub canonical_name: String,
    pub immutable_id: String,
    pub remote_url: String,
}

pub fn resolve_verified_repository_identity(
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
    let immutable_id = std::env::var(GITHUB_REPOSITORY_ID_ENV)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            DecapodError::ValidationError(format!(
                "cloud mode requires {GITHUB_REPOSITORY_ID_ENV} from an authenticated GitHub repository resolver"
            ))
        })?;

    resolve_verified_repository_identity_from_remote(&remote_url, &immutable_id)
}

pub fn resolve_verified_repository_identity_from_remote(
    remote_url: &str,
    immutable_id: &str,
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
    if immutable_id.trim().is_empty() {
        return Err(DecapodError::ValidationError(
            "cloud mode requires a non-empty immutable GitHub repository identity".to_string(),
        ));
    }

    Ok(RepositoryIdentity {
        canonical_name,
        immutable_id: immutable_id.trim().to_string(),
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
    use super::{parse_github_repository, resolve_verified_repository_identity_from_remote};

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
    fn verified_identity_is_canonical_and_immutable() {
        let identity = resolve_verified_repository_identity_from_remote(
            "git@github.com:DecapodLabs/decapod.git",
            "repo:123",
        )
        .expect("verified dogfood identity");
        assert_eq!(identity.canonical_name, "DecapodLabs/decapod");
        assert_eq!(identity.immutable_id, "repo:123");
    }

    #[test]
    fn verified_identity_rejects_other_repositories_and_missing_ids() {
        assert!(
            resolve_verified_repository_identity_from_remote(
                "git@github.com:DecapodLabs/propodus.git",
                "repo:123",
            )
            .is_err()
        );
        assert!(
            resolve_verified_repository_identity_from_remote(
                "git@github.com:DecapodLabs/decapod.git",
                " ",
            )
            .is_err()
        );
    }
}
