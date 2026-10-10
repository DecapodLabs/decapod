//! Workspace management with Git Worktree and Docker isolation
//!
//! Provides repository isolation primitives:
//! - git worktree status and provisioning
//! - protected-branch safeguards
//! - optional containerized execution for reproducible builds

use crate::core::bounded_process::{BUILD_TIMEOUT, BoundedCommand, CONTROL_TIMEOUT};
use crate::core::container_runtime;
use crate::core::db;
use crate::core::entrypoint_integrity;
use crate::core::error::DecapodError;
use crate::core::path_policy;
use crate::core::project_specs;
use crate::core::research_claims;
use crate::core::rpc::{AllowedOp, Blocker, BlockerKind};
use crate::core::todo;
use crate::core::trajectory;
use crate::core::workunit::{self, WorkUnitStatus};
use crate::plan_governance;
use crate::plugins::container;
use crate::plugins::eval;
use fancy_regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::env;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

#[path = "workspace_publication.rs"]
mod publication;

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
    /// Dirty source files are retained in place; workspaces use committed Git objects only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub root_isolation: Option<RootIsolation>,
}

/// Read-only source inventory shared by status and workspace creation.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RootIsolation {
    pub strategy: String,
    pub source_root: PathBuf,
    pub files: Vec<crate::core::dirty_classification::DirtyFile>,
}

fn root_isolation(repo_root: &Path) -> Result<RootIsolation, DecapodError> {
    Ok(RootIsolation {
        strategy: "committed_base_only_preserve_source".to_string(),
        source_root: repo_root.to_path_buf(),
        files: crate::core::dirty_classification::classify_isolation(repo_root)
            .map_err(DecapodError::IoError)?,
    })
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

/// The single tracked governance authority required for every published PR.
/// Logical sections are validated independently; authored commits are covered by
/// cumulative current-PR checkpoints, not by duplicated physical artifacts.
pub const REQUIRED_PR_GOVERNANCE_ARTIFACTS: &[&str] =
    &[crate::core::governance_document::GOVERNANCE_PATH];

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
    /// Repository base branch used when creating the worktree.
    #[serde(default)]
    pub base_branch: Option<String>,
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
        .bounded_output(CONTROL_TIMEOUT)
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
        .bounded_output(CONTROL_TIMEOUT)
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
            .bounded_output(CONTROL_TIMEOUT)
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
            .bounded_output(CONTROL_TIMEOUT)
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
    let root_isolation = git
        .is_main_repo
        .then(|| root_isolation(repo_root))
        .transpose()?;

    Ok(WorkspaceStatus {
        can_work,
        git,
        container,
        blockers,
        required_actions,
        root_isolation,
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

    // Identity is the Git checkout path, not the branch name or DECAPOD_* env.
    // Only a Decapod-owned path under `.decapod/workspaces/` is isolated.
    let is_main_repo = !is_canonical_decapod_workspace_path(repo_root);

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

/// Refresh release-bound projections immediately after a workspace is created.
/// This keeps a new worktree aligned with the Decapod binary that will govern it
/// without rewriting agent-authored specification prose.
fn refresh_workspace_release_surfaces(worktree_path: &Path) -> Result<(), DecapodError> {
    entrypoint_integrity::refresh_entrypoint_metadata(worktree_path)?;
    let config = crate::cli::DecapodProjectConfig::load(worktree_path)?;
    project_specs::refresh_specs_from_codebase(worktree_path, &config.repo.capabilities)?;
    container::refresh_managed_dockerfile_release(worktree_path)?;
    Ok(())
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
    let matching_tasks = tasks
        .iter()
        .filter(|task| {
            let candidate = AssignedTodoRef {
                id: task.id.clone(),
                hash: task.hash.clone(),
            };
            branch_contains_any_todo_id_or_hash(current_branch, &[candidate])
        })
        .count();
    if matching_tasks > 1 {
        return Err(DecapodError::ValidationError(format!(
            "WORKSPACE_AMBIGUOUS_TASK_HASH: branch {current_branch:?} matches {matching_tasks} tasks; use one full task ID in the branch. No task ownership was changed."
        )));
    }
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
    verify_workspace_parent(&main_repo)?;
    let source_inventory = root_isolation(&main_repo)?;
    let store_root = main_repo.join(".decapod").join("data");
    db::storage_health_preflight(&store_root).map_err(|e| {
        DecapodError::ValidationError(format!(
            "AUTOREMEDIABLE_VALIDATION_ERROR code=WORKSPACE_STORAGE_PREFLIGHT_FAILED severity=transient auto_remediable=true audience=agent agent_action=\"verify .decapod/data directory is accessible and has correct permissions; if storage is full, free up space or use a different store root\" user_note=\"Workspace storage preflight failed; the agent should verify storage health or report the concrete blocker.\"\n{e}"
        ))
    })?;
    // Workspace creation is also an entry point for RPC callers, which may
    // bypass the normal command migration funnel. Reconcile the canonical
    // todo/policy schema before any branch/task lookup so a partial datastore
    // cannot surface an internal `no such table` error (#1295).
    todo::initialize_todo_db(&store_root).map_err(|e| {
        DecapodError::ValidationError(format!(
            "AUTOREMEDIABLE_VALIDATION_ERROR code=WORKSPACE_SCHEMA_REPAIR_FAILED severity=transient auto_remediable=true audience=agent agent_action=\"rerun workspace ensure after confirming .decapod/data is writable\" user_note=\"Decapod could not reconcile the canonical workspace datastore schema.\"\n{e}"
        ))
    })?;

    let mut status = get_workspace_status(repo_root)?;
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

    // An explicitly task-scoped branch must not change its destination when
    // another independent task is claimed by this coordinating agent.
    let scoped_todos: Vec<_> = assigned_todos
        .iter()
        .filter(|todo| {
            config
                .as_ref()
                .and_then(|cfg| cfg.branch.as_deref())
                .is_none_or(|branch| {
                    branch_contains_any_todo_id_or_hash(branch, std::slice::from_ref(*todo))
                })
        })
        .cloned()
        .collect();
    if config
        .as_ref()
        .and_then(|cfg| cfg.branch.as_deref())
        .is_some()
        && scoped_todos.len() > 1
    {
        return Err(DecapodError::ValidationError(
            "WORKSPACE_AMBIGUOUS_TASK_HASH: requested branch matches multiple tasks; select one full task ID without changing existing claims.".to_string()
        ));
    }
    let todo_scope = build_todo_scope_component(&scoped_todos);
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
            base_branch: cfg.base_branch,
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
            base_branch: None,
        }
    };
    let branch = config.branch.as_deref().ok_or_else(|| {
        DecapodError::ValidationError("workspace branch resolution failed".to_string())
    })?;

    // 1. Ensure git worktree
    let worktree_path = if status.git.in_worktree {
        repo_root.to_path_buf()
    } else {
        let base_branch = config
            .base_branch
            .as_deref()
            .map(str::to_string)
            .unwrap_or_else(|| resolve_base_branch(&main_repo, None));
        create_worktree(repo_root, branch, agent_id, &todo_scope, &base_branch)?
    };

    refresh_workspace_release_surfaces(&worktree_path)?;

    // 2. Ensure container (if requested)
    if config.use_container {
        let mut ownership =
            crate::core::workspace_lifecycle::acquire_registered(&main_repo, &worktree_path)?;
        let runtime = container_runtime::find_container_runtime()?;
        ownership.require_container(&runtime)?;
        crate::plugins::container::prepare_generated_container_profile(&worktree_path)?;
        let image_tag = workspace_image_tag(agent_id, branch);
        build_workspace_image(&worktree_path, &image_tag, &runtime)?;

        // Return blocker telling agent to enter container
        // We re-read status but override the blocker/container info
        status = get_workspace_status(&worktree_path)?;
        let container_command = container_workspace_launch_command(
            &main_repo,
            &worktree_path,
            &runtime,
            &image_tag,
            ownership.invocation(),
        )?;
        status.blockers.push(Blocker {
            kind: BlockerKind::WorkspaceRequired,
            message: "Container environment prepared; its launch command will seed a private local data-store snapshot.".to_string(),
            resolve_hint: container_command,
        });
        status
            .required_actions
            .push("Enter containerized workspace".to_string());
        status.root_isolation = Some(source_inventory);
        return Ok(status);
    }

    // Return the exact pre-setup ownership inventory, including ignored paths.
    let mut result = get_workspace_status(&worktree_path)?;
    result.root_isolation = Some(source_inventory);
    Ok(result)
}

fn create_worktree(
    repo_root: &Path,
    branch: &str,
    agent_id: &str,
    todo_scope: &str,
    base_branch: &str,
) -> Result<PathBuf, DecapodError> {
    let main_repo = get_main_repo_root(repo_root)?;
    let workspaces_dir = main_repo.join(".decapod").join("workspaces");
    verify_workspace_parent(&main_repo)?;
    crate::core::fs_permissions::ensure_private_dir(&workspaces_dir)
        .map_err(DecapodError::IoError)?;

    let worktree_name = format!(
        "{}-{}-{}",
        sanitize_agent_id(agent_id),
        todo_scope,
        branch.replace('/', "-")
    );
    let worktree_path = workspaces_dir.join(&worktree_name);

    if std::fs::symlink_metadata(&worktree_path).is_ok() {
        if worktree_path.is_dir()
            && !std::fs::symlink_metadata(&worktree_path)?
                .file_type()
                .is_symlink()
            && registered_worktree_paths(&main_repo)?
                .contains(&normalize_path_for_compare(&worktree_path))
            && get_current_branch(&worktree_path)? == branch
        {
            return Ok(worktree_path);
        }
        match crate::core::workspace_lifecycle::acquire(&main_repo, &worktree_path)? {
            Some(owned)
                if !owned.is_ready()
                    && !owned.container_expected()
                    && std::fs::read_dir(&worktree_path)?.next().is_none() =>
            {
                owned.remove_empty()?
            }
            _ => {
                return Err(workspace_path_collision(
                    &worktree_path,
                    "existing_unowned_workspace_target",
                ));
            }
        }
    }

    // Prefer the fetched remote tip so claim/ensure does not snapshot a
    // stale local protected branch (GitHub #1259).
    fetch_base_branch_best_effort(&main_repo, base_branch);
    let start_point = base_ref_for_branch(&main_repo, base_branch).ok_or_else(|| {
        DecapodError::ValidationError(format!(
            "WORKSPACE_BASE_UNRESOLVED: committed base {base_branch:?} does not exist; fetch or select an existing committed base. Source files were preserved."
        ))
    })?;

    let mut ownership =
        crate::core::workspace_lifecycle::reserve(&main_repo, &worktree_path, false)?;

    // git worktree add <path> -b <branch> <base-ref>
    let mut args = vec![
        "-C".to_string(),
        main_repo.to_string_lossy().to_string(),
        "worktree".to_string(),
        "add".to_string(),
        "-b".to_string(),
        branch.to_string(),
        worktree_path.to_string_lossy().to_string(),
    ];
    args.push(start_point);
    let output = Command::new("git")
        .args(&args)
        .bounded_output(CONTROL_TIMEOUT)
        .map_err(DecapodError::IoError)?;

    if !output.status.success() {
        return Err(DecapodError::ValidationError(format!(
            "WORKSPACE_CREATE_FAILED: isolated workspace was not created; existing branches and source files were preserved. Choose an unused task branch or inspect the named registered workspace. {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }

    ownership.mark_ready()?;
    Ok(worktree_path)
}

fn workspace_path_collision(path: &Path, kind: &str) -> DecapodError {
    let file_type = std::fs::symlink_metadata(path)
        .map(|metadata| {
            if metadata.file_type().is_symlink() {
                "symlink"
            } else if metadata.is_dir() {
                "directory"
            } else if metadata.is_file() {
                "file"
            } else {
                "special"
            }
        })
        .unwrap_or("unavailable");
    let detail = serde_json::json!({
        "code": "WORKSPACE_PATH_OWNERSHIP_CONFLICT",
        "path": path,
        "ownership": "unverified",
        "type": kind,
        "file_type": file_type,
        "action": "preserve this path; choose another workspace or ask its owner to resolve the collision",
    });
    DecapodError::ValidationError(detail.to_string())
}

fn verify_workspace_parent(main_repo: &Path) -> Result<(), DecapodError> {
    for path in [
        main_repo.join(".decapod"),
        main_repo.join(".decapod/workspaces"),
    ] {
        match std::fs::symlink_metadata(&path) {
            Ok(meta) if meta.file_type().is_symlink() || !meta.is_dir() => {
                return Err(workspace_path_collision(
                    &path,
                    "non_directory_or_symlink_parent",
                ));
            }
            Ok(_) => {}
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
            Err(err) => return Err(DecapodError::IoError(err)),
        }
    }
    Ok(())
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
        .bounded_output(CONTROL_TIMEOUT)
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

fn process_is_inside_workspace(process_dir: Option<&Path>, workspace: &Path) -> bool {
    let Some(process_dir) = process_dir else {
        return false;
    };
    let process_dir =
        std::fs::canonicalize(process_dir).unwrap_or_else(|_| process_dir.to_path_buf());
    let workspace = std::fs::canonicalize(workspace).unwrap_or_else(|_| workspace.to_path_buf());
    process_dir.starts_with(workspace)
}

/// Build workspace container image
fn build_workspace_image(
    workspace_path: &Path,
    image_tag: &str,
    runtime: &str,
) -> Result<(), DecapodError> {
    let dockerfile_path = workspace_path
        .join(".decapod")
        .join("managed")
        .join("Dockerfile.decapod");
    let mut build = Command::new(runtime);
    build
        .arg("build")
        .arg("-t")
        .arg(image_tag)
        .arg("-f")
        .arg(dockerfile_path.to_str().unwrap_or("Dockerfile"))
        .arg("--build-arg")
        .arg(format!(
            "DECAPOD_WORKSPACE_PATH={}",
            workspace_path.display()
        ));
    if env_bool("DECAPOD_CONTAINER_LOCAL_BINARY_FALLBACK", false) {
        build
            .arg("--build-arg")
            .arg("DECAPOD_IMAGE=debian:bookworm-slim")
            .arg("--build-arg")
            .arg("DECAPOD_USE_LOCAL_BINARY=1");
    }
    let output = build
        .arg(workspace_path.to_str().unwrap_or("."))
        .bounded_output(BUILD_TIMEOUT)
        .map_err(DecapodError::IoError)?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(DecapodError::ValidationError(format!(
            "Failed to build container image: {stderr}"
        )));
    }

    Ok(())
}

/// Build a repeatable launch hint without opening the host's live WAL in the VM.
/// Shell quoting and runtime mount parsing are separate boundaries: reject path
/// delimiters that Docker/Podman would otherwise reinterpret inside mount flags.
fn container_workspace_launch_command(
    main_repo: &Path,
    worktree: &Path,
    runtime: &str,
    image_tag: &str,
    invocation: &str,
) -> Result<String, DecapodError> {
    fn mount_path(path: &Path) -> Result<&str, DecapodError> {
        path.to_str()
            .filter(|value| {
                path.is_absolute()
                    && !value.chars().any(|c| c.is_control() || matches!(c, ':' | ',' | '"'))
            })
            .ok_or_else(|| DecapodError::ValidationError(format!(
                "CONTAINER_STORE_PATH_UNSUPPORTED: '{}' must be an absolute UTF-8 path without colons, commas, double quotes, or control characters. Use a repository path supported by Docker/Podman mount arguments and retry.",
                path.display()
            )))
    }

    let repo = mount_path(main_repo)?;
    let workspace = mount_path(worktree)?;
    let store_root = main_repo.join(".decapod").join("data");
    let store_root = mount_path(&store_root)?;
    // mktemp runs when the hint is executed, not when it is printed. Each
    // invocation owns a fresh directory, even if the same hint is run twice.
    let snapshot_template = worktree.join("target/decapod-container-store.XXXXXX");
    let snapshot_template = mount_path(&snapshot_template)?;
    let cleanup = "result_code=$?; trap - EXIT; rm -f -- \"$snapshot_dir/decapod.db\"; rmdir -- \"$snapshot_dir\"; exit \"$result_code\"";
    Ok(format!(
        "( cd {workspace} && mkdir -p target || exit; snapshot_dir=$(mktemp -d {snapshot_template}) || exit; trap {cleanup} EXIT; trap 'exit 129' HUP; trap 'exit 130' INT; trap 'exit 143' TERM; decapod data database backup --destination \"$snapshot_dir/decapod.db\" && {runtime} run --rm -it --label org.decapod.managed=workspace --label {workspace_label} --label {invocation_label} -e DECAPOD_CONTAINER=1 -v {repo_mount} --mount \"type=bind,src=$snapshot_dir/decapod.db,dst=/tmp/decapod-store.db,readonly\" --tmpfs {store_tmpfs} -w {workspace} {image} sh -lc {seed_command} )",
        workspace = shell_quote(workspace),
        snapshot_template = shell_quote(snapshot_template),
        cleanup = shell_quote(cleanup),
        runtime = shell_quote(runtime),
        repo_mount = shell_quote(&format!("{repo}:{repo}")),
        workspace_label = shell_quote(&format!("org.decapod.workspace.path={workspace}")),
        invocation_label = shell_quote(&format!("org.decapod.invocation={invocation}")),
        store_tmpfs = shell_quote(&format!("{store_root}:rw,nosuid,nodev,mode=0700")),
        image = shell_quote(image_tag),
        seed_command = shell_quote(&format!(
            "cp -- /tmp/decapod-store.db {store_root}/decapod.db && exec bash",
            store_root = shell_quote(store_root),
        )),
    ))
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn env_bool(name: &str, default_value: bool) -> bool {
    match env::var(name) {
        Ok(v) => matches!(v.to_ascii_lowercase().as_str(), "1" | "true" | "yes" | "on"),
        Err(_) => default_value,
    }
}

/// Host checkout that owns a canonical `.decapod/workspaces/<name>` path.
///
/// Local-clone workspaces have their own `.git`, so `git-common-dir` cannot
/// see the parent. Walk path components instead (GitHub #1259).
pub fn host_repo_from_canonical_workspace(path: &Path) -> Option<PathBuf> {
    let canon = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let comps: Vec<_> = canon.components().collect();
    for i in 0..comps.len().saturating_sub(1) {
        if comps[i].as_os_str() == ".decapod" && comps[i + 1].as_os_str() == "workspaces" {
            let mut host = PathBuf::new();
            for component in &comps[..i] {
                host.push(component);
            }
            if host.as_os_str().is_empty() {
                return None;
            }
            return Some(host);
        }
    }
    None
}

/// Newest workspace directory whose name contains the task id (either ULID
/// form). Used so `--artifact` can resolve against the worktree that owns
/// the claim even when the command is run from the parent checkout.
pub fn find_workspace_for_task(host_repo: &Path, task_id: &str) -> Option<PathBuf> {
    let workspaces = host_repo.join(".decapod").join("workspaces");
    let entries = std::fs::read_dir(&workspaces).ok()?;
    let hyphenated = task_id.replace('_', "-");
    let mut matches = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let name = path.file_name()?.to_string_lossy();
        if name.contains(task_id) || name.contains(&hyphenated) {
            let modified = path
                .metadata()
                .ok()
                .and_then(|meta| meta.modified().ok())
                .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
            matches.push((modified, path));
        }
    }
    matches
        .into_iter()
        .max_by_key(|(modified, _)| *modified)
        .map(|(_, path)| path)
}

pub fn get_main_repo_root(current_dir: &Path) -> Result<PathBuf, DecapodError> {
    if let Some(host) = host_repo_from_canonical_workspace(current_dir) {
        return Ok(host);
    }
    let output = Command::new("git")
        .args([
            "-C",
            current_dir.to_str().unwrap_or("."),
            "rev-parse",
            "--git-common-dir",
        ])
        .bounded_output(CONTROL_TIMEOUT)
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
        .bounded_output(CONTROL_TIMEOUT)
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

/// Resolve the repository base branch used by workspace and publication paths.
pub fn resolve_base_branch(repo_root: &Path, explicit: Option<&str>) -> String {
    explicit
        .filter(|branch| !branch.trim().is_empty())
        .map(str::to_string)
        .or_else(|| {
            crate::cli::DecapodProjectConfig::load(repo_root)
                .ok()
                .and_then(|config| config.repo.base_branch)
                .filter(|branch| !branch.trim().is_empty())
        })
        .or_else(|| detect_base_branch(repo_root))
        .unwrap_or_else(|| "master".to_string())
}

/// Resolve a git ref usable in `git diff` / `git show` for the configured base.
///
/// Prefers `origin/<branch>` when the remote-tracking ref exists, otherwise the
/// local branch tip. Returns `None` when neither is available.
pub fn base_ref_for_branch(repo_root: &Path, branch: &str) -> Option<String> {
    let remote_ref = format!("refs/remotes/origin/{branch}");
    if git_ref_exists(repo_root, &remote_ref) {
        return Some(format!("origin/{branch}"));
    }
    let local_ref = format!("refs/heads/{branch}");
    git_ref_exists(repo_root, &local_ref).then(|| branch.to_string())
}

/// Best-effort `git fetch origin <base>` so workspace snapshots can start from
/// the remote tip rather than a stale local protected branch (GitHub #1259).
pub fn fetch_base_branch_best_effort(repo_root: &Path, base_branch: &str) {
    let dir = repo_root.to_str().unwrap_or(".");
    let _ = Command::new("git")
        .args([
            "-C",
            dir,
            "fetch",
            "--no-tags",
            "--update-head-ok",
            "origin",
            base_branch,
        ])
        .bounded_output(CONTROL_TIMEOUT);
}

/// SHA of the preferred workspace start point: `origin/<base>` after a
/// best-effort fetch, else the local base branch tip.
pub fn preferred_base_oid(repo_root: &Path, base_branch: &str) -> Option<String> {
    fetch_base_branch_best_effort(repo_root, base_branch);
    let dir = repo_root.to_str().unwrap_or(".");
    for candidate in [format!("origin/{base_branch}"), base_branch.to_string()] {
        let output = Command::new("git")
            .args(["-C", dir, "rev-parse", "--verify", &candidate])
            .bounded_output(CONTROL_TIMEOUT)
            .ok()?;
        if output.status.success() {
            let oid = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !oid.is_empty() {
                return Some(oid);
            }
        }
    }
    None
}

fn git_ref_exists(repo_root: &Path, git_ref: &str) -> bool {
    Command::new("git")
        .args([
            "-C",
            repo_root.to_str().unwrap_or("."),
            "show-ref",
            "--verify",
            "--quiet",
            git_ref,
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .bounded_output(CONTROL_TIMEOUT)
        .map(|output| output.status.success())
        .unwrap_or(false)
}

/// Check whether the current branch can merge the selected base without conflicts.
///
/// This is read-only: `git merge-tree` computes the result without changing the
/// worktree or index. A missing local base ref fails closed with setup guidance.
pub fn check_merge_conflicts(repo_root: &Path, base_branch: &str) -> Result<(), DecapodError> {
    let head_branch = current_branch(repo_root)?;
    check_merge_conflicts_for_branch(repo_root, base_branch, &head_branch)
}

/// Check a named local branch against the configured base without checking it out.
pub fn check_merge_conflicts_for_branch(
    repo_root: &Path,
    base_branch: &str,
    head_branch: &str,
) -> Result<(), DecapodError> {
    let base_ref = base_ref_for_branch(repo_root, base_branch).ok_or_else(|| {
        DecapodError::ValidationError(format!(
            "AUTOREMEDIABLE_VALIDATION_ERROR code=PR_BASE_REF_MISSING severity=transient auto_remediable=true audience=agent agent_action=\"fetch the configured base branch, then rerun publication\" user_note=\"The configured PR base is not available in local Git metadata.\"\nCannot preflight merge conflicts: base branch '{base_branch}' is missing locally. Fetch origin/{base_branch} and retry."
        ))
    })?;
    let output = Command::new("git")
        .args([
            "-C",
            repo_root.to_str().unwrap_or("."),
            "merge-tree",
            "--write-tree",
            &base_ref,
            head_branch,
        ])
        .bounded_output(CONTROL_TIMEOUT)
        .map_err(DecapodError::IoError)?;
    if output.status.success() {
        return Ok(());
    }

    let details = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let details = details
        .lines()
        .filter(|line| !line.trim().is_empty())
        .take(20)
        .collect::<Vec<_>>()
        .join("\\n");
    Err(DecapodError::ValidationError(format!(
        "AUTOREMEDIABLE_VALIDATION_ERROR code=PR_MERGE_CONFLICT severity=blocking auto_remediable=true audience=agent agent_action=\"rebase or merge {base_branch} into the feature branch, resolve conflicts, rerun validation, and retry publication\" user_note=\"The feature branch cannot be safely proposed against the configured base branch.\"\nMerge conflict preflight failed for '{head_branch}' against '{base_ref}'.\n{details}"
    )))
}

fn current_branch(repo_root: &Path) -> Result<String, DecapodError> {
    let output = Command::new("git")
        .args([
            "-C",
            repo_root.to_str().unwrap_or("."),
            "branch",
            "--show-current",
        ])
        .bounded_output(CONTROL_TIMEOUT)
        .map_err(DecapodError::IoError)?;
    if !output.status.success() {
        return Err(DecapodError::ValidationError(format!(
            "failed determining current branch: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    let branch = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if branch.is_empty() {
        return Err(DecapodError::ValidationError(
            "Cannot preflight merge conflicts from a detached HEAD".to_string(),
        ));
    }
    Ok(branch)
}

/// Detect the repository's base branch from local Git metadata.
///
/// `origin/HEAD` is the strongest local signal because it is set from the
/// remote's advertised default branch. A protected checked-out branch and
/// existing local main/master refs provide deterministic offline fallbacks.
pub fn detect_base_branch(repo_root: &Path) -> Option<String> {
    let symbolic_head = Command::new("git")
        .args([
            "-C",
            repo_root.to_str().unwrap_or("."),
            "symbolic-ref",
            "--quiet",
            "--short",
            "refs/remotes/origin/HEAD",
        ])
        .bounded_output(CONTROL_TIMEOUT)
        .ok()
        .filter(|output| output.status.success())
        .map(|output| clean_branch_name(String::from_utf8_lossy(&output.stdout).trim()).to_string())
        .filter(|branch| !branch.is_empty());
    if symbolic_head.is_some() {
        return symbolic_head;
    }

    let current_branch = get_current_branch(repo_root).ok();
    if let Some(branch) = current_branch.filter(|branch| is_branch_protected(branch)) {
        return Some(branch);
    }

    for branch in ["main", "master"] {
        let reference = format!("refs/heads/{branch}");
        let exists = Command::new("git")
            .args([
                "-C",
                repo_root.to_str().unwrap_or("."),
                "show-ref",
                "--verify",
                "--quiet",
                &reference,
            ])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .bounded_output(CONTROL_TIMEOUT)
            .map(|output| output.status.success())
            .unwrap_or(false);
        if exists {
            return Some(branch.to_string());
        }
    }

    None
}

fn clean_branch_name(name: &str) -> &str {
    let mut s = name.trim();
    if s.starts_with('*') {
        s = s[1..].trim();
    }
    if let Some(suffix) = s.strip_prefix("remotes/origin/") {
        s = suffix;
    } else if let Some(suffix) = s.strip_prefix("remotes/") {
        s = suffix;
    } else if let Some(suffix) = s.strip_prefix("origin/") {
        s = suffix;
    }
    s
}

fn is_commit_in_protected_branch(main_repo: &Path, commit_hash: &str) -> bool {
    let output = Command::new("git")
        .args([
            "-C",
            main_repo.to_str().unwrap_or("."),
            "branch",
            "-a",
            "--contains",
            commit_hash,
        ])
        .bounded_output(CONTROL_TIMEOUT);

    let Ok(output) = output else {
        return false;
    };
    if !output.status.success() {
        return false;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        let clean = clean_branch_name(line);
        if is_branch_protected(clean) {
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
        .bounded_output(CONTROL_TIMEOUT)
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
            .bounded_output(CONTROL_TIMEOUT)
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
        .bounded_output(CONTROL_TIMEOUT)
        .map_err(DecapodError::IoError)?;

    let git_dir = String::from_utf8_lossy(&output.stdout).trim().to_string();
    // In a worktree, git-dir is usually <main-repo>/.git/worktrees/<name>
    // A local-clone workspace under .decapod/workspaces is also treated as a worktree context
    Ok(git_dir.contains("/worktrees/") || is_canonical_decapod_workspace_path(repo_root))
}

/// True when `path` is a Decapod-owned isolated workspace.
///
/// Walks path components for `.decapod/workspaces` so a stray filename or
/// environment variable cannot impersonate custody. GitHub #1255.
pub fn is_canonical_decapod_workspace_path(path: &Path) -> bool {
    let mut saw_decapod = false;
    for comp in path.components() {
        let seg = comp.as_os_str().to_string_lossy();
        if seg == ".decapod" {
            saw_decapod = true;
            continue;
        }
        if saw_decapod && seg == "workspaces" {
            return true;
        }
        saw_decapod = false;
    }
    false
}

/// Resolve the actual Git worktree root (`rev-parse --show-toplevel`).
///
/// Returns `None` when `start_dir` is not inside a Git repository. Callers
/// that mutate projections must not fall back to `DECAPOD_WORKSPACE` or the
/// current branch name.
pub fn git_toplevel(start_dir: &Path) -> Result<Option<PathBuf>, DecapodError> {
    match get_repo_root(start_dir) {
        Ok(path) => Ok(Some(path)),
        Err(DecapodError::ValidationError(message))
            if message.contains("Not in a git repository") =>
        {
            Ok(None)
        }
        Err(err) => Err(err),
    }
}

fn projection_workspace_required(toplevel: &Path) -> DecapodError {
    DecapodError::ValidationError(format!(
        "AUTOREMEDIABLE_VALIDATION_ERROR code=workspace_required severity=transient auto_remediable=true audience=agent agent_action=\"Run `decapod todo claim --id <task-id>` then `decapod workspace ensure`, cd into the reported `.decapod/workspaces/*` path, and retry. Do not mutate, stash, or reset files in the protected root checkout.\" user_note=\"Managed spec projections may be written only inside the claimed isolated Decapod workspace.\"\nworkspace_required: refusing to mutate `.decapod/managed/specs/*` outside an isolated Decapod workspace (git toplevel: {}). Protected root main/master checkouts stay untouched.",
        toplevel.display()
    ))
}

/// Fail closed unless `project_root`'s Git toplevel is a claimed isolated
/// Decapod workspace. Tests may set `DECAPOD_VALIDATE_SKIP_GIT_GATES`.
///
/// Non-git directories (scaffold fixtures) are allowed so `decapod init` and
/// unit tests can still write specs. GitHub #1255.
pub fn ensure_isolated_workspace_for_projection_mutation(
    project_root: &Path,
) -> Result<(), DecapodError> {
    if std::env::var("DECAPOD_VALIDATE_SKIP_GIT_GATES").is_ok() {
        return Ok(());
    }

    let Some(toplevel) = git_toplevel(project_root)? else {
        return Ok(());
    };

    if !is_canonical_decapod_workspace_path(&toplevel) {
        return Err(projection_workspace_required(&toplevel));
    }

    let status = get_workspace_status(&toplevel)?;
    if status.git.is_protected || status.git.is_main_repo || !status.git.in_worktree {
        return Err(projection_workspace_required(&toplevel));
    }
    Ok(())
}

fn has_local_modifications(repo_root: &Path) -> Result<bool, DecapodError> {
    let output = Command::new("git")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .args([
            "-C",
            repo_root.to_str().unwrap_or("."),
            "status",
            "--porcelain",
            "-z",
        ])
        .bounded_output(CONTROL_TIMEOUT)
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
    let embedded_ids: Vec<_> = extract_task_ids_from_branch(branch)
        .into_iter()
        .filter(|id| {
            id.rsplit_once('_')
                .is_some_and(|(_, suffix)| suffix.len() > 6)
        })
        .collect();
    if !embedded_ids.is_empty() {
        return todo_refs
            .iter()
            .any(|todo| embedded_ids.contains(&todo.id.to_lowercase()));
    }
    todo_refs.iter().any(|todo| {
        let id = &todo.id;
        let id_lower = id.to_lowercase();
        let id_sanitized = sanitize_todo_component(id);
        let hash_lower = todo.hash.to_lowercase();
        branch_lower.contains(&id_lower)
            || branch_lower.contains(&id_sanitized)
            || (!hash_lower.is_empty()
                && branch_lower
                    .split(['/', '-', '_'])
                    .any(|part| part == hash_lower))
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
    /// Exact pushed commit and final remote diff were read back.
    #[serde(default)]
    pub remote_verified: bool,
    /// A matching open PR and its complete diff were verified.
    #[serde(default)]
    pub pr_verified: bool,
    /// Non-blocking publication limitations; never substitutes for failed proof.
    #[serde(default)]
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PublishRemote {
    name: String,
    url: String,
}

fn is_network_remote_url(url: &str) -> bool {
    let trimmed = url.trim();
    if trimmed.is_empty()
        || trimmed.starts_with('/')
        || trimmed.starts_with("./")
        || trimmed.starts_with("../")
        || trimmed.starts_with("file://")
    {
        return false;
    }

    trimmed.contains("://") || trimmed.starts_with("git@")
}

fn github_repo_slug(url: &str) -> Option<String> {
    let trimmed = url.trim().trim_end_matches('/').trim_end_matches(".git");
    let path = if let Some(path) = trimmed.strip_prefix("git@github.com:") {
        path
    } else if let Some(path) = trimmed.strip_prefix("https://github.com/") {
        path
    } else if let Some(path) = trimmed.strip_prefix("http://github.com/") {
        path
    } else {
        trimmed.strip_prefix("ssh://git@github.com/")?
    };

    let mut parts = path.split('/');
    let owner = parts.next()?.trim();
    let repo = parts.next()?.trim();
    if owner.is_empty() || repo.is_empty() || parts.next().is_some() {
        return None;
    }
    Some(format!("{owner}/{repo}"))
}

fn git_remote_names(repo_root: &Path) -> Result<Vec<String>, DecapodError> {
    let dir = repo_root.to_str().unwrap_or(".");
    let remotes = Command::new("git")
        .args(["-C", dir, "remote"])
        .bounded_output(CONTROL_TIMEOUT)
        .map_err(DecapodError::IoError)?;
    if !remotes.status.success() {
        return Err(DecapodError::ValidationError(format!(
            "Cannot inspect git remotes: {}",
            String::from_utf8_lossy(&remotes.stderr).trim()
        )));
    }
    Ok(String::from_utf8_lossy(&remotes.stdout)
        .lines()
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_string)
        .collect())
}

fn git_remote_url(repo_root: &Path, name: &str) -> Option<String> {
    let dir = repo_root.to_str().unwrap_or(".");
    let mut remote = Command::new("git")
        .args(["-C", dir, "remote", "get-url", "--push", name])
        .bounded_output(CONTROL_TIMEOUT)
        .ok()?;
    if !remote.status.success() {
        remote = Command::new("git")
            .args(["-C", dir, "remote", "get-url", name])
            .bounded_output(CONTROL_TIMEOUT)
            .ok()?;
    }
    if !remote.status.success() {
        return None;
    }
    let url = String::from_utf8_lossy(&remote.stdout).trim().to_string();
    (!url.is_empty()).then_some(url)
}

fn collect_network_remotes(repo_root: &Path) -> Result<Vec<PublishRemote>, DecapodError> {
    let mut candidates = Vec::new();
    for name in git_remote_names(repo_root)? {
        let Some(url) = git_remote_url(repo_root, &name) else {
            continue;
        };
        if is_network_remote_url(&url) {
            candidates.push(PublishRemote { name, url });
        }
    }
    Ok(candidates)
}

fn pick_network_remote(candidates: &[PublishRemote]) -> Option<PublishRemote> {
    candidates
        .iter()
        .find(|remote| remote.name == "origin")
        .cloned()
        .or_else(|| {
            candidates
                .iter()
                .find(|remote| remote.name == "upstream")
                .cloned()
        })
        .or_else(|| candidates.first().cloned())
}

fn local_remote_paths(repo_root: &Path) -> Result<Vec<PathBuf>, DecapodError> {
    let mut paths = Vec::new();
    for name in git_remote_names(repo_root)? {
        let Some(url) = git_remote_url(repo_root, &name) else {
            continue;
        };
        if is_network_remote_url(&url) {
            continue;
        }
        let path = PathBuf::from(url.trim_start_matches("file://"));
        if path.exists() {
            paths.push(path);
        }
    }
    Ok(paths)
}

fn ensure_git_remote(repo_root: &Path, name: &str, url: &str) -> Result<(), DecapodError> {
    let dir = repo_root.to_str().unwrap_or(".");
    if git_remote_url(repo_root, name).as_deref() == Some(url) {
        return Ok(());
    }
    if git_remote_url(repo_root, name).is_some() {
        return Ok(());
    }
    let output = Command::new("git")
        .args(["-C", dir, "remote", "add", name, url])
        .bounded_output(CONTROL_TIMEOUT)
        .map_err(DecapodError::IoError)?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if !stderr.contains("already exists") {
            return Err(DecapodError::ValidationError(format!(
                "Cannot add publish remote '{name}': {}",
                stderr.trim()
            )));
        }
    }
    Ok(())
}

/// Copy network remotes from a parent checkout onto a local-clone workspace
/// as `upstream` (origin stays the parent path).
pub(crate) fn inherit_network_remotes_from_parent(
    parent: &Path,
    workspace: &Path,
) -> Result<Option<PublishRemote>, DecapodError> {
    let Some(remote) = pick_network_remote(&collect_network_remotes(parent)?) else {
        return Ok(None);
    };
    let name = if git_remote_url(workspace, "origin").is_some() {
        "upstream"
    } else {
        "origin"
    };
    ensure_git_remote(workspace, name, &remote.url)?;
    Ok(Some(PublishRemote {
        name: name.to_string(),
        url: remote.url,
    }))
}

fn resolve_publish_remote(repo_root: &Path) -> Result<PublishRemote, DecapodError> {
    if let Some(remote) = pick_network_remote(&collect_network_remotes(repo_root)?) {
        return Ok(remote);
    }

    // Local-clone workspaces inherit `origin` as a filesystem path. Walk that
    // parent (and the canonical host checkout) for a GitHub remote, then add
    // it as `upstream` so `workspace publish` can push (GitHub #1259 / #758).
    let mut parents = local_remote_paths(repo_root)?;
    if let Some(host) = host_repo_from_canonical_workspace(repo_root) {
        parents.push(host);
    }
    for parent in parents {
        if parent == repo_root {
            continue;
        }
        if let Some(remote) = inherit_network_remotes_from_parent(&parent, repo_root)? {
            return Ok(remote);
        }
    }

    Err(DecapodError::ValidationError(
        "Cannot publish: no network-capable git remote is configured. The workspace may inherit a local-clone origin; Decapod tried to copy a GitHub remote from the parent checkout and did not find one. Add the upstream GitHub remote before retrying, for example: git remote add upstream git@github.com:OWNER/REPO.git. No commit was pushed.".to_string(),
    ))
}

fn publish_push_failure(stderr: &str, branch: &str, remote: &str) -> String {
    // Transport diagnostics may contain credential-bearing URLs or headers.
    let detail = "Git did not confirm a successful push (raw transport output withheld)";
    let divergence = stderr.contains("non-fast-forward") || stderr.contains("fetch first");

    if divergence {
        format!(
            "Workspace publication requires a fast-forward push and Decapod never force-pushes. Git rejected {remote}/{branch} because the remote branch has diverged: {detail}\nRemediation: Do not run `git push --force` or `git push --force-with-lease`. Fetch the remote, inspect the divergence, reconcile it with a reviewed merge or rebase in this workspace, rerun `decapod validate`, and retry `decapod workspace publish`. Stop for human judgment if history would need to be rewritten."
        )
    } else {
        format!(
            "PUBLICATION_OUTCOME_UNKNOWN: {remote}/{branch}: {detail}. The server may have accepted the commit before acknowledgement was lost. Preserve the branch and inspect the exact push destination with `git ls-remote` before retrying `decapod workspace publish`; an idempotent retry verifies the remote commit and reuses a matching PR. Never delete the branch or force-push to recover. If history differs, fetch, reconcile, and rerun `decapod validate`."
        )
    }
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
    verify_trajectory_gate_for_publish(repo_root, &status.git.current_branch)?;
    verify_validation_artifacts_for_publish(repo_root)?;
    eval::verify_eval_gate_for_publish(&repo_root.join(".decapod").join("data"))?;

    // Resolve the actual network remote before committing. Decapod-created
    // worktrees can inherit a local-clone origin, which must never be
    // treated as publication to the upstream GitHub repository.
    let publish_remote = resolve_publish_remote(repo_root)?;
    let base_branch = resolve_base_branch(repo_root, None);

    let dir = repo_root.to_str().unwrap_or(".");

    // 2. Stage and commit any uncommitted changes
    if status.git.has_local_mods {
        let add_output = Command::new("git")
            .args(["-C", dir, "add", "-A"])
            .bounded_output(CONTROL_TIMEOUT)
            .map_err(DecapodError::IoError)?;
        if !add_output.status.success() {
            return Err(DecapodError::ValidationError(
                "Failed to stage publication changes. Inspect local Git state and retry; raw diagnostics are withheld to protect credentials.".into(),
            ));
        }

        // Refuse before creating history: a later checkpoint cannot repair an
        // already-authored commit that never recorded its own proof.
        crate::core::governance_document::verify_staged_checkpoint(repo_root)?;

        let commit_msg = title
            .as_deref()
            .unwrap_or("decapod: publish workspace changes");
        let commit_output = Command::new("git")
            .args(["-C", dir, "commit", "-m", commit_msg])
            .bounded_output(CONTROL_TIMEOUT)
            .map_err(DecapodError::IoError)?;
        if !commit_output.status.success() {
            let stderr = String::from_utf8_lossy(&commit_output.stderr);
            // Allow "nothing to commit" as non-fatal
            if !stderr.contains("nothing to commit") {
                return Err(DecapodError::ValidationError(
                    "Failed to commit publication changes. Inspect local hooks and Git state, revalidate, and retry; raw diagnostics are withheld to protect credentials.".into(),
                ));
            }
        }
    }

    ensure_validation_artifacts_staged(repo_root)?;
    ensure_required_governance_artifacts_in_pr(repo_root, &base_branch)?;
    ensure_material_specs_change_in_pr(repo_root, &base_branch)?;
    ensure_managed_spec_projections_in_pr(repo_root, &base_branch)?;
    publication::verify_spec_reviews(repo_root, &base_branch)?;

    // Get current commit hash
    let hash_output = Command::new("git")
        .args(["-C", dir, "rev-parse", "HEAD"])
        .bounded_output(CONTROL_TIMEOUT)
        .map_err(DecapodError::IoError)?;
    if !hash_output.status.success() {
        return Err(DecapodError::ValidationError(
            "Cannot publish: cannot resolve HEAD.".into(),
        ));
    }
    let commit_hash = String::from_utf8_lossy(&hash_output.stdout)
        .trim()
        .to_string();

    publication::verify_committed_bundle(repo_root, &commit_hash)?;
    verify_validation_artifacts_for_publish(repo_root)?;
    // Preflight against the same base that publication will use. This runs
    // before any push or PR creation and never mutates the worktree.
    check_merge_conflicts(repo_root, &base_branch)?;

    // 3. Push branch to the selected network remote.
    let push_output = Command::new("git")
        .args([
            "-C",
            dir,
            "push",
            "-u",
            "--",
            &publish_remote.url,
            &format!("{commit_hash}:refs/heads/{}", status.git.current_branch),
        ])
        .bounded_output(publication::NETWORK_TIMEOUT)
        .map_err(|_| DecapodError::ValidationError(
            "PUBLICATION_INCOMPLETE: push outcome is unknown; it may have reached the remote. Read back the target branch and retry publication safely. Do not force-push, delete the branch, or report success.".into()
        ))?;
    if !push_output.status.success() {
        return Err(DecapodError::ValidationError(publish_push_failure(
            &String::from_utf8_lossy(&push_output.stderr),
            &status.git.current_branch,
            &publish_remote.name,
        )));
    }

    // Read the exact push destination, not a possibly different fetch URL or
    // stale remote-tracking ref. A successful push alone is not publication proof.
    let proof = publication::verify_remote(
        repo_root,
        &publish_remote.url,
        &status.git.current_branch,
        &base_branch,
        &commit_hash,
    )
    .map_err(publication::after_push)?;
    let (pr_url, warnings) = if let Some(slug) =
        github_repo_slug(&publication::redact_remote(&publish_remote.url))
    {
        let url = publication::ensure_and_verify_pr(
            repo_root,
            &slug,
            &status.git.current_branch,
            &base_branch,
            &proof,
            title.as_deref().unwrap_or(&status.git.current_branch),
            description.as_deref().unwrap_or(""),
        )
        .map_err(publication::after_push)?;
        // Detect a branch/base movement while GitHub was serving the PR pages.
        publication::verify_remote_unchanged(
            repo_root,
            &publish_remote.url,
            &status.git.current_branch,
            &base_branch,
            &proof,
        )
        .map_err(publication::after_push)?;
        (Some(url), Vec::new())
    } else {
        (None, vec!["Branch verified remotely; PR verification is unavailable for this non-GitHub remote. This is not a publish-ready PR.".to_string()])
    };

    Ok(PublishResult {
        branch: status.git.current_branch,
        commit_hash,
        remote_url: publication::redact_remote(&publish_remote.url),
        pr_verified: pr_url.is_some(),
        pr_url,
        remote_verified: true,
        warnings,
    })
}

pub fn extract_task_ids_from_branch(branch: &str) -> Vec<String> {
    let re = Regex::new(r"(?i)(?:aiml|apis|appl|arch|bend|bugs|cicd|code|data|desn|devx|docs|feat|fend|infr|lang|perf|plat|proj|r|refa|reft|root|secu|spec|test|todo|tool)[_-][a-z0-9]{6,}").expect("static regex");
    let mut out: Vec<String> = re
        .find_iter(branch)
        .filter_map(|m| m.ok())
        .map(|m| m.as_str().replace('-', "_").to_lowercase())
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

/// Verify the durable publication binding for a task-owned branch.
///
/// Workunit manifests and context capsules remain useful local execution
/// material, but the checked-in trajectory cookie is the only promotion
/// record. Git history then provides the durable sequence of prior cookies.
pub fn verify_trajectory_gate_for_publish(
    repo_root: &Path,
    branch: &str,
) -> Result<(), DecapodError> {
    let task_ids = extract_task_ids_from_branch(branch);
    if task_ids.is_empty() {
        return Ok(());
    }

    let trajectory = trajectory::load_trajectory_cookie(repo_root)?.ok_or_else(|| {
        DecapodError::ValidationError(format!(
            "Cannot publish: missing trajectory cookie at {}.",
            trajectory::trajectory_cookie_path(repo_root).display()
        ))
    })?;

    if !trajectory.blockers.is_empty() {
        return Err(DecapodError::ValidationError(format!(
            "Cannot publish: trajectory '{}' has blockers: {}.",
            trajectory.run_id,
            trajectory.blockers.join("; ")
        )));
    }

    for task_id in task_ids {
        if trajectory.task_id.as_deref() != Some(task_id.as_str()) {
            return Err(DecapodError::ValidationError(format!(
                "Cannot publish: trajectory '{}' is bound to task {:?}, expected '{}'.",
                trajectory.run_id, trajectory.task_id, task_id
            )));
        }
    }

    match trajectory.proof_status {
        trajectory::TrajectoryProofStatus::Failed
        | trajectory::TrajectoryProofStatus::NoChecksRun
        | trajectory::TrajectoryProofStatus::Unavailable => {
            Err(DecapodError::ValidationError(format!(
                "Cannot publish: trajectory '{}' has insufficient proof status {:?}.",
                trajectory.run_id, trajectory.proof_status
            )))
        }
        trajectory::TrajectoryProofStatus::Passed | trajectory::TrajectoryProofStatus::Partial => {
            Ok(())
        }
    }
}

/// Publication is only valid when the receipt and trajectory are present and
/// cryptographically bound to one another. This closes the gap where a PR can
/// be created from code that passed validation while carrying stale or absent
/// proof artifacts.
pub fn verify_validation_artifacts_for_publish(repo_root: &Path) -> Result<(), DecapodError> {
    let trajectory = trajectory::load_trajectory_cookie(repo_root)?.ok_or_else(|| {
        DecapodError::ValidationError(format!(
            "Cannot publish: missing trajectory cookie at {}.",
            trajectory::trajectory_cookie_path(repo_root).display()
        ))
    })?;
    let receipt_value = crate::core::governance_document::read_section(repo_root, "validation")?
        .ok_or_else(|| {
            DecapodError::ValidationError(
                "Cannot publish: missing governance validation section; rerun `decapod validate`."
                    .into(),
            )
        })?;
    let receipt: crate::core::validate::ValidationReceipt = serde_json::from_value(receipt_value)
        .map_err(|error| {
        DecapodError::ValidationError(format!(
            "Cannot publish: invalid governance validation section: {error}"
        ))
    })?;
    receipt.validate_integrity()?;
    let active_epoch = crate::core::validation_epoch::active_validation_epoch(repo_root)?;
    if receipt.validation_epoch != active_epoch
        || receipt.repo_signal_fingerprint != project_specs::repo_signal_fingerprint(repo_root)?
        || receipt.decapod_release != entrypoint_integrity::RELEASE_VERSION
    {
        return Err(DecapodError::ValidationError(
            "STALE_PUBLICATION_VALIDATION: code, living specs, or evaluator changed since validation. Review the affected specs, refresh through `decapod rpc --op specs.refresh`, and rerun `decapod validate`.".into(),
        ));
    }
    // Receipt commits may follow the validated commit, but a receipt from an
    // unrelated history (or an unresolved revision) cannot prove this branch.
    if !matches!(receipt.git_revision.len(), 40 | 64)
        || !receipt.git_revision.bytes().all(|b| b.is_ascii_hexdigit())
    {
        return Err(DecapodError::ValidationError("STALE_PUBLICATION_REVISION: validation must name a resolved commit. Rerun `decapod validate`.".into()));
    }
    let ancestor = Command::new("git")
        .current_dir(repo_root)
        .args(["merge-base", "--is-ancestor", &receipt.git_revision, "HEAD"])
        .bounded_output(CONTROL_TIMEOUT)
        .map_err(DecapodError::IoError)?;
    if !ancestor.status.success() {
        return Err(DecapodError::ValidationError("STALE_PUBLICATION_REVISION: validation is not bound to this commit history. Rerun `decapod validate`.".into()));
    }
    let plan = plan_governance::load_plan(repo_root)?.ok_or_else(|| {
        DecapodError::ValidationError(
            "Cannot publish: missing governed plan; run `decapod govern plan init`.".into(),
        )
    })?;
    if !trajectory
        .task_id
        .as_ref()
        .is_some_and(|task| plan.todo_ids.contains(task))
    {
        return Err(DecapodError::ValidationError("STALE_PUBLICATION_TASK: plan and trajectory must bind the same current todo. Update the governed plan and rerun validation.".into()));
    }
    if !plan.human_questions.is_empty() || !plan.unresolved_contradictions.is_empty() {
        return Err(DecapodError::ValidationError("PUBLICATION_DECISION_REQUIRED: resolve the governed plan's human questions and contradictions before publication; do not infer approval.".into()));
    }
    if receipt.trajectory_run_id.as_deref() != Some(trajectory.run_id.as_str())
        || receipt.trajectory_artifact_hash.as_deref() != Some(trajectory.artifact_hash.as_str())
    {
        return Err(DecapodError::ValidationError(
            "Cannot publish: validation receipt is not bound to the current trajectory artifact."
                .to_string(),
        ));
    }
    for (path, present) in [
        (
            ".decapod/governance.json#/sections/plan",
            plan_governance::load_plan(repo_root)?.is_some(),
        ),
        (
            ".decapod/governance.json#/claims",
            research_claims::load_and_validate(repo_root)?.is_some(),
        ),
    ] {
        if !present {
            return Err(DecapodError::ValidationError(format!(
                "Cannot publish: required governance artifact is missing or invalid: {path}. Run `decapod govern artifacts inventory --repair`, then rerun the inventory command before publication."
            )));
        }
    }
    if crate::core::governance_document::read_section(repo_root, "jev")?.is_some() {
        let ledger = crate::core::jev_history::load_and_validate(repo_root)?.ok_or_else(|| {
            DecapodError::ValidationError(
                "Jev ledger disappeared during publish verification".to_string(),
            )
        })?;
        if ledger.trajectory_run_id != trajectory.run_id {
            return Err(DecapodError::ValidationError(
                "Cannot publish: Jev observation ledger is not bound to the current trajectory."
                    .to_string(),
            ));
        }
    }
    Ok(())
}

fn ensure_validation_artifacts_staged(repo_root: &Path) -> Result<(), DecapodError> {
    let dir = repo_root.to_str().unwrap_or(".");
    for path in REQUIRED_PR_GOVERNANCE_ARTIFACTS {
        let output = Command::new("git")
            .args([
                "-C",
                dir,
                "ls-files",
                "--cached",
                "--error-unmatch",
                "--",
                path,
            ])
            .bounded_output(CONTROL_TIMEOUT)
            .map_err(DecapodError::IoError)?;
        if !output.status.success() {
            return Err(DecapodError::ValidationError(format!(
                "Cannot publish: required validation artifact was not staged: {path}"
            )));
        }
    }
    let unstaged = Command::new("git")
        .current_dir(repo_root)
        .args(["diff", "--quiet", "--"])
        .args(REQUIRED_PR_GOVERNANCE_ARTIFACTS)
        .bounded_output(CONTROL_TIMEOUT)
        .map_err(DecapodError::IoError)?;
    if !unstaged.status.success() {
        return Err(DecapodError::ValidationError("UNSTAGED_GOVERNANCE_ARTIFACT: required artifact bytes differ from the index. Stage the current validated bundle before publication.".into()));
    }
    let jev_path = crate::core::jev_history::JEV_HISTORY_PATH;
    if repo_root.join(jev_path).is_file() {
        let output = Command::new("git")
            .args([
                "-C",
                dir,
                "ls-files",
                "--cached",
                "--error-unmatch",
                "--",
                jev_path,
            ])
            .bounded_output(CONTROL_TIMEOUT)
            .map_err(DecapodError::IoError)?;
        if !output.status.success() {
            return Err(DecapodError::ValidationError(format!(
                "Cannot publish: Jev observation ledger was not staged: {jev_path}"
            )));
        }
    }
    Ok(())
}

/// Require the normalized document in the PR delta and a valid checkpoint for
/// every authored commit. Receipt/trajectory binding is checked independently by
/// [`verify_validation_artifacts_for_publish`].
pub fn ensure_required_governance_artifacts_in_pr(
    repo_root: &Path,
    base_branch: &str,
) -> Result<(), DecapodError> {
    let base_ref = base_ref_for_branch(repo_root, base_branch).ok_or_else(|| {
        DecapodError::ValidationError(format!(
            "Cannot publish: base branch '{base_branch}' is not available locally; fetch it before checking required PR governance artifacts."
        ))
    })?;

    let inventory =
        crate::core::governance_artifacts::inventory(repo_root, Some(base_branch), false)?;
    let incomplete: Vec<String> = inventory
        .artifacts
        .iter()
        .filter(|artifact| !artifact.present || !artifact.valid)
        .map(|artifact| {
            let reason = if !artifact.present {
                "missing"
            } else {
                artifact.schema_error.as_deref().unwrap_or("invalid")
            };
            format!("{} ({reason})", artifact.path)
        })
        .collect();
    if !incomplete.is_empty() {
        return Err(DecapodError::ValidationError(format!(
            "Cannot publish: required governance artifacts are missing or invalid: {}. Run `decapod govern artifacts inventory --repair` then re-validate.",
            incomplete.join(", ")
        )));
    }

    // The document participates once in the PR delta. Checkpoint verification
    // below separately proves each authored commit, including intermediate work.
    let dir = repo_root.to_str().unwrap_or(".");
    let output = Command::new("git")
        .args([
            "-C",
            dir,
            "diff",
            "--name-only",
            &format!("{base_ref}...HEAD"),
            "--",
        ])
        .args(REQUIRED_PR_GOVERNANCE_ARTIFACTS)
        .bounded_output(CONTROL_TIMEOUT)
        .map_err(DecapodError::IoError)?;
    if !output.status.success() {
        return Err(DecapodError::ValidationError(format!(
            "Cannot publish: failed to inspect the PR diff against '{base_ref}': {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    let changed: std::collections::HashSet<String> = String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .filter(|path| !path.is_empty())
        .map(str::to_string)
        .collect();
    let missing: Vec<&str> = REQUIRED_PR_GOVERNANCE_ARTIFACTS
        .iter()
        .copied()
        .filter(|path| !changed.contains(*path))
        .collect();
    if !missing.is_empty() {
        return Err(DecapodError::ValidationError(format!(
            "Cannot publish: required governance artifacts are not included in the PR diff against '{base_ref}': {}. Every project PR must update the normalized governance document and preserve checkpoint coverage of each authored commit.",
            missing.join(", ")
        )));
    }

    crate::core::governance_document::verify_pr_checkpoints_for_target(
        repo_root,
        &base_ref,
        "HEAD",
        base_branch,
    )?;

    Ok(())
}

/// Require at least one material living-spec rewrite in the PR diff.
///
/// `decapod rpc --op specs.refresh` and fingerprint attestation updates are
/// necessary but insufficient: each PR must mutate authored prose under
/// `.decapod/managed/specs/*.md` (not only auto-generated attestation /
/// capability blocks). See GitHub #1183.
pub fn ensure_material_specs_change_in_pr(
    repo_root: &Path,
    base_branch: &str,
) -> Result<(), DecapodError> {
    let base_ref = base_ref_for_branch(repo_root, base_branch).ok_or_else(|| {
        DecapodError::ValidationError(format!(
            "Cannot publish: base branch '{base_branch}' is not available locally; fetch it before checking material living-spec rewrites."
        ))
    })?;

    // Same commit as base: nothing to publish as a PR delta.
    let dir = repo_root.to_str().unwrap_or(".");
    let same_commit = Command::new("git")
        .args(["-C", dir, "rev-parse", base_ref.as_str(), "HEAD"])
        .bounded_output(CONTROL_TIMEOUT)
        .map_err(DecapodError::IoError)?;
    if same_commit.status.success() {
        let stdout = String::from_utf8_lossy(&same_commit.stdout);
        let revs: Vec<&str> = stdout
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .collect();
        if revs.len() == 2 && revs[0] == revs[1] {
            return Ok(());
        }
    }

    let report = project_specs::material_specs_change_vs_base(repo_root, &base_ref)?;
    if report.has_material_change {
        return Ok(());
    }

    let fingerprint_only = if report.fingerprint_only_changed_paths.is_empty() {
        "none".to_string()
    } else {
        report.fingerprint_only_changed_paths.join(", ")
    };
    Err(DecapodError::ValidationError(format!(
        "Cannot publish: FINGERPRINT_ONLY_SPECS — living specs under .decapod/managed/specs/*.md have no material authored-content change versus '{base_ref}'. \
Fingerprint/attestation refresh alone is insufficient (observed fingerprint-only paths: {fingerprint_only}). \
Update at least one living spec (INTENT/ARCHITECTURE/INTERFACES/VALIDATION/SEMANTICS/OPERATIONS/SECURITY/README) with prose that reflects this PR's change, then re-run `decapod rpc --op specs.refresh` and `decapod validate`."
    )))
}

/// List paths changed in `base_ref...HEAD`.
pub fn pr_changed_paths(repo_root: &Path, base_ref: &str) -> Result<Vec<String>, DecapodError> {
    let dir = repo_root.to_str().unwrap_or(".");
    let output = Command::new("git")
        .args([
            "-C",
            dir,
            "diff",
            "--name-only",
            &format!("{base_ref}...HEAD"),
        ])
        .bounded_output(CONTROL_TIMEOUT)
        .map_err(DecapodError::IoError)?;
    if !output.status.success() {
        return Err(DecapodError::ValidationError(format!(
            "Cannot inspect PR diff against '{base_ref}': {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .filter(|path| !path.is_empty())
        .map(str::to_string)
        .collect())
}

fn path_is_managed_spec(path: &str) -> bool {
    path == project_specs::LOCAL_PROJECT_SPECS_DIR
        || path.starts_with(&format!("{}/", project_specs::LOCAL_PROJECT_SPECS_DIR))
}

fn path_is_code_related(path: &str) -> bool {
    matches!(
        crate::core::dirty_classification::classify_path(path, &[]),
        crate::core::dirty_classification::DirtyFileClass::UserAuthored
    )
}

/// When a PR changes implementation files, the managed spec bundle must travel
/// in the same diff. Projections generated on a later root-checkout cleanup
/// are the GitHub #1255 failure mode.
pub fn ensure_managed_spec_projections_in_pr(
    repo_root: &Path,
    base_branch: &str,
) -> Result<(), DecapodError> {
    let base_ref = base_ref_for_branch(repo_root, base_branch).ok_or_else(|| {
        DecapodError::ValidationError(format!(
            "Cannot publish: base branch '{base_branch}' is not available locally; fetch it before checking managed spec projections."
        ))
    })?;

    let dir = repo_root.to_str().unwrap_or(".");
    let same_commit = Command::new("git")
        .args(["-C", dir, "rev-parse", base_ref.as_str(), "HEAD"])
        .bounded_output(CONTROL_TIMEOUT)
        .map_err(DecapodError::IoError)?;
    if same_commit.status.success() {
        let stdout = String::from_utf8_lossy(&same_commit.stdout);
        let revs: Vec<&str> = stdout
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .collect();
        if revs.len() == 2 && revs[0] == revs[1] {
            return Ok(());
        }
    }

    let changed = pr_changed_paths(repo_root, &base_ref)?;
    let has_code = changed.iter().any(|path| path_is_code_related(path));
    if !has_code {
        return Ok(());
    }
    if changed.iter().any(|path| path_is_managed_spec(path)) {
        return Ok(());
    }

    Err(DecapodError::ValidationError(format!(
        "Cannot publish: code-related changes versus '{base_ref}' are missing the managed spec projections that belong in the same PR. \
Include `.decapod/managed/specs/*` (authored living-spec rewrite plus `decapod rpc --op specs.refresh`) in this workspace commit. \
Do not generate those files later from the protected root checkout (workspace_required / GitHub #1255)."
    )))
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
    /// Stable relative path or redacted external path of the pruned workspace.
    pub path: String,
    /// Reason for pruning: branch_deleted, no_matching_task, task_completed, no_active_claim, not_registered
    pub reason: String,
}

/// Workspace that was identified as stale but intentionally preserved.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SkippedWorkspace {
    /// Stable relative path or redacted external path of the preserved workspace.
    pub path: String,
    /// Why automatic cleanup was skipped.
    pub reason: String,
    /// Actionable explanation for the operator or agent.
    pub detail: String,
}

/// Result of a stale workspace cleanup pass.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WorkspacePruneReport {
    pub pruned: Vec<PrunedWorkspace>,
    pub skipped: Vec<SkippedWorkspace>,
}

/// Prune stale/unused agent workspaces
pub fn prune_workspaces(
    repo_root: &Path,
    force: bool,
) -> Result<Vec<PrunedWorkspace>, DecapodError> {
    Ok(prune_workspaces_report(repo_root, force)?.pruned)
}

/// Prune stale/unused agent workspaces and report candidates preserved by safety gates.
pub fn prune_workspaces_report(
    repo_root: &Path,
    force: bool,
) -> Result<WorkspacePruneReport, DecapodError> {
    let process_dir = std::env::current_dir().ok();
    prune_workspaces_report_with_process_dir(repo_root, force, process_dir.as_deref())
}

fn prune_workspaces_report_with_process_dir(
    repo_root: &Path,
    force: bool,
    process_dir: Option<&Path>,
) -> Result<WorkspacePruneReport, DecapodError> {
    let main_repo = get_main_repo_root(repo_root)?;
    let workspaces_dir = main_repo.join(".decapod").join("workspaces");
    if !workspaces_dir.is_dir() {
        return Ok(WorkspacePruneReport {
            pruned: vec![],
            skipped: vec![],
        });
    }

    let canonical_main = main_repo.canonicalize().map_err(DecapodError::IoError)?;
    if workspaces_dir
        .canonicalize()
        .map_err(DecapodError::IoError)?
        != canonical_main.join(".decapod").join("workspaces")
    {
        return Err(DecapodError::ValidationError(
            "WORKSPACE_OWNERSHIP_UNVERIFIED: managed workspace parent resolves outside the repository; no workspaces were removed".into(),
        ));
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
        .bounded_output(CONTROL_TIMEOUT)
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
        head: Option<String>,
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
                    head: current_head.take(),
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
            head: current_head,
        });
    }

    // 2) Get all tasks from todo database
    let store_root = main_repo.join(".decapod").join("data");
    let tasks = if store_root.exists() {
        todo::list_tasks(&store_root, None, None, None, None, None)?
    } else {
        vec![]
    };

    let mut pruned = Vec::new();
    let mut skipped = Vec::new();
    // 3) Iterate over the directory entries under .decapod/workspaces/
    for entry in std::fs::read_dir(&workspaces_dir).map_err(DecapodError::IoError)? {
        let entry = entry.map_err(DecapodError::IoError)?;
        let dir_path = entry.path();
        if !dir_path.is_dir() {
            continue;
        }
        let persisted_path =
            path_policy::normalize_persisted_path(repo_root, &dir_path.to_string_lossy());

        // Safety safeguard: never prune the workspace containing the process cwd.
        let normalized_dir = normalize_path_for_compare(&dir_path);
        let process_is_inside_candidate = process_is_inside_workspace(process_dir, &dir_path);
        if process_is_inside_candidate {
            skipped.push(SkippedWorkspace {
                path: persisted_path.clone(),
                reason: "current_workspace".to_string(),
                detail: "workspace contains the current process; rerun prune from the host checkout after leaving this workspace so Git can release the branch".to_string(),
            });
            continue;
        }

        let dir_name = dir_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string();

        // Local clones intentionally have their own .git and are absent from
        // git worktree list. An active claim protects them even during --force
        // recovery and even before registration has completed.
        let active_refs: Vec<AssignedTodoRef> = tasks
            .iter()
            .filter(|task| {
                !task.assigned_to.is_empty() && !matches!(task.status.as_str(), "done" | "archived")
            })
            .map(|task| AssignedTodoRef {
                id: task.id.clone(),
                hash: task.hash.clone(),
            })
            .collect();
        let active_claim = branch_contains_any_todo_id_or_hash(&dir_name, &active_refs);

        // Check if registered as a worktree in git
        let matching_wt = worktrees
            .iter()
            .find(|wt| normalize_path_for_compare(&wt.path) == normalized_dir);
        let ownership = match crate::core::workspace_lifecycle::acquire(&main_repo, &dir_path) {
            Ok(owned) => owned,
            Err(error) => {
                skipped.push(SkippedWorkspace {
                    path: persisted_path,
                    reason: "workspace_lifecycle_busy".into(),
                    detail: error.to_string(),
                });
                continue;
            }
        };
        if active_claim {
            // A producer-held invocation lease proves whether a container.run
            // caller still exists. Reconcile its abandoned container separately
            // while retaining every active-task workspace file. Interactive
            // launch hints do not hold that lease and are not reclaimed here.
            let recovery = ownership
                .as_ref()
                .filter(|owned| owned.producer_lease() && owned.container_expected())
                .map(|owned| {
                    owned.container_runtime().and_then(|runtime| {
                        container_runtime::remove_workspace_containers_for_invocation(
                            runtime,
                            &dir_path,
                            Some(owned.invocation()),
                        )
                    })
                });
            let detail = match recovery {
                Some(Ok(())) => "abandoned invocation container reconciled; active task files preserved".to_string(),
                Some(Err(error)) => format!("active task files preserved; abandoned container recovery remains blocked: {error}"),
                None => "workspace belongs to an active task claim; release or complete the claim before pruning".to_string(),
            };
            skipped.push(SkippedWorkspace {
                path: persisted_path,
                reason: "active_claim".into(),
                detail,
            });
            continue;
        }
        // --force relaxes cleanliness, never ownership. A name alone, an
        // arbitrary Dockerfile, or a symlink into another checkout is not a
        // registration. Even a full completed-task ID in the directory name
        // is not evidence that Decapod owns the files within it.
        if entry
            .file_type()
            .map_err(DecapodError::IoError)?
            .is_symlink()
            || (matching_wt.is_none() && ownership.is_none())
        {
            skipped.push(SkippedWorkspace {
                path: persisted_path, reason: "unregistered_workspace".into(),
                detail: "workspace ownership is unverified; --force does not authorize deleting unrelated directories. Inspect and preserve or remove this directory explicitly".into(),
            });
            continue;
        }

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
                        "--quiet",
                        ref_name,
                    ])
                    .bounded_output(CONTROL_TIMEOUT);

                let branch_exists = match show_ref_out {
                    Ok(out) if out.status.success() => true,
                    Ok(out) if out.status.code() == Some(1) => false,
                    result => {
                        let detail = match result {
                            Ok(out) => String::from_utf8_lossy(&out.stderr).into_owned(),
                            Err(error) => error.to_string(),
                        };
                        skipped.push(SkippedWorkspace {
                            path: persisted_path.clone(),
                            reason: "branch_status_unavailable".into(),
                            detail: format!(
                                "could not establish branch state; workspace preserved: {detail}"
                            ),
                        });
                        continue;
                    }
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

                    let mut potential_stale = false;
                    if matched_tasks.is_empty() {
                        potential_stale = true;
                        prune_reason = "no_matching_task".to_string();
                    } else {
                        // Check status of matched tasks
                        let all_completed = matched_tasks
                            .iter()
                            .all(|t| t.status == "done" || t.status == "archived");
                        let no_active_claim =
                            matched_tasks.iter().all(|t| t.assigned_to.is_empty());

                        if all_completed {
                            potential_stale = true;
                            prune_reason = "task_completed".to_string();
                        } else if no_active_claim {
                            potential_stale = true;
                            prune_reason = "no_active_claim".to_string();
                        }
                    }

                    if potential_stale {
                        let is_merged = Command::new("git")
                            .args([
                                "-C",
                                main_repo.to_str().unwrap_or("."),
                                "rev-parse",
                                ref_name,
                            ])
                            .bounded_output(CONTROL_TIMEOUT)
                            .ok()
                            .filter(|o| o.status.success())
                            .map(|o| {
                                let commit_hash =
                                    String::from_utf8_lossy(&o.stdout).trim().to_string();
                                is_commit_in_protected_branch(&main_repo, &commit_hash)
                            })
                            .unwrap_or(false);
                        if is_merged {
                            is_stale = true;
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
                let mut potential_stale = false;
                if matched_tasks.is_empty() {
                    potential_stale = true;
                    prune_reason = "no_matching_task".to_string();
                } else {
                    let all_completed = matched_tasks
                        .iter()
                        .all(|t| t.status == "done" || t.status == "archived");
                    let no_active_claim = matched_tasks.iter().all(|t| t.assigned_to.is_empty());
                    if all_completed {
                        potential_stale = true;
                        prune_reason = "task_completed".to_string();
                    } else if no_active_claim {
                        potential_stale = true;
                        prune_reason = "no_active_claim".to_string();
                    }
                }

                if potential_stale {
                    let is_merged = wt.head.as_ref().is_some_and(|commit_hash| {
                        is_commit_in_protected_branch(&main_repo, commit_hash)
                    });
                    if is_merged {
                        is_stale = true;
                    }
                }
            }
        } else {
            // Case A: Not registered in git worktrees
            is_stale = true;
            prune_reason = "not_registered".to_string();
        }

        if is_stale {
            if !force {
                if matching_wt.is_none() {
                    skipped.push(SkippedWorkspace {
                        path: persisted_path.clone(),
                        reason: "unregistered_workspace".to_string(),
                        detail: "workspace directory is not registered with git; inspect it and rerun with --force only after preserving any needed files".to_string(),
                    });
                    continue;
                }

                match worktree_is_dirty(&dir_path) {
                    Ok(true) => {
                        skipped.push(SkippedWorkspace {
                            path: persisted_path.clone(),
                            reason: "dirty_workspace".to_string(),
                            detail: "workspace contains tracked or untracked changes; preserve or review them before rerunning with --force".to_string(),
                        });
                        continue;
                    }
                    Ok(false) => {}
                    Err(error) => {
                        skipped.push(SkippedWorkspace {
                            path: persisted_path.clone(),
                            reason: "workspace_status_unavailable".to_string(),
                            detail: format!(
                                "could not establish that the workspace is clean: {error}; rerun after inspection or use --force deliberately"
                            ),
                        });
                        continue;
                    }
                }
            }

            // A daemon-owned container can outlive an interrupted client.
            // Preserve the workspace and report recovery failure rather than
            // deleting data still mounted by a container.
            let container_cleanup = if let Some(owned) = ownership.as_ref() {
                if owned.container_expected() {
                    owned.container_runtime().and_then(|runtime| {
                        container_runtime::remove_workspace_containers_for_invocation(
                            runtime,
                            &dir_path,
                            Some(owned.invocation()),
                        )
                    })
                } else {
                    Ok(())
                }
            } else {
                reconcile_workspace_containers(
                    &dir_path,
                    Err(DecapodError::NotFound("runtime provenance absent".into())),
                )
            };
            if let Err(error) = container_cleanup {
                skipped.push(SkippedWorkspace {
                    path: persisted_path.clone(),
                    reason: "container_cleanup_failed".to_string(),
                    detail: format!("workspace preserved because container cleanup could not be verified: {error}"),
                });
                continue;
            }

            // Attempt to remove git worktree if registered
            if matching_wt.is_some() {
                let mut args = vec!["worktree", "remove"];
                if force {
                    args.push("--force");
                }
                args.push(dir_path.to_str().unwrap_or("."));

                let remove_output = Command::new("git")
                    .args(["-C", main_repo.to_str().unwrap_or(".")])
                    .args(&args)
                    .bounded_output(CONTROL_TIMEOUT);
                match remove_output {
                    Ok(output) if output.status.success() => {}
                    Ok(output) => {
                        skipped.push(SkippedWorkspace {
                            path: persisted_path.clone(),
                            reason: "git_remove_failed".to_string(),
                            detail: format!(
                                "git refused worktree removal: {}; no files were deleted",
                                String::from_utf8_lossy(&output.stderr).trim()
                            ),
                        });
                        continue;
                    }
                    Err(error) => {
                        skipped.push(SkippedWorkspace {
                            path: persisted_path.clone(),
                            reason: "git_remove_failed".to_string(),
                            detail: format!(
                                "could not invoke git for worktree removal: {error}; no files were deleted"
                            ),
                        });
                        continue;
                    }
                }
            }

            if std::env::var_os("DECAPOD_VALIDATE_SKIP_CONTAINER_CLEANUP").is_none()
                && container_runtime::container_runtime_available()
            {
                let _ = container_runtime::remove_workspace_images_for_path(&dir_path);
            }

            if dir_path.exists()
                && let Some(owned) = ownership.as_ref()
            {
                owned.verify()?;
            }
            // Fallback: forcefully remove from disk if it still exists
            if let Err(error) = std::fs::remove_dir_all(&dir_path)
                && error.kind() != std::io::ErrorKind::NotFound
            {
                skipped.push(SkippedWorkspace {
                    path: persisted_path,
                    reason: "workspace_remove_failed".into(),
                    detail: format!("workspace cleanup incomplete: {error}"),
                });
                continue;
            }

            pruned.push(PrunedWorkspace {
                path: persisted_path,
                reason: prune_reason,
            });
        }
    }

    // Call prune_stale_worktree_config to scrub .git/config entries
    let _ = prune_stale_worktree_config(repo_root);

    Ok(WorkspacePruneReport { pruned, skipped })
}

fn reconcile_workspace_containers(
    workspace: &Path,
    runtime: Result<String, DecapodError>,
) -> Result<(), DecapodError> {
    let _ = runtime;
    if workspace
        .join(container::MANAGED_DOCKERFILE_REL_PATH)
        .exists()
    {
        return Err(DecapodError::ValidationError(
            "Container runtime provenance is unknown; preserve the workspace and recover its owning engine explicitly.".into(),
        ));
    }
    // Legacy plain Git workspaces did not expose a managed container profile.
    Ok(())
}

// `all` recursively enumerates every untracked file. A validation pass only
// needs to know whether any untracked directory exists, so keep this check
// bounded by Git's collapsed directory-level status representation.
const WORKTREE_DIRTY_STATUS_ARGS: &[&str] =
    &["status", "--porcelain=1", "--untracked-files=normal"];

fn worktree_is_dirty(path: &Path) -> Result<bool, DecapodError> {
    let output = Command::new("git")
        .args(["-C", path.to_str().unwrap_or(".")])
        .args(WORKTREE_DIRTY_STATUS_ARGS)
        .bounded_output(CONTROL_TIMEOUT)
        .map_err(DecapodError::IoError)?;
    if !output.status.success() {
        return Err(DecapodError::ValidationError(format!(
            "git status failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    Ok(!output.stdout.is_empty())
}

fn workspace_image_tag(agent_id: &str, branch: &str) -> String {
    format!(
        "{}:{}-{}",
        container_runtime::DECAPOD_WORKSPACE_IMAGE_REPOSITORY,
        sanitize_agent_id(agent_id),
        branch.replace('/', "-")
    )
}
#[cfg(test)]
#[path = "../../../tests/unit/core/workspace_tests.rs"]
mod tests;
