// Moved from src/decapod/core/dactyl.rs
use super::*;
use crate::core::error::{DecapodError, StorageFailureKind};
use crate::core::repo_identity::RepositoryIdentity;
use tempfile::tempdir;

fn no_params() -> Vec<Parameter> {
    Vec::new()
}

fn local_runtime_available() -> bool {
    match DactylBridge::open_memory() {
        Ok(_) => true,
        Err(DecapodError::DactylError(error))
            if error.adapter_code() == Some("sqlite_runtime_unavailable") =>
        {
            false
        }
        Err(error) => panic!("unexpected local Dactyl open failure: {error}"),
    }
}

#[test]
fn missing_local_runtime_is_reported_as_typed_storage_failure() {
    if let Err(error) = DactylBridge::open_memory() {
        assert_eq!(error.storage_failure_kind(), StorageFailureKind::Io);
        assert!(matches!(
            error,
            DecapodError::DactylError(error)
                if error.adapter_code() == Some("sqlite_runtime_unavailable")
        ));
    }
}

#[test]
fn missing_runtime_message_is_concise_and_agent_actionable() {
    let message = sqlite_runtime_required_error().to_string();
    assert!(message.starts_with(
        "Validation error: AUTOREMEDIABLE_VALIDATION_ERROR code=LOCAL_SQLITE_RUNTIME_REQUIRED"
    ));
    assert!(message.contains("auto_remediable=true"));
    assert!(message.contains("audience=agent"));
    assert!(message.contains("agent_action=\"Install the OS SQLite runtime"));
    assert!(message.contains("backend=local"));
    assert!(message.contains("DACTYL_SQLITE_LIBRARY"));
    assert!(message.contains("export DACTYL_SQLITE_LIBRARY="));
    assert!(message.contains(".config/decapod/runtime.toml"));
    assert!(message.contains("Cloud backend does not require SQLite"));
    assert!(!message.contains("storage open failed at stage="));
}

#[test]
fn host_runtime_config_is_machine_local_and_serializable() {
    let config = HostRuntimeConfig {
        schema_version: default_runtime_schema_version(),
        sqlite_library: Some("/opt/sqlite/lib/libsqlite3.so".to_string()),
    };
    let encoded = toml::to_string_pretty(&config).expect("host runtime config encoding");
    let decoded: HostRuntimeConfig = toml::from_str(&encoded).expect("host runtime config decode");
    assert_eq!(decoded.schema_version, "1");
    assert_eq!(
        decoded.sqlite_library.as_deref(),
        Some("/opt/sqlite/lib/libsqlite3.so")
    );
}

#[test]
fn sqlite_library_discovery_accepts_supported_host_names_only() {
    assert!(is_sqlite_library_name("libsqlite3.so"));
    assert!(is_sqlite_library_name("libsqlite3.so.0"));
    assert!(is_sqlite_library_name("libsqlite3.dylib"));
    assert!(is_sqlite_library_name("sqlite3.dll"));
    assert!(!is_sqlite_library_name("libsqlite.so"));
    assert!(!is_sqlite_library_name("sqlite3"));
}

#[test]
fn explicit_ids_and_atomic_rollback_are_backend_neutral() {
    if !local_runtime_available() {
        eprintln!("skipping local Dactyl conformance: host SQLite runtime unavailable");
        return;
    }
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
    if !local_runtime_available() {
        eprintln!("skipping local Dactyl conformance: host SQLite runtime unavailable");
        return;
    }
    let bridge = DactylBridge::open_memory_with_access_mode(AccessMode::ReadOnly)
        .expect("read-only memory bridge");
    let error = bridge
        .write("INSERT INTO missing (id) VALUES (?)", &[1_i64.into()])
        .expect_err("read-only route must reject writes");
    assert_eq!(error.storage_failure_kind(), StorageFailureKind::Capability);
}

#[test]
fn local_route_reopens_existing_sqlite() {
    if !local_runtime_available() {
        eprintln!("skipping local Dactyl conformance: host SQLite runtime unavailable");
        return;
    }
    let tmp = tempdir().expect("temporary local database directory");
    let database_path = tmp.path().join("decapod.db");
    std::fs::File::create(&database_path).expect("empty local database target");
    let bridge = DactylBridge::open_local(&database_path, AccessMode::ReadWrite)
        .expect("Dactyl local database bridge");
    bridge
        .atomic(&[Operation::schema(
            "CREATE TABLE tasks (id INTEGER PRIMARY KEY, title TEXT NOT NULL)",
            no_params(),
        )])
        .expect("local schema");
    bridge
        .write(
            "INSERT INTO tasks (id, title) VALUES (?, ?)",
            &[101_i64.into(), "persisted".into()],
        )
        .expect("local row");
    drop(bridge);

    let reopened = DactylBridge::open_local(&database_path, AccessMode::ReadOnly)
        .expect("reopen Dactyl local database");
    let rows = reopened
        .read("SELECT id, title FROM tasks", &no_params())
        .expect("read persisted local database");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows.as_slice()[0].get_int("id").expect("id"), 101);
    assert_eq!(
        rows.as_slice()[0].get_str("title").expect("title"),
        "persisted"
    );
    drop(reopened);
}

#[test]
fn canonical_route_is_dactyl_owned_and_schema_inspection_is_portable() {
    if !local_runtime_available() {
        eprintln!("skipping local Dactyl conformance: host SQLite runtime unavailable");
        return;
    }
    let tmp = tempdir().expect("temporary canonical data root");
    let canonical_path = tmp.path().join("decapod.db");
    std::fs::File::create(&canonical_path).expect("canonical Dactyl database target");

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
    let context =
        StorageContext::from_route(route, Some("opaque-session-token")).expect("remote context");
    let bridge = DactylBridge::from_storage_context(&context, AccessMode::ReadOnly)
        .expect("constructing cloud route does not perform I/O");
    assert_eq!(context.version(), StorageContext::CURRENT_VERSION);
    assert_eq!(context.bearer(), Some("opaque-session-token"));
    assert!(bridge.access_mode() == AccessMode::ReadOnly);
    assert_eq!(bridge.connection.datastore(), dactyl_db::Datastore::Neon);
    assert_eq!(
        bridge.connection.route().route(),
        "https://example.invalid/api/v1/store"
    );
    assert_eq!(
        bridge.connection.route().token(),
        Some("opaque-session-token")
    );
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
