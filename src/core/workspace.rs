//! Workspace management with Docker-first isolation
//!
//! Decapod enforces Silicon Valley project hygiene: containerized, reproducible,
//! isolated workspaces. Docker is the PRIMARY interface - git worktrees are an
//! implementation detail.
//!
//! # For AI Agents
//!
//! You MUST work in a Docker container. Decapod refuses direct host filesystem work.
//! The workspace system ensures:
//! - Containerized execution (no exceptions)
//! - Git worktree isolation (for parallel work)
//! - Protected branch enforcement (no main/master mutations)
//! - Reproducible environments (identical to CI)

use crate::core::error::DecapodError;
use crate::core::rpc::{AllowedOp, Blocker, BlockerKind};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Command;

/// Workspace status information
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WorkspaceStatus {
    /// Whether workspace is valid for work
    pub can_work: bool,
    /// Docker container context
    pub container: ContainerStatus,
    /// Git workspace context
    pub git: GitStatus,
    /// Blockers preventing work
    pub blockers: Vec<Blocker>,
    /// Required actions before working
    pub required_actions: Vec<String>,
}

/// Container/Docker status
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ContainerStatus {
    /// Whether running inside a Docker container
    pub in_container: bool,
    /// Container ID (if in container)
    pub container_id: Option<String>,
    /// Container image name
    pub image: Option<String>,
    /// Whether Docker is available on host
    pub docker_available: bool,
    /// Dockerfile hash (for reproducibility verification)
    pub dockerfile_hash: Option<String>,
    /// Container workspace path
    pub workspace_path: Option<PathBuf>,
}

/// Git status
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GitStatus {
    /// Current branch name
    pub current_branch: String,
    /// Whether branch is protected
    pub is_protected: bool,
    /// Whether in git worktree
    pub in_worktree: bool,
    /// Worktree path
    pub worktree_path: Option<PathBuf>,
    /// Has local modifications
    pub has_local_mods: bool,
}

/// Workspace configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WorkspaceConfig {
    /// Base image for container
    pub base_image: String,
    /// Git branch name
    pub branch: String,
    /// Whether to use container (always true)
    pub use_container: bool,
    /// Resource limits
    pub resources: ContainerResources,
}

/// Container resource limits
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ContainerResources {
    pub memory: String,
    pub cpus: String,
    pub timeout_seconds: u64,
}

impl Default for ContainerResources {
    fn default() -> Self {
        Self {
            memory: "4g".to_string(),
            cpus: "2".to_string(),
            timeout_seconds: 3600,
        }
    }
}

/// Protected branch patterns
const PROTECTED_PATTERNS: &[&str] = &["main", "master", "production", "release/*", "hotfix/*"];

/// Get workspace status - checks both container and git contexts
pub fn get_workspace_status(repo_root: &Path) -> Result<WorkspaceStatus, DecapodError> {
    let container = check_container_status(repo_root)?;
    let git = check_git_status(repo_root)?;

    // Determine if work is allowed
    let mut blockers = vec![];
    let mut required_actions = vec![];

    // CRITICAL: Must be in container
    if !container.in_container {
        blockers.push(Blocker {
            kind: BlockerKind::WorkspaceRequired,
            message: "Not running in Docker container. Silicon Valley hygiene requires containerized work.".to_string(),
            resolve_hint: "Run 'decapod workspace ensure --container' to create containerized workspace".to_string(),
        });
        required_actions.push("Create containerized workspace".to_string());
    }

    // CRITICAL: Must not be on protected branch
    if git.is_protected && !container.in_container {
        blockers.push(Blocker {
            kind: BlockerKind::ProtectedBranch,
            message: format!(
                "On protected branch '{}' without container isolation",
                git.current_branch
            ),
            resolve_hint: "Use containerized workspace to work safely".to_string(),
        });
        required_actions.push("Switch to isolated workspace".to_string());
    }

    // Docker must be available
    if !container.docker_available {
        blockers.push(Blocker {
            kind: BlockerKind::ValidationFailed,
            message: "Docker not available. Containerized work required.".to_string(),
            resolve_hint: "Install Docker or use environment with Docker available".to_string(),
        });
    }

    let can_work = container.in_container && container.docker_available && !git.is_protected;

    Ok(WorkspaceStatus {
        can_work,
        container,
        git,
        blockers,
        required_actions,
    })
}

/// Check container status
fn check_container_status(repo_root: &Path) -> Result<ContainerStatus, DecapodError> {
    // Check if running inside container
    let in_container = Path::new("/.dockerenv").exists()
        || std::env::var("CONTAINER_ID").is_ok()
        || std::env::var("KUBERNETES_SERVICE_HOST").is_ok();

    // Get container ID if in container
    let container_id = if in_container {
        std::fs::read_to_string("/etc/hostname")
            .ok()
            .map(|s| s.trim().to_string())
    } else {
        None
    };

    // Check Docker availability on host
    let docker_available = Command::new("docker")
        .args(["version", "--format", "{{.Server.Version}}"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    // Get image name
    let image = std::env::var("DECAPOD_WORKSPACE_IMAGE").ok();

    // Compute Dockerfile hash for reproducibility
    let dockerfile_hash = compute_dockerfile_hash(repo_root)?;

    // Container workspace path
    let workspace_path = if in_container {
        Some(repo_root.to_path_buf())
    } else {
        None
    };

    Ok(ContainerStatus {
        in_container,
        container_id,
        image,
        docker_available,
        dockerfile_hash,
        workspace_path,
    })
}

/// Check git status
fn check_git_status(repo_root: &Path) -> Result<GitStatus, DecapodError> {
    let current_branch = get_current_branch(repo_root)?;
    let is_protected = is_branch_protected(&current_branch);
    let in_worktree = is_worktree(repo_root)?;
    let has_local_mods = has_local_modifications(repo_root)?;

    let worktree_path = if in_worktree {
        Some(repo_root.to_path_buf())
    } else {
        None
    };

    Ok(GitStatus {
        current_branch,
        is_protected,
        in_worktree,
        worktree_path,
        has_local_mods,
    })
}

/// Compute hash of Dockerfile for reproducibility verification
fn compute_dockerfile_hash(repo_root: &Path) -> Result<Option<String>, DecapodError> {
    let dockerfile_paths = [
        repo_root.join("Dockerfile"),
        repo_root
            .join(".decapod")
            .join("generated")
            .join("Dockerfile"),
        repo_root.join(".devcontainer").join("Dockerfile"),
    ];

    for path in &dockerfile_paths {
        if path.exists() {
            let content = std::fs::read_to_string(path).map_err(|e| DecapodError::IoError(e))?;
            use sha2::{Digest, Sha256};
            let mut hasher = Sha256::new();
            hasher.update(content.as_bytes());
            return Ok(Some(format!("{:x}", hasher.finalize())));
        }
    }

    Ok(None)
}

/// Ensure/create containerized workspace
pub fn ensure_workspace(
    repo_root: &Path,
    config: Option<WorkspaceConfig>,
    agent_id: &str,
) -> Result<WorkspaceStatus, DecapodError> {
    let status = get_workspace_status(repo_root)?;

    // If already in valid container workspace, return status
    if status.can_work && status.container.in_container {
        return Ok(status);
    }

    // Check Docker is available
    if !status.container.docker_available {
        return Err(DecapodError::ValidationError(
            "Docker not available. Cannot create containerized workspace.".to_string(),
        ));
    }

    let config = config.unwrap_or_else(|| WorkspaceConfig {
        base_image: "rust:1.75-slim".to_string(),
        branch: format!(
            "agent/{}/{}",
            sanitize_agent_id(agent_id),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs()
        ),
        use_container: true,
        resources: ContainerResources::default(),
    });

    // Create git worktree for isolation
    let worktree_path = create_worktree(repo_root, &config.branch, agent_id)?;

    // Generate Dockerfile if doesn't exist
    ensure_dockerfile(&worktree_path)?;

    // Build container image
    let image_tag = format!(
        "decapod-workspace:{}-{}",
        sanitize_agent_id(agent_id),
        config.branch.replace('/', "-")
    );

    build_workspace_image(&worktree_path, &image_tag)?;

    // Return instructions for entering container
    Ok(WorkspaceStatus {
        can_work: false, // Still need to enter container
        container: ContainerStatus {
            in_container: false,
            container_id: None,
            image: Some(image_tag.clone()),
            docker_available: true,
            dockerfile_hash: compute_dockerfile_hash(&worktree_path)?,
            workspace_path: Some(worktree_path.clone()),
        },
        git: GitStatus {
            current_branch: config.branch.clone(),
            is_protected: false,
            in_worktree: true,
            worktree_path: Some(worktree_path.clone()),
            has_local_mods: false,
        },
        blockers: vec![Blocker {
            kind: BlockerKind::WorkspaceRequired,
            message: "Container workspace created. You must now enter it.".to_string(),
            resolve_hint: format!(
                "cd {} && docker run -it -v $(pwd):/workspace {} bash",
                worktree_path.display(),
                image_tag
            ),
        }],
        required_actions: vec!["Enter containerized workspace".to_string()],
    })
}

/// Create git worktree
fn create_worktree(
    repo_root: &Path,
    branch: &str,
    agent_id: &str,
) -> Result<PathBuf, DecapodError> {
    let main_repo = get_main_repo_root(repo_root)?;

    // Create worktree directory
    let workspaces_dir = main_repo.join(".decapod").join("workspaces");
    std::fs::create_dir_all(&workspaces_dir).map_err(|e| DecapodError::IoError(e))?;

    let worktree_name = format!(
        "{}-{}",
        sanitize_agent_id(agent_id),
        branch.replace('/', "-")
    );
    let worktree_path = workspaces_dir.join(&worktree_name);

    // Create branch if doesn't exist
    let _ = Command::new("git")
        .args(["-C", main_repo.to_str().unwrap_or("."), "branch", branch])
        .output();

    // Create worktree
    let output = Command::new("git")
        .args([
            "-C",
            main_repo.to_str().unwrap_or("."),
            "worktree",
            "add",
            worktree_path.to_str().unwrap_or("."),
            branch,
        ])
        .output()
        .map_err(|e| DecapodError::IoError(e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(DecapodError::ValidationError(format!(
            "Failed to create worktree: {}",
            stderr
        )));
    }

    Ok(worktree_path)
}

/// Ensure Dockerfile exists in workspace
fn ensure_dockerfile(workspace_path: &Path) -> Result<(), DecapodError> {
    let dockerfile_path = workspace_path.join("Dockerfile");

    if dockerfile_path.exists() {
        return Ok(());
    }

    // Generate standard Decapod workspace Dockerfile
    let dockerfile_content = r#"# Decapod Workspace Dockerfile
# Auto-generated for reproducible agent environments

FROM rust:1.75-slim

# Install essential tools
RUN apt-get update && apt-get install -y \
    git \
    curl \
    build-essential \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# Install decapod
RUN cargo install decapod

# Set up workspace
WORKDIR /workspace
ENV DECAPOD_IN_CONTAINER=true
ENV DECAPOD_WORKSPACE_IMAGE=decapod-workspace

# Default command
CMD ["/bin/bash"]
"#;

    std::fs::write(&dockerfile_path, dockerfile_content).map_err(|e| DecapodError::IoError(e))?;

    Ok(())
}

/// Build workspace container image
fn build_workspace_image(workspace_path: &Path, image_tag: &str) -> Result<(), DecapodError> {
    let output = Command::new("docker")
        .args([
            "build",
            "-t",
            image_tag,
            workspace_path.to_str().unwrap_or("."),
        ])
        .output()
        .map_err(|e| DecapodError::IoError(e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(DecapodError::ValidationError(format!(
            "Failed to build container image: {}",
            stderr
        )));
    }

    Ok(())
}

/// Get main repo root
fn get_main_repo_root(current_dir: &Path) -> Result<PathBuf, DecapodError> {
    let output = Command::new("git")
        .args([
            "-C",
            current_dir.to_str().unwrap_or("."),
            "rev-parse",
            "--git-common-dir",
        ])
        .output()
        .map_err(|e| DecapodError::IoError(e))?;

    if !output.status.success() {
        return get_repo_root(current_dir);
    }

    let git_common_dir_str = String::from_utf8_lossy(&output.stdout);
    let git_common_dir = git_common_dir_str.trim();
    PathBuf::from(git_common_dir)
        .parent()
        .map(|p| p.to_path_buf())
        .ok_or_else(|| {
            DecapodError::ValidationError("Could not determine main repository root".to_string())
        })
}

/// Get repo root
fn get_repo_root(start_dir: &Path) -> Result<PathBuf, DecapodError> {
    let output = Command::new("git")
        .args([
            "-C",
            start_dir.to_str().unwrap_or("."),
            "rev-parse",
            "--show-toplevel",
        ])
        .output()
        .map_err(|e| DecapodError::IoError(e))?;

    if !output.status.success() {
        return Err(DecapodError::ValidationError(
            "Not in a git repository".to_string(),
        ));
    }

    Ok(PathBuf::from(
        String::from_utf8_lossy(&output.stdout).trim(),
    ))
}

/// Check if branch is protected
fn is_branch_protected(branch: &str) -> bool {
    let branch_lower = branch.to_lowercase();

    for pattern in PROTECTED_PATTERNS {
        if pattern.ends_with("/*") {
            let prefix = &pattern[..pattern.len() - 2];
            if branch_lower.starts_with(prefix) {
                return true;
            }
        } else if branch_lower == *pattern {
            return true;
        }
    }

    false
}

/// Get current branch
fn get_current_branch(repo_root: &Path) -> Result<String, DecapodError> {
    // Prefer modern branch API; fall back to rev-parse/symbolic-ref for portability.
    let try_branch = Command::new("git")
        .args([
            "-C",
            repo_root.to_str().unwrap_or("."),
            "branch",
            "--show-current",
        ])
        .output()
        .map_err(DecapodError::IoError)?;
    if try_branch.status.success() {
        let branch = String::from_utf8_lossy(&try_branch.stdout)
            .trim()
            .to_string();
        if !branch.is_empty() {
            return Ok(branch);
        }
    }

    let try_abbrev = Command::new("git")
        .args([
            "-C",
            repo_root.to_str().unwrap_or("."),
            "rev-parse",
            "--abbrev-ref",
            "HEAD",
        ])
        .output()
        .map_err(DecapodError::IoError)?;
    if try_abbrev.status.success() {
        let branch = String::from_utf8_lossy(&try_abbrev.stdout)
            .trim()
            .to_string();
        if !branch.is_empty() && branch != "HEAD" {
            return Ok(branch);
        }
    }

    let rev_output = Command::new("git")
        .args([
            "-C",
            repo_root.to_str().unwrap_or("."),
            "rev-parse",
            "--short",
            "HEAD",
        ])
        .output()
        .map_err(DecapodError::IoError)?;
    if rev_output.status.success() {
        let hash = String::from_utf8_lossy(&rev_output.stdout)
            .trim()
            .to_string();
        if !hash.is_empty() {
            return Ok(format!("detached-{}", hash));
        }
    }

    Ok("unknown".to_string())
}

/// Check if in worktree
fn is_worktree(repo_root: &Path) -> Result<bool, DecapodError> {
    let output = Command::new("git")
        .args([
            "-C",
            repo_root.to_str().unwrap_or("."),
            "rev-parse",
            "--git-dir",
        ])
        .output()
        .map_err(|e| DecapodError::IoError(e))?;

    if !output.status.success() {
        return Ok(false);
    }

    let git_dir = String::from_utf8_lossy(&output.stdout).trim().to_string();
    Ok(git_dir.contains("worktrees") || !git_dir.ends_with(".git"))
}

/// Check for local modifications
fn has_local_modifications(repo_root: &Path) -> Result<bool, DecapodError> {
    let output = Command::new("git")
        .args([
            "-C",
            repo_root.to_str().unwrap_or("."),
            "status",
            "--porcelain",
        ])
        .output()
        .map_err(|e| DecapodError::IoError(e))?;

    if !output.status.success() {
        return Ok(false);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(!stdout.trim().is_empty())
}

/// Sanitize agent ID
fn sanitize_agent_id(agent_id: &str) -> String {
    agent_id
        .to_lowercase()
        .replace(|c: char| !c.is_alphanumeric() && c != '-' && c != '_', "-")
        .replace("--", "-")
        .trim_matches('-')
        .to_string()
}

/// Get allowed operations based on status
pub fn get_allowed_ops(status: &WorkspaceStatus) -> Vec<AllowedOp> {
    let mut ops = vec![];

    if !status.container.in_container {
        ops.push(AllowedOp {
            op: "workspace.ensure".to_string(),
            reason: "Create containerized workspace (Docker required)".to_string(),
            required_params: vec![],
        });
    } else if status.git.is_protected {
        ops.push(AllowedOp {
            op: "workspace.ensure".to_string(),
            reason: "Create isolated workspace (not on protected branch)".to_string(),
            required_params: vec![],
        });
    } else {
        ops.push(AllowedOp {
            op: "todo.claim".to_string(),
            reason: "Ready to claim work".to_string(),
            required_params: vec![],
        });
    }

    ops.push(AllowedOp {
        op: "workspace.status".to_string(),
        reason: "Check workspace state".to_string(),
        required_params: vec![],
    });

    ops
}
