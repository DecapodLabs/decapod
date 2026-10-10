use super::{BackendRoute, BackendSelection, LOCAL_DATASTORE_RELATIVE_PATH, StorageContext};
use crate::cli::BackendType;
use tempfile::TempDir;

#[test]
fn local_selection_binds_the_canonical_repository_database_without_git() {
    let project = TempDir::new().expect("project directory");
    let selection =
        BackendSelection::resolve(project.path(), BackendType::Local).expect("local selection");
    let route = selection.route(None).expect("local route");

    assert_eq!(selection.backend(), BackendType::Local);
    assert!(selection.repository_identity().is_none());
    assert_eq!(
        route.local_path().expect("local path"),
        project.path().join(LOCAL_DATASTORE_RELATIVE_PATH)
    );
    assert!(route.cloud_uri().is_none());
}

#[test]
fn cloud_selection_binds_git_owner_and_repository_to_an_opaque_route() {
    let project = TempDir::new().expect("project directory");
    std::process::Command::new("git")
        .args(["init", "-q"])
        .current_dir(project.path())
        .status()
        .expect("git init");
    std::process::Command::new("git")
        .args([
            "remote",
            "add",
            "origin",
            "git@github.com:DecapodLabs/decapod.git",
        ])
        .current_dir(project.path())
        .status()
        .expect("git remote");

    let selection =
        BackendSelection::resolve(project.path(), BackendType::Cloud).expect("cloud selection");
    let route = selection
        .route(Some("https://datastore.example.test/DecapodLabs/decapod"))
        .expect("cloud route");

    assert_eq!(selection.backend(), BackendType::Cloud);
    assert_eq!(
        selection
            .repository_identity()
            .expect("repository identity")
            .canonical_name,
        "DecapodLabs/decapod"
    );
    assert_eq!(
        route.repository_identity().expect("route identity").owner,
        "DecapodLabs"
    );
    assert_eq!(
        route.cloud_uri(),
        Some("https://datastore.example.test/DecapodLabs/decapod")
    );
    assert!(route.local_path().is_none());
}

#[test]
fn cloud_route_rejects_missing_or_credential_bearing_uri() {
    let project = TempDir::new().expect("project directory");
    std::process::Command::new("git")
        .args(["init", "-q"])
        .current_dir(project.path())
        .status()
        .expect("git init");
    std::process::Command::new("git")
        .args([
            "remote",
            "add",
            "origin",
            "https://github.com/example/project.git",
        ])
        .current_dir(project.path())
        .status()
        .expect("git remote");
    let selection =
        BackendSelection::resolve(project.path(), BackendType::Cloud).expect("cloud selection");

    assert!(selection.route(None).is_err());
    assert!(
        selection
            .route(Some("https://user:secret@datastore.example.test/route"))
            .is_err()
    );
    assert!(selection.route(Some("datastore://example/route")).is_err());
}

#[test]
fn local_route_rejects_a_remote_uri() {
    let project = TempDir::new().expect("project directory");
    let selection =
        BackendSelection::resolve(project.path(), BackendType::Local).expect("local selection");
    assert!(matches!(
        selection.route(Some("https://datastore.example.test/route")),
        Err(crate::core::error::DecapodError::Config(_))
    ));
}

#[test]
fn local_context_has_no_cloud_scope_or_credential() {
    let project = TempDir::new().expect("project directory");
    let selection =
        BackendSelection::resolve(project.path(), BackendType::Local).expect("local selection");
    let context = selection
        .storage_context(None, None)
        .expect("local context");

    assert_eq!(context.version(), StorageContext::CURRENT_VERSION);
    assert!(context.is_local());
    assert!(!context.is_remote());
    assert_eq!(context.bearer(), None);
    let encoded = serde_json::to_string(&context).expect("context JSON");
    assert!(!encoded.contains("organization"));
    assert!(!encoded.contains("repository"));
    assert!(!encoded.contains("bearer"));
}

#[test]
fn remote_context_requires_opaque_auth_and_excludes_it_from_serialization() {
    let project = TempDir::new().expect("project directory");
    std::process::Command::new("git")
        .args(["init", "-q"])
        .current_dir(project.path())
        .status()
        .expect("git init");
    std::process::Command::new("git")
        .args([
            "remote",
            "add",
            "origin",
            "git@github.com:DecapodLabs/decapod.git",
        ])
        .current_dir(project.path())
        .status()
        .expect("git remote");
    let selection =
        BackendSelection::resolve(project.path(), BackendType::Cloud).expect("cloud selection");

    assert!(matches!(
        selection.storage_context(Some("https://datastore.example.test/route"), None),
        Err(crate::core::error::DecapodError::CloudAuth(_))
    ));
    let context = selection
        .storage_context(
            Some("https://datastore.example.test/route"),
            Some("opaque-session-token"),
        )
        .expect("remote context");
    assert!(context.is_remote());
    assert_eq!(context.bearer(), Some("opaque-session-token"));
    assert_eq!(
        context
            .route()
            .repository_identity()
            .expect("repository scope")
            .canonical_name,
        "DecapodLabs/decapod"
    );
    let encoded = serde_json::to_string(&context).expect("context JSON");
    assert!(!encoded.contains("opaque-session-token"));
}

#[test]
fn local_context_rejects_cloud_credentials() {
    let project = TempDir::new().expect("project directory");
    let selection =
        BackendSelection::resolve(project.path(), BackendType::Local).expect("local selection");
    assert!(matches!(
        selection.storage_context(None, Some("unexpected-token")),
        Err(crate::core::error::DecapodError::Config(_))
    ));
}

#[test]
fn future_context_versions_fail_closed_before_driver_use() {
    let project = TempDir::new().expect("project directory");
    let selection =
        BackendSelection::resolve(project.path(), BackendType::Local).expect("local selection");
    let context = selection
        .storage_context(None, None)
        .expect("local context");
    let mut encoded = serde_json::to_value(&context).expect("context JSON");
    encoded["version"] = serde_json::json!(2);
    let future: StorageContext = serde_json::from_value(encoded).expect("future context");

    assert!(matches!(
        future.validate(),
        Err(crate::core::error::DecapodError::Config(message))
            if message.contains("unsupported storage context version")
    ));
}

#[test]
fn route_serialization_preserves_only_the_opaque_target_and_scope() {
    let route = BackendRoute::Cloud {
        repository: crate::core::repo_identity::resolve_repository_identity_from_remote(
            "git@github.com:example/project.git",
        )
        .expect("identity"),
        uri: "https://datastore.example.test/example/project".to_string(),
    };
    let encoded = serde_json::to_string(&route).expect("route JSON");
    assert!(encoded.contains("example/project"));
    assert!(!encoded.contains("neon"));
    assert!(!encoded.contains("propodus"));
}

#[test]
fn cloud_context_debug_redacts_endpoint_and_credentials() {
    let identity = crate::core::repo_identity::resolve_repository_identity_from_remote(
        "git@github.com:example/project.git",
    )
    .unwrap();
    let route = BackendRoute::cloud(identity, "https://service.example.test/private-path").unwrap();
    let context = StorageContext::from_route(route, Some("synthetic-bearer-secret")).unwrap();
    let debug = format!("{context:?}");
    assert!(!debug.contains("synthetic-bearer-secret"));
    assert!(!debug.contains("private-path"));
    assert!(!debug.contains("service.example.test"));
    assert!(debug.contains("REDACTED"));
    let encoded = serde_json::to_value(&context).unwrap();
    assert!(encoded.get("bearer").is_none());
    assert!(encoded.get("cloud_datastore").is_none());
    let restored: StorageContext = serde_json::from_value(encoded).unwrap();
    assert!(
        restored.validate().is_err(),
        "deserialization must not restore authentication"
    );
}

#[test]
fn remote_routes_reject_credential_query_and_fragment_without_echoing_them() {
    let identity = crate::core::repo_identity::resolve_repository_identity_from_remote(
        "git@github.com:example/project.git",
    )
    .unwrap();
    for endpoint in [
        "https://service.example.test?token=synthetic-secret",
        "https://service.example.test/#synthetic-secret",
        "postgres://user:synthetic-secret@db.example.test/db",
        "https://user:synthetic-secret@service.example.test",
    ] {
        let error = BackendRoute::cloud(identity.clone(), endpoint).unwrap_err();
        assert!(!error.to_string().contains("synthetic-secret"));
    }
}

#[test]
fn cloud_datastore_selection_preserves_neon_and_rejects_unknown_values() {
    use super::CloudDatastore;
    assert_eq!(CloudDatastore::default(), CloudDatastore::Neon);
    assert_eq!(
        CloudDatastore::parse("supabase").unwrap(),
        CloudDatastore::Supabase
    );
    for value in [
        "",
        "postgres",
        "sqlite",
        "supabase-postgres",
        "supabase?token=secret",
    ] {
        let error = CloudDatastore::parse(value).unwrap_err();
        assert!(!error.to_string().contains("token=secret"));
    }
    assert!(CloudDatastore::Neon.validate_available().is_ok());
    assert_eq!(
        CloudDatastore::Supabase.validate_available().is_ok(),
        cfg!(feature = "supabase-cloud")
    );
}

#[test]
fn supabase_requires_explicit_service_endpoint_and_never_uses_neon_default() {
    let config = crate::cli::CloudRuntimeConfig {
        provider: "vercel".to_string(),
        api_url: crate::cli::PROPODUS_VERCEL_NEON_ENTRYPOINT.to_string(),
        datastore: "supabase".to_string(),
    };
    assert!(config.validate_datastore().is_err());
    let config = crate::cli::CloudRuntimeConfig {
        datastore: "neon".to_string(),
        ..config
    };
    assert!(config.validate_datastore().is_ok());
}

#[cfg(feature = "supabase-cloud")]
#[test]
fn supabase_validates_transport_before_onboarding() {
    for endpoint in [
        "http://remote.example.test",
        "postgres://user:secret@host/db",
        "https://service.example.test/?token=secret",
    ] {
        let config = crate::cli::CloudRuntimeConfig {
            provider: "vercel".to_string(),
            api_url: endpoint.to_string(),
            datastore: "supabase".to_string(),
        };
        let error = config.validate_datastore().unwrap_err();
        assert!(!error.to_string().contains("secret"));
    }
    for endpoint in [
        "https://service.example.test",
        "http://127.0.0.1:9000",
        "http://[::1]:9000",
    ] {
        let config = crate::cli::CloudRuntimeConfig {
            provider: "vercel".to_string(),
            api_url: endpoint.to_string(),
            datastore: "supabase".to_string(),
        };
        assert!(config.validate_datastore().is_ok());
    }
}
