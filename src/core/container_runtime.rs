use std::path::Path;
use std::process::{Command, Stdio};

use crate::core::error;

pub const DECAPOD_RELEASE_IMAGE_REPOSITORY: &str = "ghcr.io/decapodlabs/decapod";
pub const DECAPOD_WORKSPACE_IMAGE_REPOSITORY: &str = "localhost/decapod-workspace";
const DECAPOD_SOURCE_LABEL: &str =
    "org.opencontainers.image.source=https://github.com/DecapodLabs/decapod";
const DECAPOD_WORKSPACE_LABEL: &str = "org.decapod.managed=workspace";

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
    format!("v{}", env!("CARGO_PKG_VERSION"))
}

pub fn is_current_decapod_image(repository: &str, tag: &str, version_tag: &str) -> bool {
    match repository {
        DECAPOD_RELEASE_IMAGE_REPOSITORY => {
            tag == version_tag || tag == format!("{version_tag}-alpine")
        }
        _ => false,
    }
}

/// Remove Decapod-owned image tags from older releases and reclaim their
/// unreferenced layers without touching unrelated container images.
pub fn prune_decapod_images() -> Result<ImagePruneReport, error::DecapodError> {
    let runtime = find_container_runtime()?;
    let version_tag = current_decapod_version_tag();
    let listed = Command::new(&runtime)
        .args([
            "image",
            "ls",
            "--format",
            "{{.Repository}}|{{.Tag}}|{{.ID}}",
        ])
        .output()
        .map_err(error::DecapodError::IoError)?;
    if !listed.status.success() {
        return Err(error::DecapodError::ValidationError(format!(
            "Failed to list container images: {}",
            String::from_utf8_lossy(&listed.stderr).trim()
        )));
    }

    let mut report = ImagePruneReport::default();
    for line in String::from_utf8_lossy(&listed.stdout).lines() {
        let mut fields = line.split('|');
        let Some(repository) = fields.next() else {
            continue;
        };
        let Some(tag) = fields.next() else {
            continue;
        };
        if repository != DECAPOD_RELEASE_IMAGE_REPOSITORY
            || tag == "<none>"
            || is_current_decapod_image(repository, tag, &version_tag)
        {
            continue;
        }

        let image_ref = format!("{repository}:{tag}");
        let removed = Command::new(&runtime)
            .args(["image", "rm", "--force", &image_ref])
            .output()
            .map_err(error::DecapodError::IoError)?;
        if !removed.status.success() {
            return Err(error::DecapodError::ValidationError(format!(
                "Failed to remove stale Decapod image {image_ref}: {}",
                String::from_utf8_lossy(&removed.stderr).trim()
            )));
        }
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
            .output()
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

pub fn remove_image(image_ref: &str) -> Result<bool, error::DecapodError> {
    let runtime = find_container_runtime()?;
    let removed = Command::new(&runtime)
        .args(["image", "rm", "--force", image_ref])
        .output()
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
        .output()
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
    if command_succeeds(cmd, "--help") || command_succeeds(cmd, "--version") {
        return true;
    }
    let Some(path) = std::env::var_os("PATH") else {
        return false;
    };
    std::env::split_paths(&path).any(|dir| executable_exists(&dir.join(cmd)))
}

fn executable_exists(path: &Path) -> bool {
    path.is_file()
}

fn command_succeeds(cmd: &str, arg: &str) -> bool {
    Command::new(cmd)
        .arg(arg)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
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
}
