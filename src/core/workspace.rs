//! Workspace management with Git Worktree and Docker isolation
//!
//! Provides repository isolation primitives:
//! - git worktree status and provisioning
//! - protected-branch safeguards
//! - optional containerized execution for reproducible builds

use crate::core::db;
use crate::core::error::DecapodError;
use crate::core::rpc::{AllowedOp, Blocker, BlockerKind};
use crate::core::todo;
use crate::core::workunit::{self, WorkUnitStatus};
use crate::plugins::eval;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Workspace status information
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WorkspaceStatus {
    /// Whether workspace is valid for work
    pub can_work: bool,
    /// Git workspace context
    pub git: GitStatus,
    /// Docker container context
    pub container: ContainerStatus,
    /// Blockers preventing work
    pub blockers: Vec<Blocker>,
    /// Required actions before working
    pub required_actions: Vec<String>,
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
    /// Worktree path (if in worktree)
    pub worktree_path: Option<PathBuf>,
    /// Whether this is the main repository checkout
    pub is_main_repo: bool,
    /// Has local modifications
    pub has_local_mods: bool,
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
}

/// Workspace configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WorkspaceConfig {
    /// Git branch name
    pub branch: String,
    /// Whether to use container
    pub use_container: bool,
    /// Base image for container (if use_container is true)
    pub base_image: Option<String>,
}

#[derive(Debug, Clone)]
struct AssignedTodoRef {
    id: String,
    hash: String,
}

/// Protected branch patterns
const PROTECTED_PATTERNS: &[&str] = &[
    "main",
    "master",
    "production",
    "stable",
    "release/*",
    "hotfix/*",
];

/// Prune stale git worktree metadata and remove stale worktree sections from .git/config.
///
/// This is a best-effort maintenance operation to keep worktree state healthy after
/// merged PRs, deleted branches, or manually removed worktree directories.
/// Returns the number of stale `worktree.<name>` sections removed from `.git/config`.
pub fn prune_stale_worktree_config(repo_root: &Path) -> Result<usize, DecapodError> {
    let main_repo = get_main_repo_root(repo_root)?;
    let dir = main_repo.to_str().unwrap_or(".");

    // 1) Let git clean known stale admin entries first.
    let prune_output = Command::new("git")
        .args(["-C", dir, "worktree", "prune", "--expire", "now"])
        .output()
        .map_err(DecapodError::IoError)?;
    if !prune_output.status.success() {
        return Err(DecapodError::ValidationError(format!(
            "Failed to prune worktrees: {}",
            String::from_utf8_lossy(&prune_output.stderr)
        )));
    }

    let config_path = main_repo.join(".git").join("config");
    if !config_path.exists() {
        return Ok(0);
    }

    let registered_paths = registered_worktree_paths(&main_repo)?;
    let keys_output = Command::new("git")
        .args([
            "-C",
            dir,
            "config",
            "--file",
            config_path.to_str().unwrap_or(".git/config"),
            "--name-only",
            "--get-regexp",
            r"^worktree\..*\.path$",
        ])
        .output()
        .map_err(DecapodError::IoError)?;

    // No worktree sections to process.
    if !keys_output.status.success() && keys_output.stdout.is_empty() {
        return Ok(0);
    }

    let mut removed = 0usize;
    for key in String::from_utf8_lossy(&keys_output.stdout)
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        let Some(section_name) = key.strip_suffix(".path") else {
            continue;
        };
        let value_output = Command::new("git")
            .args([
                "-C",
                dir,
                "config",
                "--file",
                config_path.to_str().unwrap_or(".git/config"),
                "--get",
                key,
            ])
            .output()
            .map_err(DecapodError::IoError)?;
        if !value_output.status.success() {
            continue;
        }
        let raw_path = String::from_utf8_lossy(&value_output.stdout)
            .trim()
            .to_string();
        if raw_path.is_empty() {
            continue;
        }
        let candidate = resolve_worktree_candidate_path(&main_repo, &raw_path);
        let normalized = normalize_path_for_compare(&candidate);
        let is_stale = !candidate.exists() || !registered_paths.contains(&normalized);
        if !is_stale {
            continue;
        }

        let remove_output = Command::new("git")
            .args([
                "-C",
                dir,
                "config",
                "--file",
                config_path.to_str().unwrap_or(".git/config"),
                "--remove-section",
                section_name,
            ])
            .output()
            .map_err(DecapodError::IoError)?;
        if remove_output.status.success() {
            removed += 1;
        }
    }

    Ok(removed)
}

/// Get workspace status
pub fn get_workspace_status(repo_root: &Path) -> Result<WorkspaceStatus, DecapodError> {
    let git = check_git_status(repo_root)?;
    let container = check_container_status(repo_root)?;

    let mut blockers = vec![];
    let mut required_actions = vec![];

    // Mandate: Must not work on protected branch
    if git.is_protected {
        blockers.push(Blocker {
            kind: BlockerKind::ProtectedBranch,
            message: format!("Currently on protected branch '{}'. Decapod prohibits implementation work on protected refs.", git.current_branch),
            resolve_hint: "Run `decapod todo claim --id <task-id>` then `decapod workspace ensure` to create a todo-scoped isolated worktree.".to_string(),
        });
        required_actions.push("Switch to working branch".to_string());
        if git.has_local_mods {
            blockers.push(Blocker {
                kind: BlockerKind::WorkspaceRequired,
                message: "Protected branch has local modifications. Creating an isolated worktree from a dirty protected branch is blocked.".to_string(),
                resolve_hint: "Commit/stash/discard local changes on protected branch, then run `decapod workspace ensure`.".to_string(),
            });
            required_actions.push("Commit/stash/discard local modifications".to_string());
        }
    }

    // Mandate: Should use worktree for isolation
    if !git.in_worktree && !git.is_protected {
        // Technically allowed if not on master, but we prefer worktrees for agents
        // to keep the main checkout clean and allow parallel agents.
    }

    let can_work = !git.is_protected;

    Ok(WorkspaceStatus {
        can_work,
        git,
        container,
        blockers,
        required_actions,
    })
}

fn check_git_status(repo_root: &Path) -> Result<GitStatus, DecapodError> {
    let current_branch = get_current_branch(repo_root)?;
    let is_protected = is_branch_protected(&current_branch);
    let in_worktree = is_worktree(repo_root)?;
    let has_local_mods = has_local_modifications(repo_root)?;

    // Check if this is the main repository by seeing if .git is a directory
    let is_main_repo = repo_root.join(".git").is_dir();

    Ok(GitStatus {
        current_branch,
        is_protected,
        in_worktree,
        worktree_path: if in_worktree {
            Some(repo_root.to_path_buf())
        } else {
            None
        },
        is_main_repo,
        has_local_mods,
    })
}

fn check_container_status(_repo_root: &Path) -> Result<ContainerStatus, DecapodError> {
    let in_container = Path::new("/.dockerenv").exists() || std::env::var("CONTAINER_ID").is_ok();

    let container_id = if in_container {
        std::fs::read_to_string("/etc/hostname")
            .ok()
            .map(|s| s.trim().to_string())
    } else {
        None
    };

    let docker_available = Command::new("docker")
        .arg("version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    Ok(ContainerStatus {
        in_container,
        container_id,
        image: std::env::var("DECAPOD_WORKSPACE_IMAGE").ok(),
        docker_available,
    })
}

/// Ensure/create isolated workspace
pub fn ensure_workspace(
    repo_root: &Path,
    config: Option<WorkspaceConfig>,
    agent_id: &str,
) -> Result<WorkspaceStatus, DecapodError> {
    let main_repo = get_main_repo_root(repo_root)?;
    let store_root = main_repo.join(".decapod").join("data");
    db::storage_health_preflight(&store_root).map_err(|e| {
        DecapodError::ValidationError(format!("WORKSPACE_STORAGE_PREFLIGHT_FAILED: {}", e))
    })?;

    let mut status = get_workspace_status(repo_root)?;
    if status.git.is_protected && status.git.has_local_mods && !status.git.in_worktree {
        return Err(DecapodError::ValidationError(
            "WORKSPACE_INTERLOCK_DIRTY_PROTECTED: protected branch has local modifications. Commit/stash/discard changes before creating a Decapod worktree.".to_string(),
        ));
    }
    let assigned_todos = get_assigned_open_tasks(repo_root, agent_id)?;
    if assigned_todos.is_empty() {
        return Err(DecapodError::ValidationError(format!(
            "No claimed/open todo assigned to agent '{}'. Claim a todo first with `decapod todo claim --id <task-id>` before spawning a worktree.",
            agent_id
        )));
    }

    // If config is provided, check if we need to upgrade context (e.g. add container)
    let upgrade_container = config.as_ref().map(|c| c.use_container).unwrap_or(false);

    // If we're already in a valid worktree, on todo-scoped branch, and no upgrade needed, we're good.
    if status.git.in_worktree
        && !branch_contains_any_todo_id_or_hash(&status.git.current_branch, &assigned_todos)
    {
        return Err(DecapodError::ValidationError(format!(
            "Current worktree branch '{}' is not todo-scoped. Branch must include one of assigned todo IDs or hashes: {}.",
            status.git.current_branch,
            render_todo_refs(&assigned_todos)
        )));
    }

    if status.can_work
        && status.git.in_worktree
        && !status.git.is_protected
        && (!upgrade_container || status.container.in_container)
    {
        return Ok(status);
    }

    let todo_scope = build_todo_scope_component(&assigned_todos);
    let config = if let Some(cfg) = config {
        if !branch_contains_any_todo_id_or_hash(&cfg.branch, &assigned_todos) {
            return Err(DecapodError::ValidationError(format!(
                "Requested branch '{}' must include an assigned todo ID/hash (one of: {}).",
                cfg.branch,
                render_todo_refs(&assigned_todos)
            )));
        }
        cfg
    } else {
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        WorkspaceConfig {
            branch: format!(
                "agent/{}/{}-{}",
                sanitize_agent_id(agent_id),
                todo_scope,
                ts
            ),
            use_container: false,
            base_image: None,
        }
    };

    // 1. Ensure git worktree
    let worktree_path = if status.git.in_worktree {
        repo_root.to_path_buf()
    } else {
        create_worktree(repo_root, &config.branch, agent_id, &todo_scope)?
    };

    // 2. Ensure container (if requested)
    if config.use_container {
        ensure_dockerfile(&worktree_path)?;
        let image_tag = format!(
            "decapod-workspace:{}-{}",
            sanitize_agent_id(agent_id),
            config.branch.replace('/', "-")
        );
        build_workspace_image(&worktree_path, &image_tag)?;

        // Return blocker telling agent to enter container
        // We re-read status but override the blocker/container info
        status = get_workspace_status(&worktree_path)?;
        status.blockers.push(Blocker {
            kind: BlockerKind::WorkspaceRequired,
            message: "Container environment prepared.".to_string(),
            resolve_hint: format!(
                "cd {} && docker run -it -v $(pwd):/workspace {} bash",
                worktree_path.display(),
                image_tag
            ),
        });
        status
            .required_actions
            .push("Enter containerized workspace".to_string());
        return Ok(status);
    }

    // Re-check status in the new worktree
    get_workspace_status(&worktree_path)
}

fn create_worktree(
    repo_root: &Path,
    branch: &str,
    agent_id: &str,
    todo_scope: &str,
) -> Result<PathBuf, DecapodError> {
    let main_repo = get_main_repo_root(repo_root)?;
    let workspaces_dir = main_repo.join(".decapod").join("workspaces");
    std::fs::create_dir_all(&workspaces_dir).map_err(DecapodError::IoError)?;

    let worktree_name = format!(
        "{}-{}-{}",
        sanitize_agent_id(agent_id),
        todo_scope,
        branch.replace('/', "-")
    );
    let worktree_path = workspaces_dir.join(&worktree_name);

    if worktree_path.exists() {
        return Ok(worktree_path);
    }

    // git worktree add <path> -b <branch>
    let output = Command::new("git")
        .args([
            "-C",
            main_repo.to_str().unwrap_or("."),
            "worktree",
            "add",
            "-b",
            branch,
            worktree_path.to_str().unwrap_or("."),
        ])
        .output()
        .map_err(DecapodError::IoError)?;

    if !output.status.success() {
        // Fallback: try adding without -b if branch might exist
        let output2 = Command::new("git")
            .args([
                "-C",
                main_repo.to_str().unwrap_or("."),
                "worktree",
                "add",
                worktree_path.to_str().unwrap_or("."),
                branch,
            ])
            .output()
            .map_err(DecapodError::IoError)?;

        if !output2.status.success() {
            let stderr = String::from_utf8_lossy(&output2.stderr);
            return Err(DecapodError::ValidationError(format!(
                "Failed to create worktree: {}",
                stderr
            )));
        }
    }

    Ok(worktree_path)
}

fn registered_worktree_paths(main_repo: &Path) -> Result<HashSet<String>, DecapodError> {
    let output = Command::new("git")
        .args([
            "-C",
            main_repo.to_str().unwrap_or("."),
            "worktree",
            "list",
            "--porcelain",
        ])
        .output()
        .map_err(DecapodError::IoError)?;
    if !output.status.success() {
        return Err(DecapodError::ValidationError(format!(
            "Failed to list git worktrees: {}",
            String::from_utf8_lossy(&output.stderr)
        )));
    }

    let mut out = HashSet::new();
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        let Some(path) = line.strip_prefix("worktree ") else {
            continue;
        };
        out.insert(normalize_path_for_compare(Path::new(path.trim())));
    }
    Ok(out)
}

fn resolve_worktree_candidate_path(main_repo: &Path, raw: &str) -> PathBuf {
    let p = PathBuf::from(raw);
    if p.is_absolute() {
        p
    } else {
        main_repo.join(p)
    }
}

fn normalize_path_for_compare(path: &Path) -> String {
    std::fs::canonicalize(path)
        .unwrap_or_else(|_| path.to_path_buf())
        .to_string_lossy()
        .to_string()
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

    std::fs::write(&dockerfile_path, dockerfile_content).map_err(DecapodError::IoError)?;

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
        .map_err(DecapodError::IoError)?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(DecapodError::ValidationError(format!(
            "Failed to build container image: {}",
            stderr
        )));
    }

    Ok(())
}

fn get_main_repo_root(current_dir: &Path) -> Result<PathBuf, DecapodError> {
    let output = Command::new("git")
        .args([
            "-C",
            current_dir.to_str().unwrap_or("."),
            "rev-parse",
            "--git-common-dir",
        ])
        .output()
        .map_err(DecapodError::IoError)?;

    if !output.status.success() {
        // Not in a worktree, return current toplevel
        return get_repo_root(current_dir);
    }

    let common_dir = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let common_path = Path::new(&common_dir);

    // If common_dir is ".git", then current_dir IS the main repo
    if common_dir == ".git" {
        return get_repo_root(current_dir);
    }

    Ok(common_path.parent().unwrap_or(common_path).to_path_buf())
}

fn get_repo_root(start_dir: &Path) -> Result<PathBuf, DecapodError> {
    let output = Command::new("git")
        .args([
            "-C",
            start_dir.to_str().unwrap_or("."),
            "rev-parse",
            "--show-toplevel",
        ])
        .output()
        .map_err(DecapodError::IoError)?;

    if !output.status.success() {
        return Err(DecapodError::ValidationError(
            "Not in a git repository".to_string(),
        ));
    }

    Ok(PathBuf::from(
        String::from_utf8_lossy(&output.stdout).trim(),
    ))
}

fn is_branch_protected(branch: &str) -> bool {
    let branch_lower = branch.to_lowercase();
    for pattern in PROTECTED_PATTERNS {
        if let Some(prefix) = pattern.strip_suffix("/*") {
            if branch_lower.starts_with(prefix) {
                return true;
            }
        } else if branch_lower == *pattern {
            return true;
        }
    }
    false
}

fn get_current_branch(repo_root: &Path) -> Result<String, DecapodError> {
    let output = Command::new("git")
        .args([
            "-C",
            repo_root.to_str().unwrap_or("."),
            "branch",
            "--show-current",
        ])
        .output()
        .map_err(DecapodError::IoError)?;

    let branch = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if branch.is_empty() {
        // Fallback for detached HEAD
        let output = Command::new("git")
            .args([
                "-C",
                repo_root.to_str().unwrap_or("."),
                "rev-parse",
                "--short",
                "HEAD",
            ])
            .output()
            .map_err(DecapodError::IoError)?;
        return Ok(format!(
            "detached-{}",
            String::from_utf8_lossy(&output.stdout).trim()
        ));
    }
    Ok(branch)
}

fn is_worktree(repo_root: &Path) -> Result<bool, DecapodError> {
    let output = Command::new("git")
        .args([
            "-C",
            repo_root.to_str().unwrap_or("."),
            "rev-parse",
            "--git-dir",
        ])
        .output()
        .map_err(DecapodError::IoError)?;

    let git_dir = String::from_utf8_lossy(&output.stdout).trim().to_string();
    // In a worktree, git-dir is usually <main-repo>/.git/worktrees/<name>
    Ok(git_dir.contains("/worktrees/"))
}

fn has_local_modifications(repo_root: &Path) -> Result<bool, DecapodError> {
    let output = Command::new("git")
        .args([
            "-C",
            repo_root.to_str().unwrap_or("."),
            "status",
            "--porcelain",
        ])
        .output()
        .map_err(DecapodError::IoError)?;

    Ok(!String::from_utf8_lossy(&output.stdout).trim().is_empty())
}

fn sanitize_agent_id(agent_id: &str) -> String {
    agent_id
        .to_lowercase()
        .replace(|c: char| !c.is_alphanumeric() && c != '-' && c != '_', "-")
        .replace("--", "-")
        .trim_matches('-')
        .to_string()
}

fn sanitize_todo_component(todo_id: &str) -> String {
    todo_id
        .to_lowercase()
        .replace(|c: char| !c.is_alphanumeric() && c != '-' && c != '_', "-")
        .replace("--", "-")
        .trim_matches('-')
        .to_string()
}

fn build_todo_scope_component(todo_refs: &[AssignedTodoRef]) -> String {
    if todo_refs.is_empty() {
        return "todo-unassigned".to_string();
    }
    let head = sanitize_todo_component(&todo_refs[0].hash);
    if todo_refs.len() == 1 {
        return format!("todo-{}", head);
    }
    format!("todo-{}-plus-{}", head, todo_refs.len() - 1)
}

fn branch_contains_any_todo_id_or_hash(branch: &str, todo_refs: &[AssignedTodoRef]) -> bool {
    let branch_lower = branch.to_lowercase();
    todo_refs.iter().any(|todo| {
        let id = &todo.id;
        let id_lower = id.to_lowercase();
        let id_sanitized = sanitize_todo_component(id);
        let hash_lower = todo.hash.to_lowercase();
        branch_lower.contains(&id_lower)
            || branch_lower.contains(&id_sanitized)
            || branch_lower.contains(&hash_lower)
    })
}

fn get_assigned_open_tasks(
    repo_root: &Path,
    agent_id: &str,
) -> Result<Vec<AssignedTodoRef>, DecapodError> {
    let main_repo = get_main_repo_root(repo_root)?;
    let store_root = main_repo.join(".decapod").join("data");
    let mut tasks = todo::list_tasks(
        &store_root,
        Some("open".to_string()),
        None,
        None,
        None,
        None,
    )?;
    tasks.retain(|t| t.assigned_to == agent_id);
    let mut refs: Vec<AssignedTodoRef> = tasks
        .into_iter()
        .map(|t| AssignedTodoRef {
            id: t.id,
            hash: t.hash,
        })
        .collect();
    refs.sort_by(|a, b| a.id.cmp(&b.id));
    refs.dedup_by(|a, b| a.id == b.id);
    Ok(refs)
}

fn render_todo_refs(todo_refs: &[AssignedTodoRef]) -> String {
    todo_refs
        .iter()
        .map(|t| format!("{} ({})", t.id, t.hash))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Result from publishing a workspace
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PublishResult {
    /// Branch that was published
    pub branch: String,
    /// Commit hash of the published changes
    pub commit_hash: String,
    /// Remote URL the branch was pushed to
    pub remote_url: String,
    /// PR URL if one was created
    pub pr_url: Option<String>,
}

/// Publish workspace changes: commit, push, and optionally create a PR
pub fn publish_workspace(
    repo_root: &Path,
    title: Option<String>,
    description: Option<String>,
) -> Result<PublishResult, DecapodError> {
    let status = get_workspace_status(repo_root)?;

    // 1. Must be in a worktree on an unprotected branch
    if !status.git.in_worktree {
        return Err(DecapodError::ValidationError(
            "Cannot publish: not in a git worktree. Run `decapod workspace ensure` first."
                .to_string(),
        ));
    }
    if status.git.is_protected {
        return Err(DecapodError::ValidationError(format!(
            "Cannot publish: on protected branch '{}'. Work must be on a feature branch.",
            status.git.current_branch
        )));
    }
    let artifact_manifest =
        repo_root.join(".decapod/generated/artifacts/provenance/artifact_manifest.json");
    let proof_manifest =
        repo_root.join(".decapod/generated/artifacts/provenance/proof_manifest.json");
    if !artifact_manifest.exists() || !proof_manifest.exists() {
        return Err(DecapodError::ValidationError(
            "Cannot publish: provenance manifests are required for promotion. Missing `.decapod/generated/artifacts/provenance/artifact_manifest.json` and/or `.decapod/generated/artifacts/provenance/proof_manifest.json`."
                .to_string(),
        ));
    }
    verify_workunit_gate_for_publish(repo_root, &status.git.current_branch)?;
    eval::verify_eval_gate_for_publish(&repo_root.join(".decapod").join("data"))?;

    let dir = repo_root.to_str().unwrap_or(".");

    // 2. Stage and commit any uncommitted changes
    if status.git.has_local_mods {
        let add_output = Command::new("git")
            .args(["-C", dir, "add", "-A"])
            .output()
            .map_err(DecapodError::IoError)?;
        if !add_output.status.success() {
            return Err(DecapodError::ValidationError(format!(
                "Failed to stage changes: {}",
                String::from_utf8_lossy(&add_output.stderr)
            )));
        }

        let commit_msg = title
            .as_deref()
            .unwrap_or("decapod: publish workspace changes");
        let commit_output = Command::new("git")
            .args(["-C", dir, "commit", "-m", commit_msg])
            .output()
            .map_err(DecapodError::IoError)?;
        if !commit_output.status.success() {
            let stderr = String::from_utf8_lossy(&commit_output.stderr);
            // Allow "nothing to commit" as non-fatal
            if !stderr.contains("nothing to commit") {
                return Err(DecapodError::ValidationError(format!(
                    "Failed to commit: {}",
                    stderr
                )));
            }
        }
    }

    // Get current commit hash
    let hash_output = Command::new("git")
        .args(["-C", dir, "rev-parse", "HEAD"])
        .output()
        .map_err(DecapodError::IoError)?;
    let commit_hash = String::from_utf8_lossy(&hash_output.stdout)
        .trim()
        .to_string();

    // 3. Push branch to origin
    let push_output = Command::new("git")
        .args([
            "-C",
            dir,
            "push",
            "-u",
            "origin",
            &status.git.current_branch,
        ])
        .output()
        .map_err(DecapodError::IoError)?;
    if !push_output.status.success() {
        return Err(DecapodError::ValidationError(format!(
            "Failed to push: {}",
            String::from_utf8_lossy(&push_output.stderr)
        )));
    }

    // Get remote URL
    let remote_output = Command::new("git")
        .args(["-C", dir, "remote", "get-url", "origin"])
        .output()
        .map_err(DecapodError::IoError)?;
    let remote_url = String::from_utf8_lossy(&remote_output.stdout)
        .trim()
        .to_string();

    // 4. If gh CLI is available, create a PR
    let pr_url = if Command::new("gh")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
    {
        let pr_title = title.as_deref().unwrap_or(&status.git.current_branch);
        let mut pr_args = vec![
            "-C",
            dir,
            "pr",
            "create",
            "--title",
            pr_title,
            "--head",
            &status.git.current_branch,
        ];
        let desc;
        if let Some(ref d) = description {
            desc = d.clone();
            pr_args.push("--body");
            pr_args.push(&desc);
        }
        let pr_output = Command::new("gh")
            .args(&pr_args)
            .output()
            .map_err(DecapodError::IoError)?;
        if pr_output.status.success() {
            Some(
                String::from_utf8_lossy(&pr_output.stdout)
                    .trim()
                    .to_string(),
            )
        } else {
            None
        }
    } else {
        None
    };

    Ok(PublishResult {
        branch: status.git.current_branch,
        commit_hash,
        remote_url,
        pr_url,
    })
}

fn extract_task_ids_from_branch(branch: &str) -> Vec<String> {
    let re = Regex::new(r"(?i)(?:r_|test_|docs_|fix_|feat_)[a-z0-9]+").expect("static regex");
    let mut out: Vec<String> = re
        .find_iter(branch)
        .map(|m| m.as_str().to_string())
        .collect();
    out.sort();
    out.dedup();
    out
}

pub fn verify_workunit_gate_for_publish(
    repo_root: &Path,
    branch: &str,
) -> Result<(), DecapodError> {
    let task_ids = extract_task_ids_from_branch(branch);
    if task_ids.is_empty() {
        return Ok(());
    }

    for task_id in task_ids {
        let path = workunit::workunit_path(repo_root, &task_id)?;
        if !path.exists() {
            return Err(DecapodError::ValidationError(format!(
                "Cannot publish: missing required workunit manifest for task '{}' at {}.",
                task_id,
                path.display()
            )));
        }
        let manifest = workunit::load_workunit(repo_root, &task_id)?;
        if manifest.status != WorkUnitStatus::Verified {
            return Err(DecapodError::ValidationError(format!(
                "Cannot publish: workunit '{}' is not VERIFIED (current {:?}).",
                task_id, manifest.status
            )));
        }
        workunit::verify_capsule_policy_lineage_for_task(repo_root, &manifest)?;
    }

    Ok(())
}

pub fn get_allowed_ops(status: &WorkspaceStatus) -> Vec<AllowedOp> {
    let mut ops = vec![];

    if status.git.is_protected {
        ops.push(AllowedOp {
            op: "workspace.ensure".to_string(),
            reason: "Create isolated working branch (cannot work on protected branch)".to_string(),
            required_params: vec!["branch".to_string()],
        });
    } else {
        ops.push(AllowedOp {
            op: "todo.list".to_string(),
            reason: "Workspace ready for work".to_string(),
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
