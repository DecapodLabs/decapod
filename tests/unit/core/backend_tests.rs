use super::{BackendRoute, BackendSelection, LOCAL_DATASTORE_RELATIVE_PATH};
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
