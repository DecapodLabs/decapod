//! Provider-neutral repository-to-datastore routing.
//!
//! Decapod chooses only the logical backend from `.decapod/config.toml` and
//! derives the repository scope from the Git origin.  The local route is a
//! repository-owned SQLite path.  A cloud route is an opaque URI returned by
//! the authenticated remote/session boundary; Decapod does not construct or
//! interpret provider-specific storage URLs here.

use crate::cli::{BackendType, DecapodProjectConfig};
use crate::core::error::{CloudAuthDiagnostic, CloudAuthStatus, DecapodError};
use crate::core::repo_identity::{RepositoryIdentity, resolve_repository_identity};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const LOCAL_DATASTORE_RELATIVE_PATH: &str = ".decapod/data/decapod.db";

/// The logical backend selection plus the repository scope it is allowed to
/// address.  Local selection deliberately does not require a GitHub remote;
/// cloud selection does, because the remote is the source of repository
/// tenancy rather than project configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackendSelection {
    backend: BackendType,
    repository: Option<RepositoryIdentity>,
    project_root: PathBuf,
}

impl BackendSelection {
    pub fn from_project(project_root: &Path) -> Result<Self, DecapodError> {
        let config = DecapodProjectConfig::load(project_root)?;
        Self::resolve(project_root, config.repo.effective_backend())
    }

    pub fn resolve(project_root: &Path, backend: BackendType) -> Result<Self, DecapodError> {
        let repository = backend
            .is_cloud()
            .then(|| resolve_repository_identity(project_root))
            .transpose()?;
        Ok(Self {
            backend,
            repository,
            project_root: project_root.to_path_buf(),
        })
    }

    pub fn backend(&self) -> BackendType {
        self.backend
    }

    pub fn repository_identity(&self) -> Option<&RepositoryIdentity> {
        self.repository.as_ref()
    }

    /// Bind the logical selection to the actual datastore route.
    ///
    /// `remote_uri` is intentionally supplied by the authenticated/session
    /// boundary.  It is opaque to Decapod and is never derived from project
    /// configuration or assembled from a provider name.
    pub fn route(&self, remote_uri: Option<&str>) -> Result<BackendRoute, DecapodError> {
        match self.backend {
            BackendType::Local => {
                if remote_uri.is_some() {
                    return Err(DecapodError::Config(
                        "local backend must not receive a remote datastore route".to_string(),
                    ));
                }
                Ok(BackendRoute::Local {
                    path: self.project_root.join(LOCAL_DATASTORE_RELATIVE_PATH),
                })
            }
            BackendType::Cloud => {
                let repository = self.repository.clone().ok_or_else(|| {
                    DecapodError::ValidationError(
                        "cloud backend requires a resolved repository identity".to_string(),
                    )
                })?;
                let uri = validate_remote_uri(remote_uri.ok_or_else(|| {
                    DecapodError::SessionError(
                        "cloud backend requires an authenticated datastore route for the resolved repository"
                            .to_string(),
                    )
                })?)?;
                Ok(BackendRoute::Cloud { repository, uri })
            }
        }
    }

    /// Resolve the versioned Decapod storage context for a physical route.
    ///
    /// The bearer is an opaque session credential and is retained only in
    /// memory. It is never serialized into the context or treated as a
    /// repository/organization identity.
    pub fn storage_context(
        &self,
        remote_uri: Option<&str>,
        bearer: Option<&str>,
    ) -> Result<StorageContext, DecapodError> {
        StorageContext::from_route(self.route(remote_uri)?, bearer)
    }
}

/// A fully bound datastore route.  The cloud URI is opaque: callers may pass
/// it to Dactyl, but Decapod does not know whether it is backed by Neon,
/// Vercel, or another compatible service.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BackendRoute {
    Local {
        path: PathBuf,
    },
    Cloud {
        repository: RepositoryIdentity,
        uri: String,
    },
}

impl BackendRoute {
    pub fn local_path(&self) -> Option<&Path> {
        match self {
            Self::Local { path } => Some(path),
            Self::Cloud { .. } => None,
        }
    }

    pub fn cloud_uri(&self) -> Option<&str> {
        match self {
            Self::Local { .. } => None,
            Self::Cloud { uri, .. } => Some(uri),
        }
    }

    pub fn repository_identity(&self) -> Option<&RepositoryIdentity> {
        match self {
            Self::Local { .. } => None,
            Self::Cloud { repository, .. } => Some(repository),
        }
    }
}

/// Versioned, backend-neutral storage target passed from Decapod policy to a
/// physical driver. Cloud tenancy and authorization are deliberately not
/// modeled as local storage fields: the remote route carries only the logical
/// repository scope, while Propodus remains responsible for resolving the
/// authenticated principal and its effective authorization.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StorageContext {
    version: u16,
    route: BackendRoute,
    #[serde(skip)]
    bearer: Option<String>,
}

impl StorageContext {
    pub const CURRENT_VERSION: u16 = 1;

    pub fn from_route(route: BackendRoute, bearer: Option<&str>) -> Result<Self, DecapodError> {
        let bearer = bearer.map(str::trim).filter(|value| !value.is_empty());
        match route {
            BackendRoute::Local { .. } if bearer.is_some() => Err(DecapodError::Config(
                "local storage context must not contain a cloud session credential".to_string(),
            )),
            BackendRoute::Cloud { .. } if bearer.is_none() => {
                Err(DecapodError::CloudAuth(CloudAuthDiagnostic::new(
                    CloudAuthStatus::Missing,
                    "remote storage requires an authenticated opaque session context",
                    "acquire or refresh the cloud session, then retry the command",
                )))
            }
            route => Ok(Self {
                version: Self::CURRENT_VERSION,
                route,
                bearer: bearer.map(str::to_owned),
            }),
        }
    }

    pub fn version(&self) -> u16 {
        self.version
    }

    pub fn route(&self) -> &BackendRoute {
        &self.route
    }

    pub fn bearer(&self) -> Option<&str> {
        self.bearer.as_deref()
    }

    pub fn is_local(&self) -> bool {
        matches!(self.route, BackendRoute::Local { .. })
    }

    pub fn is_remote(&self) -> bool {
        matches!(self.route, BackendRoute::Cloud { .. })
    }
}

fn validate_remote_uri(raw: &str) -> Result<String, DecapodError> {
    let uri = raw.trim();
    if uri.is_empty()
        || uri.chars().any(char::is_control)
        || uri.chars().any(char::is_whitespace)
        || !(uri.starts_with("https://") || uri.starts_with("http://"))
    {
        return Err(DecapodError::Config(
            "cloud datastore route must be a non-empty HTTP(S) URI without whitespace or control characters"
                .to_string(),
        ));
    }
    let authority = uri
        .split_once("://")
        .and_then(|(_, value)| value.split('/').next())
        .unwrap_or_default();
    if authority.is_empty() || authority.contains('@') {
        return Err(DecapodError::Config(
            "cloud datastore route must not contain embedded credentials".to_string(),
        ));
    }
    Ok(uri.to_string())
}

#[cfg(test)]
#[path = "../../../tests/unit/core/backend_tests.rs"]
mod tests;
