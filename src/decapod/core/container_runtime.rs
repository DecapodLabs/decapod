use crate::core::bounded_process::{BoundedCommand, CONTROL_TIMEOUT};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::process::Command;

use crate::core::error;

pub const DECAPOD_RELEASE_IMAGE_REPOSITORY: &str = "ghcr.io/decapodlabs/decapod";
pub const DECAPOD_WORKSPACE_IMAGE_REPOSITORY: &str = "localhost/decapod-workspace";
const DECAPOD_SOURCE_LABEL: &str =
    "org.opencontainers.image.source=https://github.com/DecapodLabs/decapod";
const DECAPOD_WORKSPACE_LABEL: &str = "org.decapod.managed=workspace";
const DECAPOD_SOURCE_LABEL_KEY: &str = "org.opencontainers.image.source";
const DECAPOD_SOURCE_LABEL_VALUE: &str = "https://github.com/DecapodLabs/decapod";
const DECAPOD_WORKSPACE_LABEL_KEY: &str = "org.decapod.managed";
const DECAPOD_WORKSPACE_LABEL_VALUE: &str = "workspace";
const DECAPOD_VERSION_LABEL_KEY: &str = "org.decapod.version";
const DECAPOD_WORKSPACE_PATH_LABEL_KEY: &str = "org.decapod.workspace.path";

#[derive(Debug, Default, PartialEq, Eq)]
pub struct ImagePruneReport {
    pub removed_images: Vec<String>,
    pub pruned_layer_sets: usize,
}

pub fn container_runtime_available() -> bool {
    find_container_runtime().is_ok()
}

pub fn find_container_runtime() -> Result<String, error::DecapodError> {
    if command_present("podman") {
        return Ok("podman".to_string());
    }
    if command_present("docker") {
        return Ok("docker".to_string());
    }
    Err(error::DecapodError::NotFound(
        "No container runtime found (docker/podman)".to_string(),
    ))
}

pub fn current_decapod_version_tag() -> String {
    format!("v{}-debian", env!("CARGO_PKG_VERSION"))
}

pub fn is_current_decapod_image(repository: &str, tag: &str, version_tag: &str) -> bool {
    match repository {
        DECAPOD_RELEASE_IMAGE_REPOSITORY => {
            tag == version_tag || tag == version_tag.replace("-debian", "-alpine")
        }
        _ => false,
    }
}

#[derive(Debug)]
struct ImageRecord {
    repository: String,
    tag: String,
    image_id: String,
    labels: BTreeMap<String, String>,
}

fn is_decapod_image_repository(repository: &str) -> bool {
    repository == DECAPOD_RELEASE_IMAGE_REPOSITORY
        || repository == DECAPOD_WORKSPACE_IMAGE_REPOSITORY
        || repository.starts_with("decapod-local-")
}

fn is_decapod_managed_image(repository: &str, labels: &BTreeMap<String, String>) -> bool {
    is_decapod_image_repository(repository)
        || (labels.get(DECAPOD_SOURCE_LABEL_KEY).map(String::as_str)
            == Some(DECAPOD_SOURCE_LABEL_VALUE))
        || (labels.get(DECAPOD_WORKSPACE_LABEL_KEY).map(String::as_str)
            == Some(DECAPOD_WORKSPACE_LABEL_VALUE))
}

fn is_current_decapod_image_record(
    repository: &str,
    tag: &str,
    labels: &BTreeMap<String, String>,
    version: &str,
    version_tag: &str,
) -> bool {
    if repository == DECAPOD_RELEASE_IMAGE_REPOSITORY {
        return is_current_decapod_image(repository, tag, version_tag);
    }

    is_decapod_image_repository(repository)
        && repository != DECAPOD_RELEASE_IMAGE_REPOSITORY
        && tag != "<none>"
        && !tag.starts_with(&format!("{version_tag}-"))
        && labels.get(DECAPOD_WORKSPACE_LABEL_KEY).map(String::as_str)
            == Some(DECAPOD_WORKSPACE_LABEL_VALUE)
        && labels.get(DECAPOD_VERSION_LABEL_KEY).map(String::as_str) == Some(version)
        && labels
            .get(DECAPOD_WORKSPACE_PATH_LABEL_KEY)
            .is_some_and(|path| !path.is_empty())
}

fn list_decapod_images(runtime: &str) -> Result<Vec<ImageRecord>, error::DecapodError> {
    let listed = Command::new(runtime)
        .args([
            "image",
            "ls",
            "--all",
            "--format",
            "{{.Repository}}|{{.Tag}}|{{.ID}}",
        ])
        .bounded_output(CONTROL_TIMEOUT)
        .map_err(error::DecapodError::IoError)?;
    if !listed.status.success() {
        return Err(error::DecapodError::ValidationError(format!(
            "Failed to list container images: {}",
            String::from_utf8_lossy(&listed.stderr).trim()
        )));
    }

    let mut image_ids = BTreeSet::new();
    let mut entries = Vec::new();
    for line in String::from_utf8_lossy(&listed.stdout).lines() {
        let mut fields = line.split('|');
        let Some(repository) = fields.next() else {
            continue;
        };
        let Some(tag) = fields.next() else {
            continue;
        };
        let Some(image_id) = fields.next() else {
            continue;
        };
        if repository.is_empty() || image_id.is_empty() {
            continue;
        }
        image_ids.insert(image_id.to_string());
        entries.push((
            repository.to_string(),
            tag.to_string(),
            image_id.to_string(),
        ));
    }

    let mut labels_by_id = BTreeMap::new();
    for image_id in image_ids {
        let inspected = Command::new(runtime)
            .args([
                "image",
                "inspect",
                "--format",
                "{{json .Config.Labels}}",
                &image_id,
            ])
            .bounded_output(CONTROL_TIMEOUT)
            .map_err(error::DecapodError::IoError)?;
        if !inspected.status.success() {
            return Err(error::DecapodError::ValidationError(format!(
                "Failed to inspect container image {image_id}: {}",
                String::from_utf8_lossy(&inspected.stderr).trim()
            )));
        }

        let parsed: serde_json::Value = serde_json::from_slice(&inspected.stdout).map_err(|e| {
            error::DecapodError::ValidationError(format!(
                "Failed to parse labels for container image {image_id}: {e}"
            ))
        })?;
        let labels = parsed
            .as_object()
            .map(|object| {
                object
                    .iter()
                    .filter_map(|(key, value)| {
                        value.as_str().map(|value| (key.clone(), value.to_string()))
                    })
                    .collect()
            })
            .unwrap_or_default();
        labels_by_id.insert(image_id, labels);
    }

    Ok(entries
        .into_iter()
        .map(|(repository, tag, image_id)| ImageRecord {
            repository,
            tag,
            labels: labels_by_id.remove(&image_id).unwrap_or_default(),
            image_id,
        })
        .collect())
}

/// Remove Decapod-owned image tags whose version does not match this binary,
/// then reclaim their unreferenced layers without touching unrelated images.
pub fn prune_decapod_images() -> Result<ImagePruneReport, error::DecapodError> {
    let runtime = find_container_runtime()?;
    let version = env!("CARGO_PKG_VERSION");
    let version_tag = current_decapod_version_tag();
    let records = list_decapod_images(&runtime)?;
    let current_image_ids: BTreeSet<String> = records
        .iter()
        .filter(|record| {
            is_current_decapod_image_record(
                &record.repository,
                &record.tag,
                &record.labels,
                version,
                &version_tag,
            )
        })
        .map(|record| record.image_id.clone())
        .collect();
    let mut report = ImagePruneReport::default();
    let mut removal_targets = BTreeSet::new();
    for record in records {
        if !is_decapod_managed_image(&record.repository, &record.labels)
            || is_current_decapod_image_record(
                &record.repository,
                &record.tag,
                &record.labels,
                version,
                &version_tag,
            )
        {
            continue;
        }

        if record.tag == "<none>" {
            if !current_image_ids.contains(&record.image_id) {
                removal_targets.insert(record.image_id);
            }
        } else {
            removal_targets.insert(format!("{}:{}", record.repository, record.tag));
        }
    }

    for image_ref in removal_targets {
        remove_image_with_runtime(&runtime, &image_ref)?;
        report.removed_images.push(image_ref);
    }

    for label in [DECAPOD_SOURCE_LABEL, DECAPOD_WORKSPACE_LABEL] {
        let pruned = Command::new(&runtime)
            .args([
                "image",
                "prune",
                "--force",
                "--filter",
                &format!("label={label}"),
            ])
            .bounded_output(CONTROL_TIMEOUT)
            .map_err(error::DecapodError::IoError)?;
        if !pruned.status.success() {
            return Err(error::DecapodError::ValidationError(format!(
                "Failed to prune stale Decapod image layers for {label}: {}",
                String::from_utf8_lossy(&pruned.stderr).trim()
            )));
        }
        report.pruned_layer_sets += 1;
    }

    Ok(report)
}

fn remove_image_with_runtime(runtime: &str, image_ref: &str) -> Result<bool, error::DecapodError> {
    let removed = Command::new(runtime)
        .args(["image", "rm", "--force", image_ref])
        .bounded_output(CONTROL_TIMEOUT)
        .map_err(error::DecapodError::IoError)?;
    if removed.status.success() {
        return Ok(true);
    }

    let stderr = String::from_utf8_lossy(&removed.stderr).to_ascii_lowercase();
    if stderr.contains("no such image")
        || stderr.contains("does not exist")
        || stderr.contains("not found")
    {
        return Ok(false);
    }

    Err(error::DecapodError::ValidationError(format!(
        "Failed to remove Decapod image {image_ref}: {}",
        String::from_utf8_lossy(&removed.stderr).trim()
    )))
}

pub fn remove_image(image_ref: &str) -> Result<bool, error::DecapodError> {
    let runtime = find_container_runtime()?;
    remove_image_with_runtime(&runtime, image_ref)
}
pub fn remove_workspace_images_for_path(
    workspace_path: &Path,
) -> Result<usize, error::DecapodError> {
    let runtime = find_container_runtime()?;
    let label = format!(
        "label=org.decapod.workspace.path={}",
        workspace_path.display()
    );
    let listed = Command::new(&runtime)
        .args(["image", "ls", "--quiet", "--filter", &label])
        .bounded_output(CONTROL_TIMEOUT)
        .map_err(error::DecapodError::IoError)?;
    if !listed.status.success() {
        return Err(error::DecapodError::ValidationError(format!(
            "Failed to list workspace images for {}: {}",
            workspace_path.display(),
            String::from_utf8_lossy(&listed.stderr).trim()
        )));
    }

    let mut removed = 0;
    for image_id in String::from_utf8_lossy(&listed.stdout)
        .lines()
        .map(str::trim)
        .filter(|id| !id.is_empty())
    {
        if remove_image(image_id)? {
            removed += 1;
        }
    }
    Ok(removed)
}

fn command_present(cmd: &str) -> bool {
    let Some(path) = std::env::var_os("PATH") else {
        return false;
    };
    std::env::split_paths(&path).any(|dir| executable_exists(&dir.join(cmd)))
}

fn executable_exists(path: &Path) -> bool {
    let Ok(metadata) = path.metadata() else {
        return false;
    };
    if !metadata.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        metadata.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        true
    }
}
#[cfg(test)]
#[path = "../../../tests/unit/core/container_runtime_tests.rs"]
mod tests;

/// Remove an invocation-owned container; an already removed container is success.
pub fn remove_container_with_runtime(
    runtime: &str,
    name: &str,
    workspace: &Path,
) -> Result<(), error::DecapodError> {
    remove_container_for_invocation(runtime, name, workspace, None)
}

pub fn remove_container_for_invocation(
    runtime: &str,
    name: &str,
    workspace: &Path,
    invocation: Option<&str>,
) -> Result<(), error::DecapodError> {
    let inspected = Command::new(runtime)
        .args(["container", "inspect", name, "--format", "{{json .}}"])
        .bounded_output(CONTROL_TIMEOUT)
        .map_err(error::DecapodError::IoError)?;
    if !inspected.status.success() {
        if String::from_utf8_lossy(&inspected.stderr)
            .to_ascii_lowercase()
            .contains("no such container")
        {
            return Ok(());
        }
        return Err(error::DecapodError::ValidationError(format!(
            "Container ownership inspection failed: {}",
            String::from_utf8_lossy(&inspected.stderr).trim()
        )));
    }
    let record: serde_json::Value = serde_json::from_slice(&inspected.stdout).map_err(|err| {
        error::DecapodError::ValidationError(format!("Invalid container ownership response: {err}"))
    })?;
    let id = record
        .get("Id")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("");
    let labels = &record["Config"]["Labels"];
    if id.len() != 64
        || !id.bytes().all(|c| c.is_ascii_hexdigit())
        || labels["org.decapod.managed"].as_str() != Some("workspace")
        || labels["org.decapod.workspace.path"].as_str() != workspace.to_str()
        || invocation
            .is_some_and(|expected| labels["org.decapod.invocation"].as_str() != Some(expected))
    {
        return Err(error::DecapodError::ValidationError(
            "Container ownership mismatch; refusing cleanup".into(),
        ));
    }
    // An immutable full ID cannot be rebound to an unrelated replacement name.
    let output = Command::new(runtime)
        .args(["container", "rm", "--force", id])
        .bounded_output(CONTROL_TIMEOUT)
        .map_err(error::DecapodError::IoError)?;
    let stderr = String::from_utf8_lossy(&output.stderr).to_ascii_lowercase();
    if output.status.success() || stderr.contains("no such container") {
        Ok(())
    } else {
        Err(error::DecapodError::ValidationError(format!(
            "Container cleanup failed for {name}: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )))
    }
}

/// Recover interrupted, labelled runs before deleting their workspace data.
pub fn remove_workspace_containers_for_path(
    runtime: &str,
    workspace: &Path,
) -> Result<(), error::DecapodError> {
    remove_workspace_containers_for_invocation(runtime, workspace, None)
}

pub fn remove_workspace_containers_for_invocation(
    runtime: &str,
    workspace: &Path,
    invocation: Option<&str>,
) -> Result<(), error::DecapodError> {
    let output = Command::new(runtime)
        .args([
            "container",
            "ls",
            "--all",
            "--quiet",
            "--filter",
            "label=org.decapod.managed=workspace",
            "--filter",
            &format!("label=org.decapod.workspace.path={}", workspace.display()),
        ])
        .bounded_output(CONTROL_TIMEOUT)
        .map_err(error::DecapodError::IoError)?;
    if !output.status.success() {
        return Err(error::DecapodError::ValidationError(format!(
            "Could not inspect workspace containers: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    for id in String::from_utf8_lossy(&output.stdout).split_whitespace() {
        if !id.bytes().all(|c| c.is_ascii_hexdigit()) {
            return Err(error::DecapodError::ValidationError(
                "Runtime returned invalid container ID".into(),
            ));
        }
        remove_container_for_invocation(runtime, id, workspace, invocation)?;
    }
    Ok(())
}
