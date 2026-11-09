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
        "v0.72.9-debian",
        "v0.72.9-debian"
    ));
    assert!(is_current_decapod_image(
        DECAPOD_RELEASE_IMAGE_REPOSITORY,
        "v0.72.9-alpine",
        "v0.72.9-debian"
    ));
    assert!(!is_current_decapod_image(
        DECAPOD_RELEASE_IMAGE_REPOSITORY,
        "latest",
        "v0.72.9"
    ));
    assert!(!is_current_decapod_image(
        DECAPOD_RELEASE_IMAGE_REPOSITORY,
        "v0.72.8-debian",
        "v0.72.9-debian"
    ));
    assert!(!is_current_decapod_image(
        "docker.io/library/alpine",
        "3.20",
        "v0.72.9-debian"
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

#[cfg(unix)]
fn lifecycle_runtime(temp: &tempfile::TempDir, mode: &str) -> String {
    use std::os::unix::fs::PermissionsExt;
    let workspace = temp.path().join("workspace");
    let id = "a".repeat(64);
    let owned_path = if mode == "foreign" {
        "/unrelated".into()
    } else {
        workspace.to_string_lossy().into_owned()
    };
    let record = serde_json::json!({"Id": id, "Config": {"Labels": {
        "org.decapod.managed": "workspace", "org.decapod.workspace.path": owned_path
    }}})
    .to_string();
    let script = temp.path().join("runtime");
    let marker = temp.path().join("removed");
    std::fs::write(
        &script,
        format!(
            r#"#!/bin/sh
if [ "$2" = "inspect" ]; then
  if [ -f '{marker}' ] || [ '{mode}' = 'missing' ]; then echo 'No such container' >&2; exit 1; fi
  if [ '{mode}' = 'denied' ]; then echo 'permission denied' >&2; exit 1; fi
  printf '%s\n' '{record}'
elif [ "$2" = "ls" ]; then
  printf '%s\n' '{id}'
elif [ "$2" = "rm" ]; then
  [ "$4" = '{id}' ] || exit 9
  touch '{marker}'
else exit 8
fi
"#,
            marker = marker.display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o700)).unwrap();
    await_lifecycle_fixture(&script);
    script.to_string_lossy().into_owned()
}

#[cfg(unix)]
fn await_lifecycle_fixture(script: &std::path::Path) {
    // A concurrent fork can briefly inherit the fixture's write descriptor
    // before exec closes it. Linux refuses to execute that inode with ETXTBSY
    // until every writer closes. Probe the side-effect-free listing once;
    // successful exec proves there are no remaining writers of this fixture.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    loop {
        match Command::new(script).args(["container", "ls"]).output() {
            Ok(output) => {
                assert!(
                    output.status.success(),
                    "fixture listing failed: {output:?}"
                );
                return;
            }
            Err(error)
                if error.raw_os_error() == Some(libc::ETXTBSY)
                    && std::time::Instant::now() < deadline =>
            {
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            Err(error) => panic!("fixture runtime could not execute: {error}"),
        }
    }
}

#[cfg(target_os = "linux")]
#[test]
fn lifecycle_fixture_waits_for_an_inherited_writer_to_close() {
    use std::os::unix::fs::PermissionsExt;
    let temp = tempfile::tempdir().unwrap();
    let script = temp.path().join("runtime");
    std::fs::write(&script, "#!/bin/sh\nexit 0\n").unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o700)).unwrap();
    let writer = std::fs::OpenOptions::new()
        .write(true)
        .open(&script)
        .unwrap();
    // Establish the actual kernel refusal, not a simulated error classifier.
    assert_eq!(
        Command::new(&script).output().unwrap_err().raw_os_error(),
        Some(libc::ETXTBSY)
    );
    let release = std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(30));
        drop(writer);
    });
    await_lifecycle_fixture(&script);
    release.join().unwrap();
}

#[cfg(unix)]
#[test]
fn lifecycle_cleanup_is_owned_and_idempotent() {
    let temp = tempfile::tempdir().unwrap();
    let runtime = lifecycle_runtime(&temp, "owned");
    let workspace = temp.path().join("workspace");
    remove_workspace_containers_for_path(&runtime, &workspace).unwrap();
    assert!(temp.path().join("removed").exists());
    remove_workspace_containers_for_path(&runtime, &workspace).unwrap();
}

#[cfg(unix)]
#[test]
fn lifecycle_cleanup_refuses_foreign_container_and_runtime_failure() {
    for mode in ["foreign", "denied"] {
        let temp = tempfile::tempdir().unwrap();
        let runtime = lifecycle_runtime(&temp, mode);
        assert!(
            remove_container_with_runtime(&runtime, "reused-name", &temp.path().join("workspace"))
                .is_err()
        );
        assert!(!temp.path().join("removed").exists());
    }
}

#[cfg(unix)]
#[test]
fn lifecycle_already_gone_container_is_success() {
    let temp = tempfile::tempdir().unwrap();
    let runtime = lifecycle_runtime(&temp, "missing");
    remove_container_with_runtime(&runtime, "gone", &temp.path().join("workspace")).unwrap();
    assert!(!temp.path().join("removed").exists());
}

#[cfg(unix)]
#[test]
fn runtime_discovery_rejects_nonexecutable_files_without_spawning() {
    use std::os::unix::fs::PermissionsExt;
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("docker");
    std::fs::write(&path, "#!/bin/sh\nsleep 30\n").unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
    assert!(!executable_exists(&path));
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
    let start = std::time::Instant::now();
    assert!(executable_exists(&path));
    assert!(start.elapsed() < std::time::Duration::from_secs(1));
}
