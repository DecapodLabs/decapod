//! Decapod's narrow boundary to the Dactyl physical storage contract.
//!
//! This module deliberately does not replace the canonical local SQLite
//! runtime yet. Dactyl's current local adapter is a separate pure-Rust store,
//! so opening `.decapod/data/decapod.db` through it would create a second
//! authority rather than migrate Decapod safely. The bridge is therefore
//! usable for backend-neutral conformance work and authenticated cloud routes,
//! while local call-site migration remains gated on the compatibility matrix.

use crate::core::backend::{BackendRoute, StorageContext};
use crate::core::error::{CloudAuthDiagnostic, CloudAuthStatus, DecapodError};
use dactyl_db::{AccessMode, AtomicResult, Connection, OpenOptions, Operation, Parameter, Rows};
use std::time::Duration;

pub use dactyl_db::{OperationResult, WriteResult};

const DEFAULT_LOCK_TIMEOUT: Duration = Duration::from_millis(250);

/// A route-scoped Dactyl driver. The underlying connection never escapes this
/// wrapper, so Decapod callers use Dactyl's operation/result contract rather
/// than backend-specific handles.
pub struct DactylBridge {
    connection: Connection,
}

impl DactylBridge {
    /// Open Dactyl's isolated in-memory store for conformance tests and
    /// adapter probes. This is not the Decapod canonical local store.
    pub fn open_memory() -> Result<Self, DecapodError> {
        Self::open_route(
            dactyl_db::DatastoreRoute::sqlite(":memory:"),
            AccessMode::ReadWrite,
        )
    }

    /// Open Dactyl's isolated in-memory store with an explicit access mode.
    pub fn open_memory_with_access_mode(access_mode: AccessMode) -> Result<Self, DecapodError> {
        Self::open_route(dactyl_db::DatastoreRoute::sqlite(":memory:"), access_mode)
    }

    /// Bind a governed backend route to Dactyl.
    ///
    /// Local canonical SQLite is intentionally rejected until Dactyl proves a
    /// compatible file-backed driver. Cloud routes require a separate
    /// machine-local bearer credential and are passed through as opaque HTTP
    /// endpoints; this method never derives provider URLs or silently falls
    /// back to local storage.
    pub fn from_backend_route(
        route: &BackendRoute,
        access_mode: AccessMode,
        bearer: Option<&str>,
    ) -> Result<Self, DecapodError> {
        match route {
            BackendRoute::Local { path } => Err(DecapodError::NotImplemented(format!(
                "Dactyl local compatibility is not proven for canonical SQLite path {}",
                path.display()
            ))),
            BackendRoute::Cloud { uri, .. } => {
                let bearer = bearer
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .ok_or_else(|| {
                        DecapodError::CloudAuth(CloudAuthDiagnostic::new(
                            CloudAuthStatus::Missing,
                            "cloud storage requires a machine-local session credential",
                            "acquire or refresh the cloud session, then retry the command",
                        ))
                    })?;
                Self::open_route(
                    dactyl_db::DatastoreRoute::neon(uri.clone(), Some(bearer.to_string())),
                    access_mode,
                )
            }
        }
    }

    /// Open the physical driver from a Decapod-owned storage context.
    ///
    /// The context's bearer is passed as opaque authentication material to
    /// Dactyl and is never interpreted as organization, user, or repository
    /// policy by this bridge.
    pub fn from_storage_context(
        context: &StorageContext,
        access_mode: AccessMode,
    ) -> Result<Self, DecapodError> {
        Self::from_backend_route(context.route(), access_mode, context.bearer())
    }

    pub fn read(&self, sql: &str, params: &[Parameter]) -> Result<Rows, DecapodError> {
        Ok(self.connection.read(sql, params)?)
    }

    pub fn write(&self, sql: &str, params: &[Parameter]) -> Result<WriteResult, DecapodError> {
        Ok(self.connection.write_result(sql, params)?)
    }

    pub fn atomic(&self, operations: &[Operation]) -> Result<AtomicResult, DecapodError> {
        Ok(self.connection.atomic(operations)?)
    }

    pub fn access_mode(&self) -> AccessMode {
        self.connection.access_mode()
    }

    fn open_route(
        route: dactyl_db::DatastoreRoute,
        access_mode: AccessMode,
    ) -> Result<Self, DecapodError> {
        let connection = Connection::open_with_options(
            route,
            OpenOptions {
                access_mode,
                lock_timeout: DEFAULT_LOCK_TIMEOUT,
            },
        )?;
        Ok(Self { connection })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::backend::LOCAL_DATASTORE_RELATIVE_PATH;
    use crate::core::error::{DecapodError, StorageFailureKind};
    use crate::core::repo_identity::RepositoryIdentity;
    use std::path::PathBuf;

    fn no_params() -> Vec<Parameter> {
        Vec::new()
    }

    #[test]
    fn explicit_ids_and_atomic_rollback_are_backend_neutral() {
        let bridge = DactylBridge::open_memory().expect("memory bridge");
        bridge
            .atomic(&[Operation::schema(
                "CREATE TABLE tasks (id INTEGER PRIMARY KEY, title TEXT NOT NULL, revision INTEGER NOT NULL)",
                no_params(),
            )])
            .expect("schema");

        let insert = |id: i64, title: &str| {
            Operation::write(
                "INSERT INTO tasks (id, title, revision) VALUES (?, ?, ?)",
                vec![id.into(), title.into(), 0_i64.into()],
            )
        };

        bridge
            .atomic(&[insert(101, "stable")])
            .expect("explicit insert");
        let rolled_back = bridge.atomic(&[insert(102, "temporary"), insert(101, "conflict")]);
        assert!(rolled_back.is_err(), "duplicate key must abort the batch");

        let rows = bridge
            .read("SELECT id, title, revision FROM tasks", &no_params())
            .expect("read rows");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows.as_slice()[0].get_int("id").expect("id"), 101);
        assert_eq!(
            rows.as_slice()[0].get_str("title").expect("title"),
            "stable"
        );
    }

    #[test]
    fn read_only_access_is_rejected_by_physical_driver() {
        let bridge = DactylBridge::open_memory_with_access_mode(AccessMode::ReadOnly)
            .expect("read-only memory bridge");
        let error = bridge
            .write("INSERT INTO missing (id) VALUES (?)", &[1_i64.into()])
            .expect_err("read-only route must reject writes");
        assert_eq!(error.storage_failure_kind(), StorageFailureKind::Capability);
    }

    #[test]
    fn canonical_local_route_fails_closed_without_second_store() {
        let route = BackendRoute::Local {
            path: PathBuf::from(LOCAL_DATASTORE_RELATIVE_PATH),
        };
        match DactylBridge::from_backend_route(&route, AccessMode::ReadWrite, Some("ignored")) {
            Err(DecapodError::NotImplemented(message)) => {
                assert!(message.contains("canonical SQLite"));
            }
            Err(other) => panic!("unexpected error: {other}"),
            Ok(_) => panic!("canonical SQLite route must not open a second store"),
        }
    }

    #[test]
    fn cloud_route_requires_explicit_bearer_and_keeps_repository_scope() {
        let identity = RepositoryIdentity {
            canonical_name: "DecapodLabs/decapod".to_string(),
            owner: "DecapodLabs".to_string(),
            repository: "decapod".to_string(),
            remote_url: "git@github.com:DecapodLabs/decapod.git".to_string(),
        };
        let route = BackendRoute::Cloud {
            repository: identity,
            uri: "https://example.invalid/api/v1/store".to_string(),
        };

        let missing = match DactylBridge::from_backend_route(&route, AccessMode::ReadWrite, None) {
            Err(error) => error,
            Ok(_) => panic!("cloud must fail closed without a bearer"),
        };
        assert!(matches!(missing, DecapodError::CloudAuth(_)));

        let bridge = DactylBridge::from_backend_route(
            &route,
            AccessMode::ReadOnly,
            Some("opaque-session-token"),
        )
        .expect("constructing a cloud route does not perform I/O");
        assert_eq!(bridge.access_mode(), AccessMode::ReadOnly);
    }

    #[test]
    fn storage_context_binds_the_opaque_bearer_without_serializing_it() {
        let identity = RepositoryIdentity {
            canonical_name: "DecapodLabs/decapod".to_string(),
            owner: "DecapodLabs".to_string(),
            repository: "decapod".to_string(),
            remote_url: "git@github.com:DecapodLabs/decapod.git".to_string(),
        };
        let route = BackendRoute::Cloud {
            repository: identity,
            uri: "https://example.invalid/api/v1/store".to_string(),
        };
        let context = StorageContext::from_route(route, Some("opaque-session-token"))
            .expect("remote context");
        let bridge = DactylBridge::from_storage_context(&context, AccessMode::ReadOnly)
            .expect("constructing cloud route does not perform I/O");
        assert_eq!(context.version(), StorageContext::CURRENT_VERSION);
        assert_eq!(context.bearer(), Some("opaque-session-token"));
        assert!(bridge.access_mode() == AccessMode::ReadOnly);
        let encoded = serde_json::to_string(&context).expect("context JSON");
        assert!(!encoded.contains("opaque-session-token"));
        assert!(!encoded.contains("organization"));
    }

    #[test]
    fn dactyl_errors_map_to_decapod_storage_classes() {
        let busy = DecapodError::from(dactyl_db::DactylError::Adapter {
            kind: dactyl_db::AdapterErrorKind::Busy,
            code: None,
            message: "busy".to_string(),
        });
        assert_eq!(busy.storage_failure_kind(), StorageFailureKind::Contention);

        let constraint = DecapodError::from(dactyl_db::DactylError::Adapter {
            kind: dactyl_db::AdapterErrorKind::Constraint,
            code: None,
            message: "constraint".to_string(),
        });
        assert_eq!(
            constraint.storage_failure_kind(),
            StorageFailureKind::Constraint
        );
    }
}
