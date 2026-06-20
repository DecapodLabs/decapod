//! Workspace management with Git Worktree and Docker isolation
//!
//! Provides repository isolation primitives:
//! - git worktree status and provisioning
//! - protected-branch safeguards
//! - optional containerized execution for reproducible builds

use crate::core::container_runtime;
use crate::core::db;
use crate::core::error::DecapodError;
use crate::core::rpc::{AllowedOp, Blocker, BlockerKind};
use crate::core::todo;
use crate::core::workunit::{self, WorkUnitStatus};
use crate::plugins::eval;
use fancy_regex::Regex;
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
    pub branch: Option<String>,
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
        required_actions
            .push("Run `decapod workspace ensure` and cd into the created worktree".to_string());
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
    if git.is_main_repo && !git.is_protected {
        blockers.push(Blocker {
            kind: BlockerKind::WorkspaceRequired,
            message: "Currently in the main repository checkout. Agentic work MUST be done in an isolated worktree to prevent disrupting the human user's environment.".to_string(),
            resolve_hint: "Run `decapod workspace ensure` and cd into the created worktree.".to_string(),
        });
        required_actions
            .push("Run `decapod workspace ensure` and cd into the created worktree".to_string());
    }

    let can_work = !git.is_main_repo && !git.is_protected;

    Ok(WorkspaceStatus {
        can_work,
        git,
        container,
        blockers,
        required_actions,
    })
}

fn check_git_status(repo_root: &Path) -> Result<GitStatus, DecapodError> {
    if !repo_root.join(".git").exists() {
        return Ok(GitStatus {
            current_branch: "none".to_string(),
            is_protected: false,
            in_worktree: false,
            worktree_path: None,
            is_main_repo: false,
            has_local_mods: false,
        });
    }

    let current_branch = get_current_branch(repo_root)?;
    let is_protected = is_branch_protected(&current_branch);
    let in_worktree = is_worktree(repo_root)?;
    let has_local_mods = has_local_modifications(repo_root)?;

    // Check if this is the main repository checkout (not an isolated workspace)
    let is_main_repo = !repo_root.to_string_lossy().contains(".decapod/workspaces");

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

    let docker_available = container_runtime::container_runtime_available();

    Ok(ContainerStatus {
        in_container,
        container_id,
        image: std::env::var("DECAPOD_WORKSPACE_IMAGE").ok(),
        docker_available,
    })
}

fn external_task_ref() -> String {
    [
        "DECAPOD_TASK_ID",
        "DECAPOD_EXTERNAL_TASK_ID",
        "BD_TASK_ID",
        "BEADS_TASK_ID",
    ]
    .iter()
    .find_map(|key| {
        std::env::var(key)
            .ok()
            .filter(|value| !value.trim().is_empty())
    })
    .unwrap_or_default()
}

fn create_and_claim_coordination_todo(
    repo_root: &Path,
    agent_id: &str,
) -> Result<AssignedTodoRef, DecapodError> {
    let main_repo = get_main_repo_root(repo_root)?;
    let store_root = main_repo.join(".decapod").join("data");
    let external_ref = external_task_ref();
    if !external_ref.is_empty() {
        let tasks = todo::list_tasks(
            &store_root,
            Some("open".to_string()),
            None,
            None,
            None,
            None,
        )?;
        if let Some(task) = tasks.into_iter().find(|task| task.r#ref == external_ref) {
            let claim =
                todo::claim_task(&store_root, &task.id, agent_id, todo::ClaimMode::Exclusive)?;
            if claim.get("status").and_then(|value| value.as_str()) != Some("ok") {
                return Err(DecapodError::ValidationError(format!(
                    "AUTOREMEDIABLE_VALIDATION_ERROR code=WORKSPACE_TODO_CLAIM_CONFLICT severity=transient auto_remediable=true audience=agent agent_action=\"inspect `decapod todo list`; Decapod already captured external task {external_ref} as a coordination todo, so coordinate with the current claimant or wait for release before launching another workspace\" user_note=\"Decapod is protecting this external task with an exclusive todo claim; no work is lost, but another agent already owns the isolated workspace slot.\"\n{claim}"
                )));
            }
            return Ok(AssignedTodoRef {
                id: task.id,
                hash: task.hash,
            });
        }
    }
    let title = if external_ref.is_empty() {
        format!("Decapod workspace coordination for {agent_id}")
    } else {
        format!("Decapod workspace coordination for {external_ref}")
    };
    let description = if external_ref.is_empty() {
        "Auto-created by decapod workspace ensure so Decapod can enforce exclusive agent ownership while an external todo system may also be in use.".to_string()
    } else {
        format!(
            "Auto-created by decapod workspace ensure to coordinate exclusive Decapod ownership for external task {external_ref}."
        )
    };
    let command = todo::TodoCommand::Add {
        title,
        description,
        priority: "medium".to_string(),
        tags: "workspace,coordination,auto-generated".to_string(),
        owner: String::new(),
        due: None,
        r#ref: external_ref,
        scope: "workspace".to_string(),
        dir: Some(main_repo.to_string_lossy().to_string()),
        depends_on: String::new(),
        blocks: String::new(),
        parent: None,
        one_shot: 1,
    };
    let added = todo::add_task(&store_root, &command)?;
    let id = added
        .get("id")
        .and_then(|value| value.as_str())
        .ok_or_else(|| {
            DecapodError::ValidationError("workspace auto-created todo without id".to_string())
        })?
        .to_string();
    let hash = added
        .get("hash")
        .and_then(|value| value.as_str())
        .unwrap_or("")
        .to_string();
    let claim = todo::claim_task(&store_root, &id, agent_id, todo::ClaimMode::Exclusive)?;
    if claim.get("status").and_then(|value| value.as_str()) != Some("ok") {
        return Err(DecapodError::ValidationError(format!(
            "AUTOREMEDIABLE_VALIDATION_ERROR code=WORKSPACE_TODO_CLAIM_CONFLICT severity=transient auto_remediable=true audience=agent agent_action=\"inspect `decapod todo list`; Decapod created a workspace coordination todo and is waiting for an exclusive claim before container launch continues\" user_note=\"Decapod has captured the workspace intent as a todo; resolve the claim conflict and rerun the command.\"\n{claim}"
        )));
    }
    Ok(AssignedTodoRef { id, hash })
}

fn ensure_assigned_open_tasks(
    repo_root: &Path,
    agent_id: &str,
    current_branch: &str,
) -> Result<Vec<AssignedTodoRef>, DecapodError> {
    let mut assigned_todos = get_assigned_open_tasks(repo_root, agent_id)?;
    claim_branch_scoped_open_tasks(repo_root, agent_id, current_branch, &mut assigned_todos)?;
    if assigned_todos.is_empty() {
        assigned_todos.push(create_and_claim_coordination_todo(repo_root, agent_id)?);
    }
    assigned_todos.sort_by(|a, b| a.id.cmp(&b.id));
    assigned_todos.dedup_by(|a, b| a.id == b.id);
    Ok(assigned_todos)
}

fn claim_branch_scoped_open_tasks(
    repo_root: &Path,
    agent_id: &str,
    current_branch: &str,
    assigned_todos: &mut Vec<AssignedTodoRef>,
) -> Result<(), DecapodError> {
    let main_repo = get_main_repo_root(repo_root)?;
    let store_root = main_repo.join(".decapod").join("data");
    let tasks = todo::list_tasks(
        &store_root,
        Some("open".to_string()),
        None,
        None,
        None,
        None,
    )?;
    for task in tasks {
        let todo_ref = AssignedTodoRef {
            id: task.id.clone(),
            hash: task.hash.clone(),
        };
        if !branch_contains_any_todo_id_or_hash(current_branch, std::slice::from_ref(&todo_ref)) {
            continue;
        }
        if task.assigned_to == agent_id {
            assigned_todos.push(todo_ref);
            continue;
        }
        if !task.assigned_to.is_empty() {
            return Err(DecapodError::ValidationError(format!(
                "AUTOREMEDIABLE_VALIDATION_ERROR code=WORKSPACE_BRANCH_TODO_CLAIM_CONFLICT severity=transient auto_remediable=true audience=agent agent_action=\"switch to the agent that owns todo {} or choose a different todo-scoped workspace; Decapod is preventing cross-work while preserving the captured todo\" user_note=\"This branch already belongs to another agent's Decapod todo claim; use the owner or a different todo-scoped workspace.\"\nBranch '{}' is scoped to todo {} but it is already claimed by {}.",
                task.id, current_branch, task.id, task.assigned_to
            )));
        }
        let claim = todo::claim_task(&store_root, &task.id, agent_id, todo::ClaimMode::Exclusive)?;
        if claim.get("status").and_then(|value| value.as_str()) != Some("ok") {
            return Err(DecapodError::ValidationError(format!(
                "AUTOREMEDIABLE_VALIDATION_ERROR code=WORKSPACE_BRANCH_TODO_CLAIM_CONFLICT severity=transient auto_remediable=true audience=agent agent_action=\"inspect `decapod todo list`; Decapod found the branch-scoped todo {} and needs its exclusive claim before container launch continues\" user_note=\"The branch todo is captured; resolve the claim conflict and rerun the workspace command.\"\n{}",
                task.id, claim
            )));
        }
        assigned_todos.push(todo_ref);
    }
    Ok(())
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
        DecapodError::ValidationError(format!(
            "AUTOREMEDIABLE_VALIDATION_ERROR code=WORKSPACE_STORAGE_PREFLIGHT_FAILED severity=transient auto_remediable=true audience=agent agent_action=\"verify .decapod/data directory is accessible and has correct permissions; if storage is full, free up space or use a different store root\" user_note=\"Workspace storage preflight failed; the agent should verify storage health or report the concrete blocker.\"\n{e}"
        ))
    })?;

    let mut status = get_workspace_status(repo_root)?;
    if status.git.is_protected && status.git.has_local_mods && !status.git.in_worktree {
        return Err(DecapodError::ValidationError(
            "AUTOREMEDIABLE_VALIDATION_ERROR code=WORKSPACE_INTERLOCK_DIRTY_PROTECTED severity=transient auto_remediable=true audience=agent agent_action=\"commit, stash, or discard local changes on the protected branch, then retry workspace creation\" user_note=\"Protected branch has local modifications; the agent should resolve this before creating an isolated worktree.\"\nprotected branch has local modifications. Agent must commit, stash, or discard changes before creating a Decapod worktree.".to_string(),
        ));
    }
    let upgrade_container = config.as_ref().map(|c| c.use_container).unwrap_or(false);
    let assigned_todos =
        ensure_assigned_open_tasks(repo_root, agent_id, &status.git.current_branch)?;

    // If we're already in a valid worktree, on todo-scoped branch, and no upgrade needed, we're good.
    // Relaxation for Issue #586: Allow non-scoped branches in worktrees if using external tracker
    // or if the project has explicitly opted into external tracker compatibility in config.toml.
    let allow_unscoped = !external_task_ref().is_empty() || is_external_tracker_config(repo_root);

    if status.git.in_worktree
        && !assigned_todos.is_empty()
        && !branch_contains_any_todo_id_or_hash(&status.git.current_branch, &assigned_todos)
        && !allow_unscoped
    {
        return Err(DecapodError::ValidationError(format!(
            "AUTOREMEDIABLE_VALIDATION_ERROR code=WORKSPACE_BRANCH_NOT_TODO_SCOPED severity=transient auto_remediable=true audience=agent agent_action=\"switch to a branch that includes one of the assigned todo IDs or hashes: {}\" user_note=\"Current branch is not todo-scoped; the agent should switch to a properly scoped branch or create one.\"\nCurrent worktree branch '{}' is not todo-scoped. Branch must include one of assigned todo IDs or hashes: {}.",
            render_todo_refs(&assigned_todos),
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
        if let Some(branch) = cfg.branch.as_ref()
            && !branch_contains_any_todo_id_or_hash(branch, &assigned_todos)
        {
            return Err(DecapodError::ValidationError(format!(
                "Requested branch '{}' must include an assigned todo ID/hash (one of: {}).",
                branch,
                render_todo_refs(&assigned_todos)
            )));
        }
        let branch = cfg.branch.unwrap_or_else(|| {
            let ts = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            format!(
                "agent/{}/{}-{}",
                sanitize_agent_id(agent_id),
                todo_scope,
                ts
            )
        });
        WorkspaceConfig {
            branch: Some(branch),
            use_container: cfg.use_container,
            base_image: cfg.base_image,
        }
    } else {
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        WorkspaceConfig {
            branch: Some(format!(
                "agent/{}/{}-{}",
                sanitize_agent_id(agent_id),
                todo_scope,
                ts
            )),
            use_container: false,
            base_image: None,
        }
    };
    let branch = config.branch.as_deref().ok_or_else(|| {
        DecapodError::ValidationError("workspace branch resolution failed".to_string())
    })?;

    // 1. Ensure git worktree
    let worktree_path = if status.git.in_worktree {
        repo_root.to_path_buf()
    } else {
        create_worktree(repo_root, branch, agent_id, &todo_scope)?
    };

    // 2. Ensure container (if requested)
    if config.use_container {
        ensure_dockerfile(&worktree_path)?;
        let image_tag = format!(
            "localhost/decapod-workspace:{}-{}",
            sanitize_agent_id(agent_id),
            branch.replace('/', "-")
        );
        build_workspace_image(&worktree_path, &image_tag)?;

        // Return blocker telling agent to enter container
        // We re-read status but override the blocker/container info
        let runtime = container_runtime::find_container_runtime()?;
        status = get_workspace_status(&worktree_path)?;
        status.blockers.push(Blocker {
            kind: BlockerKind::WorkspaceRequired,
            message: "Container environment prepared.".to_string(),
            resolve_hint: format!(
                "{} run -it -e DECAPOD_CONTAINER=1 -v {main_repo}:{main_repo} -w {} {} bash",
                runtime,
                worktree_path.display(),
                image_tag,
                main_repo = main_repo.display(),
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
                "Failed to create worktree: {stderr}"
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

FROM rust:1.91-slim

# Install essential tools
RUN apt-get update && apt-get install -y \
    git \
    curl \
    build-essential \
    pkg-config \
    libsqlite3-dev \
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
    let runtime = container_runtime::find_container_runtime()?;
    let output = Command::new(runtime)
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
            "Failed to build container image: {stderr}"
        )));
    }

    Ok(())
}

pub fn get_main_repo_root(current_dir: &Path) -> Result<PathBuf, DecapodError> {
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

    // Canonicalize to handle relative paths from git
    let common_path = if Path::new(&common_dir).is_absolute() {
        PathBuf::from(common_dir)
    } else {
        current_dir.join(common_dir)
    };

    let common_path = std::fs::canonicalize(&common_path).unwrap_or(common_path);

    // If common_path ends in .git, the root is its parent
    if common_path.file_name().and_then(|n| n.to_str()) == Some(".git") {
        return Ok(common_path.parent().unwrap_or(&common_path).to_path_buf());
    }

    Ok(common_path)
}

/// Discover the Decapod repository root by searching upwards from a directory.
///
/// If `start_dir` is None, it starts from the current working directory.
pub fn discover_repo_root(start_dir: Option<&Path>) -> Result<PathBuf, DecapodError> {
    let start = match start_dir {
        Some(p) => p.to_path_buf(),
        None => std::env::current_dir()?,
    };
    get_repo_root(&start)
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

fn is_external_tracker_config(repo_root: &Path) -> bool {
    let main_repo = get_main_repo_root(repo_root).unwrap_or_else(|_| repo_root.to_path_buf());
    let config_path = main_repo.join(".decapod").join("config.toml");
    if !config_path.exists() {
        return false;
    }
    match std::fs::read_to_string(config_path) {
        Ok(content) => content.contains("external_tracker = true"),
        Err(_) => false,
    }
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

pub fn is_worktree(repo_root: &Path) -> Result<bool, DecapodError> {
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
            "-z",
        ])
        .output()
        .map_err(DecapodError::IoError)?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut saw_non_ignorable = false;
    for entry in stdout.split('\0').filter(|entry| !entry.is_empty()) {
        if entry.len() < 4 {
            continue;
        }
        let path = &entry[3..];
        if path == ".decapod/OVERRIDE.md" {
            continue;
        }
        saw_non_ignorable = true;
        break;
    }

    Ok(saw_non_ignorable)
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
        return format!("todo-{head}");
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
                    "Failed to commit: {stderr}"
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
        .filter_map(|m| m.ok())
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

/// Workspace pruned record
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PrunedWorkspace {
    /// Absolute path of the pruned workspace
    pub path: String,
    /// Reason for pruning: not_registered, branch_deleted, no_matching_task, task_completed, no_active_claim
    pub reason: String,
}

/// Prune stale/unused agent workspaces
pub fn prune_workspaces(
    repo_root: &Path,
    force: bool,
) -> Result<Vec<PrunedWorkspace>, DecapodError> {
    let main_repo = get_main_repo_root(repo_root)?;
    let workspaces_dir = main_repo.join(".decapod").join("workspaces");
    if !workspaces_dir.is_dir() {
        return Ok(vec![]);
    }

    // 1) Parse all current git worktrees
    let worktrees_output = Command::new("git")
        .args([
            "-C",
            main_repo.to_str().unwrap_or("."),
            "worktree",
            "list",
            "--porcelain",
        ])
        .output()
        .map_err(DecapodError::IoError)?;

    if !worktrees_output.status.success() {
        return Err(DecapodError::ValidationError(format!(
            "Failed to list git worktrees: {}",
            String::from_utf8_lossy(&worktrees_output.stderr)
        )));
    }

    struct WorktreeInfo {
        path: PathBuf,
        branch: Option<String>,
        _head: Option<String>,
    }

    let mut worktrees = Vec::new();
    let mut current_path = None;
    let mut current_branch = None;
    let mut current_head = None;

    for line in String::from_utf8_lossy(&worktrees_output.stdout).lines() {
        if let Some(p) = line.strip_prefix("worktree ") {
            if let Some(path) = current_path.take() {
                worktrees.push(WorktreeInfo {
                    path,
                    branch: current_branch.take(),
                    _head: current_head.take(),
                });
            }
            current_path = Some(PathBuf::from(p.trim()));
        } else if let Some(b) = line.strip_prefix("branch ") {
            current_branch = Some(b.trim().to_string());
        } else if let Some(h) = line.strip_prefix("HEAD ") {
            current_head = Some(h.trim().to_string());
        }
    }
    if let Some(path) = current_path {
        worktrees.push(WorktreeInfo {
            path,
            branch: current_branch,
            _head: current_head,
        });
    }

    // 2) Get all tasks from todo database
    let store_root = main_repo.join(".decapod").join("data");
    let tasks = if store_root.exists() {
        todo::list_tasks(&store_root, None, None, None, None, None).unwrap_or_default()
    } else {
        vec![]
    };

    let mut pruned = Vec::new();

    // 3) Iterate over the directory entries under .decapod/workspaces/
    for entry in std::fs::read_dir(&workspaces_dir).map_err(DecapodError::IoError)? {
        let entry = entry.map_err(DecapodError::IoError)?;
        let dir_path = entry.path();
        if !dir_path.is_dir() {
            continue;
        }

        // Safety Safeguard: Do not prune if repo_root is inside or equal to dir_path.
        let normalized_dir = normalize_path_for_compare(&dir_path);
        let normalized_repo = normalize_path_for_compare(repo_root);
        if normalized_repo == normalized_dir || repo_root.starts_with(&dir_path) {
            continue;
        }

        let dir_name = dir_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string();

        // Check if registered as a worktree in git
        let matching_wt = worktrees
            .iter()
            .find(|wt| normalize_path_for_compare(&wt.path) == normalized_dir);

        let mut is_stale = false;
        let mut prune_reason = String::new();

        if let Some(wt) = matching_wt {
            // Check branch existence
            if let Some(ref_name) = &wt.branch {
                let show_ref_out = Command::new("git")
                    .args([
                        "-C",
                        main_repo.to_str().unwrap_or("."),
                        "show-ref",
                        "--verify",
                        ref_name,
                    ])
                    .output();

                let branch_exists = match show_ref_out {
                    Ok(out) => out.status.success(),
                    Err(_) => false,
                };

                if !branch_exists {
                    is_stale = true;
                    prune_reason = "branch_deleted".to_string();
                } else {
                    // Branch exists, check matching tasks
                    let mut matched_tasks = Vec::new();
                    for t in &tasks {
                        if !t.hash.is_empty() {
                            let hash_lower = t.hash.to_lowercase();
                            let dir_lower = dir_name.to_lowercase();
                            let branch_lower = ref_name.to_lowercase();
                            if dir_lower.contains(&hash_lower) || branch_lower.contains(&hash_lower)
                            {
                                matched_tasks.push(t);
                            }
                        }
                    }

                    if matched_tasks.is_empty() {
                        is_stale = true;
                        prune_reason = "no_matching_task".to_string();
                    } else {
                        // Check status of matched tasks
                        let all_completed = matched_tasks
                            .iter()
                            .all(|t| t.status == "done" || t.status == "archived");
                        let no_active_claim =
                            matched_tasks.iter().all(|t| t.assigned_to.is_empty());

                        if all_completed {
                            is_stale = true;
                            prune_reason = "task_completed".to_string();
                        } else if no_active_claim {
                            is_stale = true;
                            prune_reason = "no_active_claim".to_string();
                        }
                    }
                }
            } else {
                // No branch associated (detached HEAD) -> if no task matches it, prune it
                let mut matched_tasks = Vec::new();
                for t in &tasks {
                    if !t.hash.is_empty() {
                        let hash_lower = t.hash.to_lowercase();
                        let dir_lower = dir_name.to_lowercase();
                        if dir_lower.contains(&hash_lower) {
                            matched_tasks.push(t);
                        }
                    }
                }
                if matched_tasks.is_empty() {
                    is_stale = true;
                    prune_reason = "no_matching_task".to_string();
                } else {
                    let all_completed = matched_tasks
                        .iter()
                        .all(|t| t.status == "done" || t.status == "archived");
                    let no_active_claim = matched_tasks.iter().all(|t| t.assigned_to.is_empty());
                    if all_completed {
                        is_stale = true;
                        prune_reason = "task_completed".to_string();
                    } else if no_active_claim {
                        is_stale = true;
                        prune_reason = "no_active_claim".to_string();
                    }
                }
            }
        } else {
            // Case A: Not registered in git worktrees
            is_stale = true;
            prune_reason = "not_registered".to_string();
        }

        if is_stale {
            // Attempt to remove git worktree if registered
            if matching_wt.is_some() {
                let mut args = vec!["worktree", "remove"];
                if force {
                    args.push("--force");
                }
                args.push(dir_path.to_str().unwrap_or("."));

                let _ = Command::new("git")
                    .args(["-C", main_repo.to_str().unwrap_or(".")])
                    .args(&args)
                    .output();
            }

            // Fallback: forcefully remove from disk if it still exists
            if dir_path.exists() {
                let _ = std::fs::remove_dir_all(&dir_path);
            }

            pruned.push(PrunedWorkspace {
                path: dir_path.to_string_lossy().to_string(),
                reason: prune_reason,
            });
        }
    }

    // Call prune_stale_worktree_config to scrub .git/config entries
    let _ = prune_stale_worktree_config(repo_root);

    Ok(pruned)
}
