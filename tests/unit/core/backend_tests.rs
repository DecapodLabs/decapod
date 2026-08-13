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
