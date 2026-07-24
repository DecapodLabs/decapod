// Moved from src/decapod/core/container_runtime.rs
use super::*;

#[test]
fn missing_runtime_error_names_both_supported_runtimes() {
    let err =
        error::DecapodError::NotFound("No container runtime found (docker/podman)".to_string());
    assert!(err.to_string().contains("docker/podman"));
}

#[test]
fn image_retention_keeps_only_current_decapod_versions() {
    assert!(is_current_decapod_image(
        DECAPOD_RELEASE_IMAGE_REPOSITORY,
        "v0.72.9",
        "v0.72.9"
    ));
    assert!(is_current_decapod_image(
        DECAPOD_RELEASE_IMAGE_REPOSITORY,
        "v0.72.9-alpine",
        "v0.72.9"
    ));
    assert!(!is_current_decapod_image(
        DECAPOD_RELEASE_IMAGE_REPOSITORY,
        "latest",
        "v0.72.9"
    ));
    assert!(!is_current_decapod_image(
        DECAPOD_RELEASE_IMAGE_REPOSITORY,
        "v0.72.8",
        "v0.72.9"
    ));
    assert!(!is_current_decapod_image(
        "docker.io/library/alpine",
        "3.20",
        "v0.72.9"
    ));
}

#[test]
fn image_inventory_scopes_cleanup_to_decapod_artifacts() {
    let workspace_labels = BTreeMap::from([
        (
            DECAPOD_WORKSPACE_LABEL_KEY.to_string(),
            DECAPOD_WORKSPACE_LABEL_VALUE.to_string(),
        ),
        (DECAPOD_VERSION_LABEL_KEY.to_string(), "0.72.9".to_string()),
        (
            DECAPOD_WORKSPACE_PATH_LABEL_KEY.to_string(),
            "/tmp/workspace".to_string(),
        ),
    ]);
    let empty_labels = BTreeMap::new();

    assert!(is_decapod_managed_image(
        DECAPOD_RELEASE_IMAGE_REPOSITORY,
        &empty_labels
    ));
    assert!(is_decapod_managed_image(
        DECAPOD_WORKSPACE_IMAGE_REPOSITORY,
        &empty_labels
    ));
    assert!(!is_decapod_managed_image(
        "localhost/malware-analyzer",
        &empty_labels
    ));

    assert!(is_current_decapod_image_record(
        DECAPOD_RELEASE_IMAGE_REPOSITORY,
        "v0.72.9",
        &empty_labels,
        "0.72.9",
        "v0.72.9"
    ));
    assert!(is_current_decapod_image_record(
        DECAPOD_WORKSPACE_IMAGE_REPOSITORY,
        "agent-branch",
        &workspace_labels,
        "0.72.9",
        "v0.72.9"
    ));
    assert!(!is_current_decapod_image_record(
        DECAPOD_WORKSPACE_IMAGE_REPOSITORY,
        "v0.72.9-agent-branch",
        &workspace_labels,
        "0.72.9",
        "v0.72.9"
    ));

    let stale_workspace_labels = BTreeMap::from([
        (
            DECAPOD_WORKSPACE_LABEL_KEY.to_string(),
            DECAPOD_WORKSPACE_LABEL_VALUE.to_string(),
        ),
        (DECAPOD_VERSION_LABEL_KEY.to_string(), "0.72.8".to_string()),
        (
            DECAPOD_WORKSPACE_PATH_LABEL_KEY.to_string(),
            "/tmp/workspace".to_string(),
        ),
    ]);
    assert!(!is_current_decapod_image_record(
        DECAPOD_WORKSPACE_IMAGE_REPOSITORY,
        "agent-branch",
        &stale_workspace_labels,
        "0.72.9",
        "v0.72.9"
    ));
}
