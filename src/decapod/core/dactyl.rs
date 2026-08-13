//! Decapod's narrow boundary to the Dactyl physical storage contract.
//!
//! This module deliberately keeps legacy migration explicit. Dactyl's local
//! route is a versioned snapshot, so opening an existing SQLite
//! `.decapod/data/decapod.db` without conversion would create a second
//! authority rather than migrate Decapod safely. The normal runtime therefore
//! uses only the Dactyl snapshot contract after a separately proven cutover;
//! the optional `legacy-import` feature exposes Dactyl's one-shot conversion
//! boundary without making it part of the default runtime.

use crate::core::backend::{BackendRoute, StorageContext};
use crate::core::error::{CloudAuthDiagnostic, CloudAuthStatus, DecapodError};
use crate::core::schemas;
use dactyl_db::{AccessMode, AtomicResult, Connection, OpenOptions, Operation, Parameter, Rows};
use std::path::Path;
use std::time::Duration;

pub use dactyl_db::{OperationResult, WriteResult};

#[cfg(feature = "legacy-import")]
pub use dactyl_db::ImportReport;

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
            None,
        )
    }

    /// Open Dactyl's isolated in-memory store with an explicit access mode.
    pub fn open_memory_with_access_mode(access_mode: AccessMode) -> Result<Self, DecapodError> {
        Self::open_route(
            dactyl_db::DatastoreRoute::sqlite(":memory:"),
            access_mode,
            None,
        )
    }

    /// Open a Dactyl-owned local snapshot with an explicit access mode.
    ///
    /// The canonical `.decapod/data/decapod.db` path is now a Dactyl snapshot
    /// after the explicit Dactyl-owned legacy import has completed. Opening a
    /// pre-cutover SQLite file fails closed; callers must import it before any
    /// canonical read or write.
    pub fn open_local_snapshot(
        path: impl AsRef<Path>,
        access_mode: AccessMode,
    ) -> Result<Self, DecapodError> {
        let path = path.as_ref();
        Self::open_route(
            dactyl_db::DatastoreRoute::sqlite(path.to_string_lossy().into_owned()),
            access_mode,
            None,
        )
    }

    /// Open the repository's canonical local datastore through Dactyl.
    ///
    /// This is the only supported local runtime entrypoint for
    /// `.decapod/data/decapod.db`. The path remains Decapod-owned policy, but
    /// physical opening and all subsequent operations belong to Dactyl.
    pub fn open_canonical(
        data_root: impl AsRef<Path>,
        access_mode: AccessMode,
    ) -> Result<Self, DecapodError> {
        Self::open_local_snapshot(data_root.as_ref().join(schemas::LOCAL_DB_NAME), access_mode)
    }

    /// Bind a governed backend route to Dactyl.
    ///
    /// A local route is always opened as a Dactyl snapshot. Cloud routes
    /// require a separate machine-local bearer credential and are passed
    /// through as opaque HTTP endpoints; this method never derives provider
    /// URLs or silently falls back to local storage.
    pub fn from_backend_route(
        route: &BackendRoute,
        access_mode: AccessMode,
        bearer: Option<&str>,
    ) -> Result<Self, DecapodError> {
        match route {
            BackendRoute::Local { path } => Self::open_local_snapshot(path, access_mode),
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
                    None,
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
        context.validate()?;
        let dactyl_context = dactyl_db::StorageContext::new(
            context.version(),
            serde_json::to_value(context).map_err(|error| {
                DecapodError::Config(format!("failed to encode storage context: {error}"))
            })?,
        )?;

        match context.route() {
            BackendRoute::Local { .. } => {
                Self::from_backend_route(context.route(), access_mode, context.bearer())
            }
            BackendRoute::Cloud { uri, .. } => Self::open_route(
                dactyl_db::DatastoreRoute::neon(uri.clone(), context.bearer().map(str::to_owned)),
                access_mode,
                Some(dactyl_context),
            ),
        }
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

    /// Inspect the backend-neutral schema exposed by Dactyl.
    pub fn inspect_schema(&self) -> Result<dactyl_db::StoreSchema, DecapodError> {
        Ok(self.connection.inspect_schema()?)
    }

    /// Return whether a caller-owned table is present without querying a
    /// backend-specific catalog such as `sqlite_master` or `PRAGMA`.
    pub fn has_table(&self, name: &str) -> Result<bool, DecapodError> {
        Ok(self.inspect_schema()?.table(name).is_some())
    }

    /// Convert an existing SQLite store into a Dactyl snapshot.
    ///
    /// This is intentionally opt-in and explicit. Dactyl performs the
    /// read-only inspection, temporary-output write, atomic replacement, and
    /// idempotency/divergence checks; Decapod retains ownership of when this
    /// migration is allowed by its schema and recovery policy.
    #[cfg(feature = "legacy-import")]
    pub fn import_legacy_sqlite(
        source: impl AsRef<Path>,
        destination: impl AsRef<Path>,
    ) -> Result<ImportReport, DecapodError> {
        Ok(dactyl_db::import_sqlite_file(
            source.as_ref(),
            destination.as_ref(),
        )?)
    }

    fn open_route(
        route: dactyl_db::DatastoreRoute,
        access_mode: AccessMode,
        context: Option<dactyl_db::StorageContext>,
    ) -> Result<Self, DecapodError> {
        let connection = Connection::open_with_options_and_context(
            route,
            OpenOptions {
                access_mode,
                lock_timeout: DEFAULT_LOCK_TIMEOUT,
            },
            context,
        )?;
        Ok(Self { connection })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::error::{DecapodError, StorageFailureKind};
    use crate::core::repo_identity::RepositoryIdentity;
    use std::io::Write;
    use tempfile::tempdir;

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
    fn local_snapshot_reopens_and_rejects_canonical_sqlite() {
        let tmp = tempdir().expect("temporary snapshot directory");
        let snapshot_path = tmp.path().join("decapod.snapshot");
        std::fs::File::create(&snapshot_path).expect("empty snapshot target");
        let bridge = DactylBridge::open_local_snapshot(&snapshot_path, AccessMode::ReadWrite)
            .expect("Dactyl snapshot bridge");
        bridge
            .atomic(&[Operation::schema(
                "CREATE TABLE tasks (id INTEGER PRIMARY KEY, title TEXT NOT NULL)",
                no_params(),
            )])
            .expect("snapshot schema");
        bridge
            .write(
                "INSERT INTO tasks (id, title) VALUES (?, ?)",
                &[101_i64.into(), "persisted".into()],
            )
            .expect("snapshot row");
        drop(bridge);

        let reopened = DactylBridge::open_local_snapshot(&snapshot_path, AccessMode::ReadOnly)
            .expect("reopen Dactyl snapshot");
        let rows = reopened
            .read("SELECT id, title FROM tasks", &no_params())
            .expect("read persisted snapshot");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows.as_slice()[0].get_int("id").expect("id"), 101);
        assert_eq!(
            rows.as_slice()[0].get_str("title").expect("title"),
            "persisted"
        );
        drop(reopened);

        let sqlite_path = tmp.path().join("canonical.db");
        let mut sqlite = std::fs::File::create(&sqlite_path).expect("canonical SQLite fixture");
        sqlite
            .write_all(b"SQLite format 3\0")
            .expect("write SQLite header");
        drop(sqlite);
        let error = match DactylBridge::open_local_snapshot(&sqlite_path, AccessMode::ReadWrite) {
            Err(error) => error,
            Ok(_) => panic!("canonical SQLite must not be opened as a Dactyl snapshot"),
        };
        assert_eq!(error.storage_failure_kind(), StorageFailureKind::Capability);
        assert!(error.to_string().contains("import into the Dactyl format"));
    }

    #[test]
    fn canonical_local_route_fails_closed_without_second_store() {
        let tmp = tempdir().expect("temporary canonical store");
        let sqlite_path = tmp.path().join("canonical.db");
        let mut sqlite = std::fs::File::create(&sqlite_path).expect("canonical SQLite fixture");
        sqlite
            .write_all(b"SQLite format 3\0")
            .expect("write SQLite header");
        drop(sqlite);
        let route = BackendRoute::Local { path: sqlite_path };
        match DactylBridge::from_backend_route(&route, AccessMode::ReadWrite, Some("ignored")) {
            Err(error) => assert_eq!(error.storage_failure_kind(), StorageFailureKind::Capability),
            Ok(_) => panic!("canonical SQLite route must not open a second store"),
        }
    }

    #[test]
    fn canonical_route_is_dactyl_owned_and_schema_inspection_is_portable() {
        let tmp = tempdir().expect("temporary canonical data root");
        let canonical_path = tmp.path().join("decapod.db");
        std::fs::File::create(&canonical_path).expect("canonical Dactyl snapshot target");

        let bridge = DactylBridge::open_canonical(tmp.path(), AccessMode::ReadWrite)
            .expect("canonical Dactyl route");
        bridge
            .atomic(&[Operation::schema(
                "CREATE TABLE tasks (id INTEGER PRIMARY KEY, title TEXT NOT NULL)",
                no_params(),
            )])
            .expect("canonical schema");

        assert!(
            bridge
                .has_table("tasks")
                .expect("portable schema inspection")
        );
        assert!(canonical_path.exists());
    }

    #[cfg(feature = "legacy-import")]
    #[test]
    fn legacy_sqlite_import_reopens_through_the_snapshot_route() {
        let tmp = tempdir().expect("temporary legacy store");
        let sqlite_path = tmp.path().join("decapod.db");
        let sqlite = rusqlite::Connection::open(&sqlite_path).expect("legacy SQLite store");
        sqlite
            .execute_batch(
                "CREATE TABLE tasks (id INTEGER PRIMARY KEY, title TEXT NOT NULL);\
                 INSERT INTO tasks (id, title) VALUES (101, 'migrated');",
            )
            .expect("seed legacy SQLite store");
        drop(sqlite);

        let _report = DactylBridge::import_legacy_sqlite(&sqlite_path, &sqlite_path)
            .expect("import legacy SQLite store");
        assert!(
            sqlite_path.with_extension("db.legacy-sqlite").exists(),
            "same-path import must retain the legacy source backup"
        );

        let bridge = DactylBridge::open_local_snapshot(&sqlite_path, AccessMode::ReadOnly)
            .expect("open converted Dactyl snapshot");
        let rows = bridge
            .read("SELECT id, title FROM tasks", &no_params())
            .expect("read converted task");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows.as_slice()[0].get_int("id").expect("id"), 101);
        assert_eq!(
            rows.as_slice()[0].get_str("title").expect("title"),
            "migrated"
        );
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
        let forwarded = bridge
            .connection
            .context()
            .expect("remote context is attached to the Dactyl connection");
        assert_eq!(forwarded.version(), context.version());
        assert_eq!(
            forwarded.payload(),
            &serde_json::to_value(&context).expect("context payload")
        );
        let encoded = serde_json::to_string(&context).expect("context JSON");
        assert!(!encoded.contains("opaque-session-token"));
        assert!(!encoded.contains("organization"));
    }

    #[test]
    fn unsupported_context_version_fails_before_dactyl_open() {
        let identity = RepositoryIdentity {
            canonical_name: "DecapodLabs/decapod".to_string(),
            owner: "DecapodLabs".to_string(),
            repository: "decapod".to_string(),
            remote_url: "git@github.com:DecapodLabs/decapod.git".to_string(),
        };
        let context = StorageContext::from_route(
            BackendRoute::Cloud {
                repository: identity,
                uri: "https://example.invalid/api/v1/store".to_string(),
            },
            Some("opaque-session-token"),
        )
        .expect("remote context");
        let mut encoded = serde_json::to_value(&context).expect("context JSON");
        encoded["version"] = serde_json::json!(StorageContext::CURRENT_VERSION + 1);
        let future: StorageContext = serde_json::from_value(encoded).expect("future context");

        assert!(matches!(
            DactylBridge::from_storage_context(&future, AccessMode::ReadOnly),
            Err(DecapodError::Config(message))
                if message.contains("unsupported storage context version")
        ));
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

        let unavailable = DecapodError::from(dactyl_db::DactylError::Adapter {
            kind: dactyl_db::AdapterErrorKind::Unavailable,
            code: Some("service_unavailable".to_string()),
            message: "temporarily unavailable".to_string(),
        });
        assert_eq!(unavailable.storage_failure_kind(), StorageFailureKind::Io);
        assert!(unavailable.storage_failure_kind().is_retryable());

        let authorization = DecapodError::from(dactyl_db::DactylError::Adapter {
            kind: dactyl_db::AdapterErrorKind::Authorization,
            code: Some("repository_not_authorized".to_string()),
            message: "not authorized".to_string(),
        });
        assert_eq!(
            authorization.storage_failure_kind(),
            StorageFailureKind::Unknown
        );
        assert!(!authorization.storage_failure_kind().is_retryable());
    }
}
