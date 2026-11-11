//! Stable local user / owner / repository directory.
//!
//! A local SQLite file is a single repository physical shard. Its tenant
//! labels live in one row, rather than every event, task or index entry.
//! A Git origin and a local OS UID are observations, never permission grants;
//! hosted authorization remains server-owned and transactional in Propodus.

use crate::core::db::{Connection, OptionalExtension, params};
use crate::core::error::DecapodError;
use crate::core::{repo_identity, ulid};
use serde::{Deserialize, Serialize};
use std::ffi::OsStr;
use std::path::Path;

pub const SQLITE_SCOPE_DDL: &str = "CREATE TABLE IF NOT EXISTS repository_scope (
    repository_key INTEGER PRIMARY KEY CHECK(repository_key=1),
    version INTEGER NOT NULL CHECK(version=1),
    stable_id TEXT NOT NULL CHECK(length(stable_id)=26),
    provider INTEGER NOT NULL CHECK(provider IN (0,1)),
    owner_name TEXT,
    owner_kind INTEGER NOT NULL DEFAULT 0 CHECK(owner_kind IN (0,1,2)),
    organization_id TEXT,
    repository_name TEXT,
    local_user TEXT,
    CHECK (
       (provider=0 AND owner_name IS NULL AND repository_name IS NULL)
       OR
       (provider=1 AND owner_name IS NOT NULL AND repository_name IS NOT NULL
         AND length(owner_name)>0 AND length(repository_name)>0)
    )
);";

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct RepositoryScope {
    pub stable_id: String,
    pub owner_name: Option<String>,
    /// Zero means unknown; an observed remote cannot prove user vs organization.
    pub owner_kind: u8,
    /// Verified organization identity requires an external authority.
    pub organization_id: Option<String>,
    pub repository_name: Option<String>,
    /// Observed local OS user, never a cloud authentication subject.
    pub local_user: Option<String>,
    pub origin_observed: bool,
}

fn project_root(data_root: &Path) -> Option<&Path> {
    let parent = data_root.parent()?;
    if data_root.file_name() == Some(OsStr::new("data"))
        && parent.file_name() == Some(OsStr::new(".decapod"))
    {
        parent.parent()
    } else {
        Some(parent)
    }
}

fn operating_user() -> Option<String> {
    #[cfg(unix)]
    {
        Some(format!("os-uid:{}", unsafe { libc::geteuid() }))
    }
    #[cfg(not(unix))]
    {
        None
    }
}

type ScopeRecord = (
    i64,
    String,
    i64,
    Option<String>,
    i64,
    Option<String>,
    Option<String>,
    Option<String>,
);

pub fn read(conn: &Connection) -> Result<RepositoryScope, DecapodError> {
    let raw: Option<ScopeRecord> = conn
        .query_row(
            "SELECT version, stable_id, provider, owner_name, owner_kind,
                    organization_id, repository_name, local_user
             FROM repository_scope WHERE repository_key=1",
            [],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                    row.get(7)?,
                ))
            },
        )
        .optional()?;
    let (
        version,
        stable_id,
        provider,
        owner_name,
        owner_kind,
        organization_id,
        repository_name,
        local_user,
    ) = raw.ok_or_else(|| {
        DecapodError::ValidationError(
            "STORAGE_SCOPE_MISSING: repository directory has not been initialized".into(),
        )
    })?;
    if version != 1
        || stable_id.len() != 26
        || !matches!(provider, 0 | 1)
        || !matches!(owner_kind, 0..=2)
        || (provider == 0 && (owner_name.is_some() || repository_name.is_some()))
        || (provider == 1 && (owner_name.is_none() || repository_name.is_none()))
    {
        return Err(DecapodError::ValidationError(
            "STORAGE_SCOPE_INVALID: repository directory is inconsistent".into(),
        ));
    }
    Ok(RepositoryScope {
        stable_id,
        owner_name,
        owner_kind: owner_kind as u8,
        organization_id,
        repository_name,
        local_user,
        origin_observed: provider == 1,
    })
}

/// Create a repository binding only if absent. Preserve the old snapshot
/// across Git remote renames and never change scope during ordinary init.
pub fn ensure(conn: &Connection, data_root: &Path) -> Result<RepositoryScope, DecapodError> {
    if conn.is_read_only() {
        return Err(DecapodError::ValidationError(
            "STORAGE_SCOPE_READ_ONLY: writes require the governed storage owner".into(),
        ));
    }
    conn.execute_batch(SQLITE_SCOPE_DDL)?;
    let exists: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM repository_scope WHERE repository_key=1)",
        [],
        |row| row.get(0),
    )?;
    if !exists {
        let observed = project_root(data_root)
            .and_then(|path| repo_identity::resolve_repository_identity(path).ok());
        let (provider, owner, repo) = if let Some(value) = observed {
            (1_i64, Some(value.owner), Some(value.repository))
        } else {
            (0_i64, None, None)
        };
        let id = ulid::new_ulid().to_string();
        let user = operating_user();
        conn.execute(
            "INSERT OR IGNORE INTO repository_scope(
                repository_key,version,stable_id,provider,owner_name,owner_kind,
                organization_id,repository_name,local_user
             ) VALUES (1,1,?1,?2,?3,0,NULL,?4,?5)",
            params![id, provider, owner, repo, user],
        )?;
    }
    read(conn)
}
