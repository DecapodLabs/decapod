use crate::core::container_runtime;
use crate::core::error;
use crate::core::store::Store;
use crate::core::time;
use crate::core::workspace;
use clap::{Parser, Subcommand, ValueEnum};
use serde::Serialize;
use serde_json::json;
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

pub const MANAGED_DOCKERFILE_REL_PATH: &str = ".decapod/managed/Dockerfile.decapod";
const LEGACY_MANAGED_DOCKERFILE_REL_PATH: &str = ".decapod/managed/Dockerfile";

#[derive(Copy, Clone, Debug, Eq, PartialEq, ValueEnum)]
pub enum ImageProfile {
    DebianSlim,
    Alpine,
}

#[derive(Parser, Debug)]
#[clap(
    name = "container",
    about = "Run agent work in an ephemeral isolated Docker container"
)]
pub struct ContainerCli {
    #[clap(subcommand)]
    pub command: ContainerCommand,
}

#[derive(Subcommand, Debug)]
pub enum ContainerCommand {
    /// Execute one command in a fresh container against an isolated git worktree.
    Run {
        #[clap(long)]
        agent: String,
        #[clap(long)]
        cmd: String,
        #[clap(long)]
        branch: Option<String>,
        #[clap(long)]
        task_id: Option<String>,
        #[clap(long, default_value_t = false)]
        push: bool,
        #[clap(long, default_value_t = false)]
        pr: bool,
        #[clap(long)]
        pr_base: Option<String>,
        #[clap(long)]
        pr_title: Option<String>,
        #[clap(long)]
        pr_body: Option<String>,
        #[clap(long, value_enum, default_value = "debian-slim")]
        image_profile: ImageProfile,
        #[clap(long)]
        image: Option<String>,
        #[clap(long, default_value_t = 1800)]
        timeout_seconds: u64,
        #[clap(long, default_value = "2g")]
        memory: String,
        #[clap(long, default_value = "2.0")]
        cpus: String,
        #[clap(long)]
        repo: Option<String>,
        #[clap(long, default_value_t = false)]
        keep_worktree: bool,
        #[clap(long, default_value_t = true)]
        inherit_env: bool,
        #[clap(long, default_value_t = false)]
        local_only: bool,
    },
}

#[derive(Debug, Clone)]
struct DockerSpec {
    args: Vec<String>,
    container_name: String,
}

#[derive(Debug, Clone)]
struct WorkspaceSpec {
    branch: String,
    path: PathBuf,
    base_branch: String,
    backend: String,
}

fn auto_remediable_validation_error(
    code: &str,
    message: impl AsRef<str>,
    agent_action: &str,
) -> error::DecapodError {
    error::DecapodError::ValidationError(format!(
        "AUTOREMEDIABLE_VALIDATION_ERROR code={} severity=transient auto_remediable=true audience=agent agent_action=\"{}\" user_note=\"Recoverable validation issue; the agent should take this action or report the concrete blocker.\"\n{}",
        code,
        agent_action,
        message.as_ref()
    ))
}

fn classify_container_failure(stdout: &str, stderr: &str) -> (&'static str, &'static str) {
    let combined = format!("{stdout}\n{stderr}").to_lowercase();
    if combined.contains("permission denied")
        || combined.contains("operation not permitted")
        || combined.contains("index.lock")
    {
        (
            "container_workspace_permission_denied",
            "Agent: retry from a writable Decapod worktree; if host-user mapping is the blocker, ask before changing that isolation setting.",
        )
    } else if combined.contains("refusing to fetch into branch") {
        (
            "container_branch_sync_checked_out",
            "Agent: retry with a dedicated work branch that is not checked out in the host repository, then let Decapod fold the branch back.",
        )
    } else if combined.contains("invalid linker name") || combined.contains("-fuse-ld=lld") {
        (
            "rust_toolchain_linker_config",
            "Agent: clear RUSTFLAGS and set CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER=cc, or run inside the repo's Nix development shell.",
        )
    } else {
        (
            "container_command_failed",
            "Agent: inspect stdout/stderr, apply the relevant fix, and rerun the same Decapod container command.",
        )
    }
}

#[derive(Debug, Clone)]
pub struct RunSummary {
    pub value: serde_json::Value,
}

pub(crate) const CONTAINER_DISABLE_MARKER: &str = "DECAPOD_CONTAINER_RUNTIME_DISABLED=true";

pub(crate) enum ContainerRuntimeOverrideHeal {
    Cleared,
    Unchanged,
}

pub fn run_container_cli(store: &Store, cli: ContainerCli) -> Result<(), error::DecapodError> {
    let summary = match cli.command {
        ContainerCommand::Run {
            agent,
            cmd,
            branch,
            task_id,
            push,
            pr,
            pr_base,
            pr_title,
            pr_body,
            image_profile,
            image,
            timeout_seconds,
            memory,
            cpus,
            repo,
            keep_worktree,
            inherit_env,
            local_only,
        } => run_container(
            store,
            &agent,
            &cmd,
            branch.as_deref(),
            task_id.as_deref(),
            push,
            pr,
            pr_base.as_deref(),
            pr_title.as_deref(),
            pr_body.as_deref(),
            image_profile,
            image.as_deref(),
            timeout_seconds,
            &memory,
            &cpus,
            repo.as_deref(),
            keep_worktree,
            inherit_env,
            local_only,
        )?,
    };

    println!("{}", serde_json::to_string_pretty(&summary.value).unwrap());
    Ok(())
}

pub fn run_container_for_claim(
    store: &Store,
    agent: &str,
    task_id: &str,
    task_title: &str,
) -> Result<serde_json::Value, error::DecapodError> {
    let repo = repo_root_from_store(store)?;
    let cmd = std::env::var("DECAPOD_CLAIM_CMD")
        .unwrap_or_else(|_| "echo \"container initialized for claimed task\"".to_string());
    let branch = format!(
        "agent/{}/{}",
        sanitize_branch_component(agent),
        sanitize_branch_component(task_id)
    );

    let push = env_bool("DECAPOD_CLAIM_PUSH", false);
    let pr = env_bool("DECAPOD_CLAIM_PR", false);
    let keep_worktree = env_bool("DECAPOD_CLAIM_KEEP_WORKTREE", true);
    let pr_title = std::env::var("DECAPOD_CLAIM_PR_TITLE")
        .ok()
        .or_else(|| Some(format!("{task_title} [{task_id}]")));
    let pr_body = std::env::var("DECAPOD_CLAIM_PR_BODY").ok().or_else(|| {
        Some(format!(
            "Automated container run for claimed task {task_id}"
        ))
    });

    let summary = run_container(
        store,
        agent,
        &cmd,
        Some(&branch),
        Some(task_id),
        push,
        pr,
        None,
        pr_title.as_deref(),
        pr_body.as_deref(),
        ImageProfile::DebianSlim,
        None,
        1800,
        "2g",
        "2.0",
        Some(repo.to_str().ok_or_else(|| {
            error::DecapodError::PathError("invalid repository path".to_string())
        })?),
        keep_worktree,
        true,
        false,
    )?;

    Ok(summary.value)
}

#[allow(clippy::too_many_arguments)]
fn run_container(
    _store: &Store,
    agent: &str,
    user_cmd: &str,
    branch: Option<&str>,
    task_id: Option<&str>,
    push: bool,
    pr: bool,
    pr_base: Option<&str>,
    pr_title: Option<&str>,
    pr_body: Option<&str>,
    image_profile: ImageProfile,
    image_override: Option<&str>,
    timeout_seconds: u64,
    memory: &str,
    cpus: &str,
    repo_override: Option<&str>,
    keep_worktree: bool,
    inherit_env: bool,
    local_only: bool,
) -> Result<RunSummary, error::DecapodError> {
    let repo = resolve_repo_path(repo_override)?;
    let docker = match container_runtime::find_container_runtime() {
        Ok(runtime) => runtime,
        Err(_) => {
            let message = "No container runtime found (docker/podman).\n\
Agent cannot use the required isolated workspace until Docker or Podman is available.\n\
Warning: without isolated containers, concurrent agents can step on each other.";
            return Err(auto_remediable_validation_error(
                "container_runtime_missing",
                message,
                "Agent: check Docker/Podman availability; if host installation or daemon startup is required, ask the user for that specific host action.",
            ));
        }
    };
    clear_container_runtime_override(&repo)?;
    if container_runtime_disabled(&repo)? {
        return Err(auto_remediable_validation_error(
            "container_runtime_override_disabled",
            "Container subsystem is disabled by .decapod/OVERRIDE.md even though a runtime is available. \
Agent must clear the disable marker through Decapod self-heal before retrying.",
            "Agent: run `decapod validate` once to allow self-heal, or remove the container disable marker through Decapod before retrying.",
        ));
    }

    ensure_container_runtime_access(&docker)?;

    let image = resolve_runtime_image(&docker, &repo, image_profile, image_override)?;

    let branch_name = branch
        .map(|s| s.to_string())
        .unwrap_or_else(|| default_branch_name(agent, task_id));
    let base_branch = resolve_base_branch(&repo, pr_base);
    let workspace = prepare_workspace_clone(&repo, &branch_name, &base_branch)?;

    let spec = build_docker_spec(
        &docker,
        &repo,
        &workspace.path,
        &image,
        agent,
        user_cmd,
        &workspace.branch,
        &workspace.base_branch,
        memory,
        cpus,
        task_id,
        inherit_env,
        local_only,
    )?;

    let start = Instant::now();
    let output = execute_container_with_timeout(&docker, &spec.args, timeout_seconds).map_err(
        |exec_err| {
            let sync_msg = if local_only {
                "branch foldback: skipped for local-only container run".to_string()
            } else {
                match sync_workspace_branch_to_host_repo(&repo, &workspace.path, &workspace.branch)
                {
                    Ok(_) => "branch foldback: synced to host repo after container termination"
                        .to_string(),
                    Err(sync_err) => format!(
                        "branch foldback: sync failed after container termination: {sync_err}"
                    ),
                }
            };
            if !keep_worktree {
                let _ = cleanup_workspace_clone(&workspace.path);
            }
            auto_remediable_validation_error(
                "container_runtime_terminated",
                format!(
                    "container runtime terminated before normal completion: {exec_err}\n{sync_msg}"
                ),
                "Agent: address the runtime or sync diagnostic below, then retry the same container command.",
            )
        },
    )?;
    let elapsed = start.elapsed().as_secs();

    let status = if output.status.success() {
        "ok"
    } else {
        "error"
    };
    let branch_returned_to_host = if local_only {
        false
    } else {
        sync_workspace_branch_to_host_repo(&repo, &workspace.path, &workspace.branch)?;
        true
    };

    if pr {
        workspace::check_merge_conflicts_for_branch(
            &repo,
            &workspace.base_branch,
            &workspace.branch,
        )?;
    }

    if push {
        push_branch_to_origin(&repo, &workspace.branch)?;
    }

    if pr {
        create_gh_pr(
            &repo,
            &workspace.branch,
            &workspace.base_branch,
            pr_title,
            pr_body,
        )?;
    }

    let summary = json!({
        "ts": time::now_epoch_z(),
        "cmd": "container.run",
        "status": status,
        "agent": agent,
        "runtime": docker,
        "image": image,
        "container_name": spec.container_name,
        "repo": repo,
        "workspace": workspace.path,
        "worktree": workspace.path,
        "branch": workspace.branch,
        "base_branch": workspace.base_branch,
        "isolation_backend": workspace.backend,
        "local_only": local_only,
        "task_id": task_id,
        "push": push,
        "pr": pr,
        "keep_worktree": keep_worktree,
        "branch_returned_to_host": branch_returned_to_host,
        "exit_code": output.status.code(),
        "elapsed_seconds": elapsed,
        "stdout": String::from_utf8_lossy(&output.stdout),
        "stderr": String::from_utf8_lossy(&output.stderr)
    });

    let cleanup_err = if keep_worktree {
        None
    } else {
        cleanup_workspace_clone(&workspace.path).err()
    };

    if !output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let (code, next_action) = classify_container_failure(&stdout, &stderr);
        return Err(auto_remediable_validation_error(
            code,
            format!(
                "Container command failed (exit {:?})\nstdout:\n{}\nstderr:\n{}",
                output.status.code(),
                stdout.trim(),
                stderr.trim()
            ),
            next_action,
        ));
    }
    if let Some(err) = cleanup_err {
        return Err(err);
    }

    Ok(RunSummary { value: summary })
}

fn resolve_base_branch(repo_root: &Path, explicit: Option<&str>) -> String {
    workspace::resolve_base_branch(repo_root, explicit)
}

fn execute_container_with_timeout(
    runtime: &str,
    args: &[String],
    timeout_seconds: u64,
) -> Result<std::process::Output, error::DecapodError> {
    let start = Instant::now();
    let mut child = Command::new(runtime)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(error::DecapodError::IoError)?;

    let timeout = Duration::from_secs(timeout_seconds);
    loop {
        if let Some(_status) = child.try_wait().map_err(error::DecapodError::IoError)? {
            return child
                .wait_with_output()
                .map_err(error::DecapodError::IoError);
        }
        if start.elapsed() > timeout {
            let _ = child.kill();
            return Err(auto_remediable_validation_error(
                "container_command_timeout",
                format!("Container command timed out after {timeout_seconds}s"),
                "Agent: increase --timeout-seconds for expected long runs, or inspect the command for a lock/deadlock before retrying.",
            ));
        }
        std::thread::sleep(Duration::from_millis(250));
    }
}

fn resolve_repo_path(repo_override: Option<&str>) -> Result<PathBuf, error::DecapodError> {
    let base = if let Some(path) = repo_override {
        PathBuf::from(path)
    } else {
        std::env::current_dir().map_err(error::DecapodError::IoError)?
    };
    base.canonicalize().map_err(error::DecapodError::IoError)
}

fn override_file_path(repo_root: &Path) -> PathBuf {
    repo_root.join(".decapod").join("OVERRIDE.md")
}

fn container_runtime_disabled(repo_root: &Path) -> Result<bool, error::DecapodError> {
    let path = override_file_path(repo_root);
    if !path.exists() {
        return Ok(false);
    }
    let content = fs::read_to_string(path).map_err(error::DecapodError::IoError)?;
    Ok(content.contains(CONTAINER_DISABLE_MARKER))
}

fn clear_container_runtime_override(repo_root: &Path) -> Result<bool, error::DecapodError> {
    let path = override_file_path(repo_root);
    if !path.exists() {
        return Ok(false);
    }
    let content = fs::read_to_string(&path).map_err(error::DecapodError::IoError)?;
    if !content.contains(CONTAINER_DISABLE_MARKER) {
        return Ok(false);
    }

    let lines: Vec<&str> = content.lines().collect();
    let marker_index = lines
        .iter()
        .position(|line| line.trim() == CONTAINER_DISABLE_MARKER)
        .ok_or_else(|| {
            error::DecapodError::ValidationError(
                "container override marker exists but could not be located".to_string(),
            )
        })?;

    let mut start = marker_index;
    while start > 0 {
        let candidate = lines[start - 1].trim();
        if candidate == "### plugins/CONTAINER" {
            start -= 1;
            break;
        }
        if candidate.is_empty() {
            start -= 1;
            continue;
        }
        break;
    }

    let mut end = marker_index + 1;
    while end < lines.len() {
        let candidate = lines[end].trim();
        if candidate.starts_with("reason:")
            || candidate.starts_with("remediation:")
            || candidate.starts_with("warning:")
            || candidate == "## Runtime Guard Override (auto-generated)"
            || candidate.is_empty()
        {
            end += 1;
            continue;
        }
        break;
    }

    let mut rebuilt: Vec<&str> = Vec::with_capacity(lines.len().saturating_sub(end - start));
    rebuilt.extend_from_slice(&lines[..start]);
    rebuilt.extend_from_slice(&lines[end..]);
    let mut cleaned = rebuilt.join("\n");
    if !cleaned.is_empty() {
        cleaned.push('\n');
    }
    fs::write(path, cleaned).map_err(error::DecapodError::IoError)?;
    Ok(true)
}

pub(crate) fn heal_container_runtime_override(
    repo_root: &Path,
) -> Result<ContainerRuntimeOverrideHeal, error::DecapodError> {
    match container_runtime::find_container_runtime() {
        Ok(runtime) if ensure_container_runtime_access(&runtime).is_ok() => {
            if clear_container_runtime_override(repo_root)? {
                Ok(ContainerRuntimeOverrideHeal::Cleared)
            } else {
                Ok(ContainerRuntimeOverrideHeal::Unchanged)
            }
        }
        _ => Ok(ContainerRuntimeOverrideHeal::Unchanged),
    }
}

fn repo_root_from_store(store: &Store) -> Result<PathBuf, error::DecapodError> {
    store
        .root
        .parent()
        .and_then(|p| p.parent())
        .map(Path::to_path_buf)
        .ok_or_else(|| {
            error::DecapodError::ValidationError(
                "unable to resolve repo root from store root".to_string(),
            )
        })
}

fn ensure_container_runtime_access(runtime: &str) -> Result<(), error::DecapodError> {
    let output = Command::new(runtime)
        .arg("info")
        .output()
        .map_err(error::DecapodError::IoError)?;
    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let combined = format!("{stderr}\n{stdout}").to_lowercase();
    let uid = current_uid_gid()
        .map(|(u, g)| format!("uid={u}, gid={g}"))
        .unwrap_or_else(|| "uid/gid unavailable".to_string());
    let docker_host = std::env::var("DOCKER_HOST").unwrap_or_else(|_| "<unset>".to_string());
    let xdg_runtime_dir =
        std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "<unset>".to_string());

    let remediation = if combined.contains("permission denied")
        || combined.contains("got permission denied")
        || combined.contains("operation not permitted")
    {
        "Container runtime access denied. Agent should request elevated runtime permission or ask for user runtime access if host policy blocks it."
    } else if combined.contains("cannot connect")
        || combined.contains("is the docker daemon running")
        || combined.contains("connection refused")
        || combined.contains("no such file or directory")
    {
        "Container runtime is installed but unavailable. Agent should verify daemon status and ask the user to start the host service only if needed."
    } else {
        "Runtime preflight failed. Agent should verify Docker/Podman daemon availability and user permissions, then retry."
    };

    Err(auto_remediable_validation_error(
        "container_runtime_preflight_failed",
        format!(
            "Container runtime preflight failed.\n\
runtime: {}\n\
probe: `{}`\n\
{}\n\
context: {}, DOCKER_HOST={}, XDG_RUNTIME_DIR={}\n\
stderr:\n{}\n\
stdout:\n{}",
            runtime, "info", remediation, uid, docker_host, xdg_runtime_dir, stderr, stdout
        ),
        "Agent: inspect runtime preflight output; if a host service or permission change is required, ask the user for that exact action.",
    ))
}

fn push_branch_to_origin(repo: &Path, branch: &str) -> Result<(), error::DecapodError> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo)
        .arg("push")
        .arg("-u")
        .arg("origin")
        .arg(branch)
        .output()
        .map_err(error::DecapodError::IoError)?;
    if output.status.success() {
        return Ok(());
    }
    Err(error::DecapodError::ValidationError(format!(
        "host push failed for branch '{}': {}",
        branch,
        String::from_utf8_lossy(&output.stderr).trim()
    )))
}

fn default_image_for_profile(profile: ImageProfile) -> &'static str {
    match profile {
        ImageProfile::DebianSlim => "rust:1.96.1",
        ImageProfile::Alpine => "alpine:3.20",
    }
}

const DECAPOD_IMAGE_REPOSITORY: &str = "ghcr.io/decapodlabs/decapod";

fn resolve_runtime_image(
    runtime: &str,
    repo: &Path,
    profile: ImageProfile,
    image_override: Option<&str>,
) -> Result<String, error::DecapodError> {
    if let Some(image) = image_override {
        return Ok(image.to_string());
    }
    match profile {
        ImageProfile::DebianSlim => Ok(default_image_for_profile(profile).to_string()),
        ImageProfile::Alpine => ensure_local_generated_workspace_image(runtime, repo),
    }
}

fn ensure_local_generated_workspace_image(
    runtime: &str,
    repo: &Path,
) -> Result<String, error::DecapodError> {
    let repo_slug = repo
        .file_name()
        .and_then(|s| s.to_str())
        .map(sanitize_name)
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "repo".to_string());
    let image_tag = format!("decapod-local-{repo_slug}:workspace");

    let dockerfile = prepare_generated_container_profile(repo)?;

    let local_binary_fallback = env_bool("DECAPOD_CONTAINER_LOCAL_BINARY_FALLBACK", false);
    let mut build = Command::new(runtime);
    build.arg("build").arg("-f").arg(&dockerfile);
    build.arg("--build-arg").arg(format!(
        "DECAPOD_IMAGE={DECAPOD_IMAGE_REPOSITORY}:v{}-alpine",
        env!("CARGO_PKG_VERSION")
    ));
    if local_binary_fallback {
        build
            .arg("--build-arg")
            .arg("DECAPOD_IMAGE=debian:bookworm-slim")
            .arg("--build-arg")
            .arg("DECAPOD_USE_LOCAL_BINARY=1");
    }
    let output = build
        .arg("-t")
        .arg(&image_tag)
        .arg(repo)
        .output()
        .map_err(error::DecapodError::IoError)?;
    if !output.status.success() {
        return Err(auto_remediable_validation_error(
            "container_image_build_failed",
            format!(
                "Failed to build local generated workspace image '{}'\nstdout:\n{}\nstderr:\n{}",
                image_tag,
                String::from_utf8_lossy(&output.stdout).trim(),
                String::from_utf8_lossy(&output.stderr).trim()
            ),
            "Agent: fix Dockerfile/package resolution for the local image, then retry the container run.",
        ));
    }

    Ok(image_tag)
}

pub fn prepare_generated_container_profile(repo: &Path) -> Result<PathBuf, error::DecapodError> {
    let generated_dir = repo.join(".decapod").join("managed");
    fs::create_dir_all(&generated_dir).map_err(error::DecapodError::IoError)?;

    stage_current_decapod_binary(&generated_dir)?;

    migrate_legacy_managed_dockerfile(repo)?;
    let dockerfile = managed_dockerfile_path(repo);
    if !dockerfile.exists() {
        let contents = generated_dockerfile_for_repo(repo);
        fs::write(&dockerfile, contents).map_err(error::DecapodError::IoError)?;
    }
    Ok(dockerfile)
}

pub fn managed_dockerfile_path(repo: &Path) -> PathBuf {
    repo.join(MANAGED_DOCKERFILE_REL_PATH)
}

/// Evaluating Decapod release that managed workspace Dockerfiles must declare.
pub fn expected_managed_decapod_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// Default (debian) image tag for the evaluating release.
pub fn expected_managed_decapod_image(release: &str) -> String {
    format!("{DECAPOD_IMAGE_REPOSITORY}:v{release}-debian")
}

/// Accepted alpine image tag for the evaluating release.
pub fn expected_managed_decapod_image_alpine(release: &str) -> String {
    format!("{DECAPOD_IMAGE_REPOSITORY}:v{release}-alpine")
}

/// Parse `ARG NAME=value` lines from a managed Dockerfile.
pub fn dockerfile_arg_value(content: &str, arg_name: &str) -> Option<String> {
    let prefix = format!("ARG {arg_name}=");
    content.lines().find_map(|line| {
        let trimmed = line.trim();
        trimmed
            .strip_prefix(&prefix)
            .map(|value| value.trim().to_string())
    })
}

/// True when the managed Dockerfile image/version header matches the evaluating release.
///
/// Issue #1154: validate must fail closed when the managed workspace image is
/// still pinned to an older Decapod release after self-heal has run. Project
/// package/command mutations below the header remain allowed.
pub fn managed_dockerfile_matches_release(content: &str, release: &str) -> Result<(), String> {
    if !content.contains("# Generated by decapod container profile") {
        return Err(
            "managed Dockerfile is missing the Decapod container-profile marker; regenerate with `decapod validate` or container profile setup"
                .to_string(),
        );
    }
    let observed_version = dockerfile_arg_value(content, "DECAPOD_VERSION");
    let observed_image = dockerfile_arg_value(content, "DECAPOD_IMAGE");
    let expected_version = release.to_string();
    let expected_debian = expected_managed_decapod_image(release);
    let expected_alpine = expected_managed_decapod_image_alpine(release);

    match observed_version.as_deref() {
        Some(version) if version == expected_version => {}
        Some(version) => {
            return Err(format!(
                "ARG DECAPOD_VERSION={version} does not match evaluating Decapod release {expected_version}; refresh the managed Dockerfile header"
            ));
        }
        None => {
            return Err(format!(
                "ARG DECAPOD_VERSION is missing; expected {expected_version}"
            ));
        }
    }

    match observed_image.as_deref() {
        Some(image) if image == expected_debian || image == expected_alpine => Ok(()),
        Some(image) => Err(format!(
            "ARG DECAPOD_IMAGE={image} does not match evaluating Decapod release {expected_version} (expected {expected_debian} or {expected_alpine})"
        )),
        None => Err(format!(
            "ARG DECAPOD_IMAGE is missing; expected {expected_debian} (or alpine variant)"
        )),
    }
}

/// Rename the pre-0.83 managed workspace Dockerfile without overwriting edits.
pub fn migrate_legacy_managed_dockerfile(repo: &Path) -> Result<bool, error::DecapodError> {
    let current = managed_dockerfile_path(repo);
    let legacy = repo.join(LEGACY_MANAGED_DOCKERFILE_REL_PATH);
    if current.exists() || !legacy.exists() {
        return Ok(false);
    }

    fs::rename(legacy, current).map_err(error::DecapodError::IoError)?;
    Ok(true)
}

fn stage_current_decapod_binary(generated_dir: &Path) -> Result<PathBuf, error::DecapodError> {
    let current_exe = std::env::current_exe().map_err(error::DecapodError::IoError)?;
    let staged = generated_dir.join("decapod");
    fs::copy(&current_exe, &staged).map_err(error::DecapodError::IoError)?;
    make_executable(&staged)?;
    Ok(staged)
}

#[cfg(unix)]
fn make_executable(path: &Path) -> Result<(), error::DecapodError> {
    use std::os::unix::fs::PermissionsExt;

    let mut perms = fs::metadata(path)
        .map_err(error::DecapodError::IoError)?
        .permissions();
    perms.set_mode(0o755);
    fs::set_permissions(path, perms).map_err(error::DecapodError::IoError)
}

#[cfg(not(unix))]
fn make_executable(_path: &Path) -> Result<(), error::DecapodError> {
    Ok(())
}

#[derive(Debug, Clone, Copy)]
struct ProjectCapabilities {
    primary_language: Option<ProjectLanguage>,
    rust: bool,
    node: bool,
    python: bool,
    go: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProjectLanguage {
    Rust,
    Go,
    Python,
    Node,
    Ruby,
    Java,
}

#[derive(Debug, Clone, Serialize)]
struct DockerfileTemplateSchemaComponent {
    schema_version: &'static str,
    path: &'static str,
    generator: &'static str,
    regenerate_hint: &'static str,
    base_images: BTreeMap<&'static str, &'static str>,
    required_packages: Vec<&'static str>,
    stack_packages: BTreeMap<&'static str, Vec<&'static str>>,
    apt_required_packages: Vec<&'static str>,
    apt_stack_packages: BTreeMap<&'static str, Vec<&'static str>>,
    extra_packages_env: &'static str,
    legacy_extra_packages_env: &'static str,
}

fn dockerfile_template_schema_component() -> DockerfileTemplateSchemaComponent {
    let mut base_images = BTreeMap::new();
    base_images.insert("decapod", DECAPOD_IMAGE_REPOSITORY);

    let mut stack_packages = BTreeMap::new();
    stack_packages.insert("rust", vec!["rust", "cargo"]);
    stack_packages.insert("node", vec!["nodejs", "npm"]);
    stack_packages.insert("python", vec!["python3", "py3-pip"]);
    stack_packages.insert("go", vec!["go"]);
    stack_packages.insert("ruby", vec!["ruby", "ruby-dev"]);
    stack_packages.insert("java", vec!["openjdk21"]);

    let mut apt_stack_packages = BTreeMap::new();
    apt_stack_packages.insert("rust", vec!["rustc", "cargo", "build-essential"]);
    apt_stack_packages.insert("node", vec!["nodejs", "npm"]);
    apt_stack_packages.insert("python", vec!["python3", "python3-pip"]);
    apt_stack_packages.insert("go", vec!["golang-go"]);
    apt_stack_packages.insert("ruby", vec!["ruby", "ruby-dev"]);
    apt_stack_packages.insert("java", vec!["openjdk-17-jdk"]);

    DockerfileTemplateSchemaComponent {
        schema_version: "1.0.0",
        path: MANAGED_DOCKERFILE_REL_PATH,
        generator: "container::generated_dockerfile_for_repo",
        regenerate_hint: "decapod auto container run --image-profile alpine",
        base_images,
        required_packages: vec![
            "git",
            "openssh-client",
            "ca-certificates",
            "bash",
            "curl",
            "coreutils",
            "sqlite-libs",
        ],
        stack_packages,
        apt_required_packages: vec![
            "git",
            "openssh-client",
            "ca-certificates",
            "bash",
            "curl",
            "coreutils",
            "libsqlite3-0",
        ],
        apt_stack_packages,
        extra_packages_env: "DECAPOD_CONTAINER_SYSTEM_PACKAGES",
        legacy_extra_packages_env: "DECAPOD_CONTAINER_APK_PACKAGES",
    }
}

fn detect_project_capabilities(repo: &Path) -> ProjectCapabilities {
    let primary_language = detect_primary_language_from_config(repo);
    ProjectCapabilities {
        primary_language,
        rust: repo.join("Cargo.toml").exists(),
        node: repo.join("package.json").exists()
            || repo.join("pnpm-lock.yaml").exists()
            || repo.join("yarn.lock").exists(),
        python: repo.join("pyproject.toml").exists()
            || repo.join("requirements.txt").exists()
            || repo.join("poetry.lock").exists(),
        go: repo.join("go.mod").exists(),
    }
}

fn detect_primary_language_from_config(repo: &Path) -> Option<ProjectLanguage> {
    let config = repo.join(".decapod").join("config.toml");
    let content = fs::read_to_string(config).ok()?;
    let parsed: toml::Value = toml::from_str(&content).ok()?;
    let languages = parsed
        .get("repo")
        .and_then(|repo| repo.get("primary_languages"))
        .and_then(|value| value.as_array())?;
    languages
        .iter()
        .filter_map(|value| value.as_str())
        .find_map(ProjectLanguage::from_config_value)
}

impl ProjectLanguage {
    fn from_config_value(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "rust" => Some(Self::Rust),
            "go" | "golang" => Some(Self::Go),
            "python" | "python3" => Some(Self::Python),
            "node" | "nodejs" | "javascript" | "typescript" => Some(Self::Node),
            "ruby" => Some(Self::Ruby),
            "java" | "jvm" => Some(Self::Java),
            _ => None,
        }
    }

    fn package_key(self) -> &'static str {
        match self {
            Self::Rust => "rust",
            Self::Go => "go",
            Self::Python => "python",
            Self::Node => "node",
            Self::Ruby => "ruby",
            Self::Java => "java",
        }
    }
}

pub fn generated_dockerfile_for_repo(repo: &Path) -> String {
    let capabilities = detect_project_capabilities(repo);
    render_generated_dockerfile(&capabilities)
}

fn render_generated_dockerfile(capabilities: &ProjectCapabilities) -> String {
    let component = dockerfile_template_schema_component();
    let extra = std::env::var(component.extra_packages_env)
        .or_else(|_| std::env::var(component.legacy_extra_packages_env))
        .unwrap_or_default();
    let mut apk_pkgs: BTreeSet<String> = component
        .required_packages
        .iter()
        .map(|pkg| (*pkg).to_string())
        .collect();
    let mut apt_pkgs: BTreeSet<String> = component
        .apt_required_packages
        .iter()
        .map(|pkg| (*pkg).to_string())
        .collect();
    add_project_packages(
        &mut apk_pkgs,
        &component.stack_packages,
        capabilities,
        &extra,
    );
    add_project_packages(
        &mut apt_pkgs,
        &component.apt_stack_packages,
        capabilities,
        "",
    );
    for pkg in extra.split_whitespace().filter(|s| !s.trim().is_empty()) {
        apt_pkgs.insert(pkg.trim().to_string());
    }
    let apk_pkg_line = apk_pkgs.into_iter().collect::<Vec<_>>().join(" ");
    let apt_pkg_line = apt_pkgs.into_iter().collect::<Vec<_>>().join(" ");
    let decapod_version = env!("CARGO_PKG_VERSION");
    let decapod_image_repository = component
        .base_images
        .get("decapod")
        .copied()
        .unwrap_or(DECAPOD_IMAGE_REPOSITORY);
    let decapod_image = format!("{decapod_image_repository}:v{decapod_version}-debian");
    format!(
        "# Generated by decapod container profile\n\
         # Path: .decapod/managed/Dockerfile.decapod\n\
         # Managed seed: Decapod maintains the image/version header; agents may\n\
         # mutate project-specific packages and commands in workspace branches.\n\
         # The image tag and DECAPOD_VERSION below are the release pin; keep them aligned.\n\
         ARG DECAPOD_IMAGE={decapod_image}\n\
         FROM $DECAPOD_IMAGE\n\
         ARG DECAPOD_VERSION={decapod_version}\n\
         ARG DECAPOD_WORKSPACE_PATH=unknown\n\
         ARG DECAPOD_USE_LOCAL_BINARY=0\n\
         LABEL org.opencontainers.image.base.name=\"$DECAPOD_IMAGE\"\n\
         LABEL org.opencontainers.image.source=\"https://github.com/DecapodLabs/decapod\"\n\
         LABEL org.decapod.managed=\"workspace\"\n\
         LABEL org.decapod.workspace.path=\"$DECAPOD_WORKSPACE_PATH\"\n\
         LABEL org.decapod.version=\"$DECAPOD_VERSION\"\n\
         RUN if command -v apk >/dev/null 2>&1; then \\\n\
                 apk add --no-cache {apk_pkg_line}; \\\n\
             elif command -v apt-get >/dev/null 2>&1; then \\\n\
                 apt-get update && \\\n\
                 apt-get install -y --no-install-recommends {apt_pkg_line} && \\\n\
                 rm -rf /var/lib/apt/lists/*; \\\n\
             else \\\n\
                 echo \"No supported package manager found\" >&2; exit 1; \\\n\
             fi\n\
         RUN update-ca-certificates\n\
         COPY .decapod/managed/decapod /usr/local/bin/decapod.local\n\
         RUN chmod 0755 /usr/local/bin/decapod.local && \\\n\
             if [ \"$DECAPOD_USE_LOCAL_BINARY\" = \"1\" ]; then \\\n\
                 cp /usr/local/bin/decapod.local /usr/local/bin/decapod; \\\n\
             fi\n\
         RUN if [ \"$DECAPOD_USE_LOCAL_BINARY\" != \"1\" ]; then \\\n\
                 /usr/local/bin/decapod --help >/dev/null; \\\n\
             fi\n"
    )
}

fn add_project_packages(
    pkgs: &mut BTreeSet<String>,
    stack_packages: &BTreeMap<&'static str, Vec<&'static str>>,
    capabilities: &ProjectCapabilities,
    extra: &str,
) {
    if capabilities.node {
        for pkg in stack_packages
            .get("node")
            .into_iter()
            .flat_map(|v| v.iter())
        {
            pkgs.insert((*pkg).to_string());
        }
    }
    if capabilities.python {
        for pkg in stack_packages
            .get("python")
            .into_iter()
            .flat_map(|v| v.iter())
        {
            pkgs.insert((*pkg).to_string());
        }
    }
    if capabilities.go {
        for pkg in stack_packages.get("go").into_iter().flat_map(|v| v.iter()) {
            pkgs.insert((*pkg).to_string());
        }
    }
    if capabilities.rust {
        for pkg in stack_packages
            .get("rust")
            .into_iter()
            .flat_map(|v| v.iter())
        {
            pkgs.insert((*pkg).to_string());
        }
    }
    if let Some(language) = capabilities.primary_language {
        for pkg in stack_packages
            .get(language.package_key())
            .into_iter()
            .flat_map(|v| v.iter())
        {
            pkgs.insert((*pkg).to_string());
        }
    }
    for p in extra.split_whitespace().filter(|s| !s.trim().is_empty()) {
        pkgs.insert(p.trim().to_string());
    }
}

fn current_uid_gid() -> Option<(String, String)> {
    let uid = Command::new("id").arg("-u").output().ok()?;
    let gid = Command::new("id").arg("-g").output().ok()?;
    if !uid.status.success() || !gid.status.success() {
        return None;
    }
    let uid_s = String::from_utf8_lossy(&uid.stdout).trim().to_string();
    let gid_s = String::from_utf8_lossy(&gid.stdout).trim().to_string();
    if uid_s.is_empty() || gid_s.is_empty() {
        return None;
    }
    Some((uid_s, gid_s))
}

fn run_git(repo: &Path, args: &[&str]) -> Result<(), error::DecapodError> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .map_err(error::DecapodError::IoError)?;
    if output.status.success() {
        return Ok(());
    }
    Err(error::DecapodError::ValidationError(format!(
        "git {} failed: {}",
        args.join(" "),
        String::from_utf8_lossy(&output.stderr).trim()
    )))
}

fn prepare_workspace_clone(
    repo: &Path,
    branch: &str,
    base_branch: &str,
) -> Result<WorkspaceSpec, error::DecapodError> {
    let workspaces_root = repo.join(".decapod").join("workspaces");
    fs::create_dir_all(&workspaces_root).map_err(error::DecapodError::IoError)?;

    let suffix = crate::core::ulid::new_ulid().to_lowercase();
    let dir_name = format!("{}-{}", sanitize_branch_component(branch), &suffix[..8]);
    let workspace_path = workspaces_root.join(dir_name);
    let workspace_path_str = workspace_path
        .to_str()
        .ok_or_else(|| error::DecapodError::PathError("invalid workspace path".to_string()))?;

    let base_ref = format!("refs/heads/{base_branch}");
    let clone_output = if git_ref_exists(repo, &base_ref)? {
        Command::new("git")
            .arg("clone")
            .arg("--no-local")
            .arg("--branch")
            .arg(base_branch)
            .arg("--single-branch")
            .arg(repo)
            .arg(workspace_path_str)
            .output()
            .map_err(error::DecapodError::IoError)?
    } else {
        Command::new("git")
            .arg("clone")
            .arg("--no-local")
            .arg(repo)
            .arg(workspace_path_str)
            .output()
            .map_err(error::DecapodError::IoError)?
    };
    if !clone_output.status.success() {
        return Err(error::DecapodError::ValidationError(format!(
            "git clone failed: {}",
            String::from_utf8_lossy(&clone_output.stderr).trim()
        )));
    }

    let local_base_ref = format!("refs/heads/{base_branch}");
    let remote_base_ref = format!("refs/remotes/origin/{base_branch}");
    if git_ref_exists(&workspace_path, &local_base_ref)? {
        run_git(&workspace_path, &["checkout", "-B", branch, base_branch])?;
    } else if git_ref_exists(&workspace_path, &remote_base_ref)? {
        let from_remote = format!("origin/{base_branch}");
        run_git(&workspace_path, &["checkout", "-B", branch, &from_remote])?;
    } else {
        run_git(&workspace_path, &["checkout", "-B", branch])?;
    }

    Ok(WorkspaceSpec {
        branch: branch.to_string(),
        path: workspace_path,
        base_branch: base_branch.to_string(),
        backend: "local-clone".to_string(),
    })
}

fn git_ref_exists(repo: &Path, git_ref: &str) -> Result<bool, error::DecapodError> {
    let status = Command::new("git")
        .arg("-C")
        .arg(repo)
        .arg("show-ref")
        .arg("--verify")
        .arg("--quiet")
        .arg(git_ref)
        .status()
        .map_err(error::DecapodError::IoError)?;
    Ok(status.success())
}

fn sync_workspace_branch_to_host_repo(
    repo: &Path,
    workspace: &Path,
    branch: &str,
) -> Result<(), error::DecapodError> {
    let branch_ref = format!("refs/heads/{branch}");
    if !git_ref_exists(workspace, &branch_ref)? {
        return Err(auto_remediable_validation_error(
            "container_workspace_branch_missing",
            format!("workspace branch '{branch}' does not exist; cannot sync back to host repo"),
            "Agent: retry with a freshly prepared Decapod workspace; the previous workspace did not retain the expected branch.",
        ));
    }

    let workspace_str = workspace.to_str().ok_or_else(|| {
        error::DecapodError::PathError("invalid workspace path for host sync".to_string())
    })?;
    let refspec = format!("+{branch_ref}:{branch_ref}");
    let output = Command::new("git")
        .arg("-C")
        .arg(repo)
        .arg("fetch")
        .arg("--no-tags")
        .arg(workspace_str)
        .arg(&refspec)
        .output()
        .map_err(error::DecapodError::IoError)?;
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if stderr.contains("refusing to fetch into branch")
        && matches!(current_branch(repo), Ok(current) if current == branch)
    {
        let pull_output = Command::new("git")
            .arg("-C")
            .arg(repo)
            .arg("pull")
            .arg("--ff-only")
            .arg(workspace_str)
            .arg(branch)
            .output()
            .map_err(error::DecapodError::IoError)?;
        if pull_output.status.success() {
            return Ok(());
        }
        return Err(auto_remediable_validation_error(
            "container_branch_sync_checked_out",
            format!(
                "failed fast-forwarding checked-out branch '{}' from workspace '{}': {}",
                branch,
                workspace.display(),
                String::from_utf8_lossy(&pull_output.stderr).trim()
            ),
            "Agent: check out a different host branch or use a dedicated Decapod worktree, then retry branch foldback.",
        ));
    }
    Err(auto_remediable_validation_error(
        "container_branch_sync_failed",
        format!("failed syncing workspace branch '{branch}' back to host repo: {stderr}"),
        "Agent: resolve the git sync diagnostic, then retry foldback or push from the retained workspace branch.",
    ))
}

fn current_branch(repo: &Path) -> Result<String, error::DecapodError> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["branch", "--show-current"])
        .output()
        .map_err(error::DecapodError::IoError)?;
    if !output.status.success() {
        return Err(error::DecapodError::ValidationError(format!(
            "failed determining current branch: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn create_gh_pr(
    repo: &Path,
    branch: &str,
    base_branch: &str,
    title: Option<&str>,
    body: Option<&str>,
) -> Result<(), error::DecapodError> {
    let repo_str = repo.to_str().ok_or_else(|| {
        error::DecapodError::PathError("invalid repo path for PR creation".to_string())
    })?;

    let title_arg = title
        .map(String::from)
        .unwrap_or_else(|| format!("Agent PR: {branch}"));
    let body_arg = body
        .map(String::from)
        .unwrap_or_else(|| "Created by Decapod agent container workflow".to_string());

    let output = Command::new("gh")
        .arg("pr")
        .arg("create")
        .arg("--base")
        .arg(base_branch)
        .arg("--head")
        .arg(branch)
        .arg("--title")
        .arg(&title_arg)
        .arg("--body")
        .arg(&body_arg)
        .arg("--repo")
        .arg(repo_str)
        .output()
        .map_err(error::DecapodError::IoError)?;

    if output.status.success() {
        return Ok(());
    }
    Err(error::DecapodError::ValidationError(format!(
        "failed creating PR for branch '{}': {}",
        branch,
        String::from_utf8_lossy(&output.stderr).trim()
    )))
}

fn cleanup_workspace_clone(workspace_path: &Path) -> Result<(), error::DecapodError> {
    if workspace_path.exists() {
        fs::remove_dir_all(workspace_path).map_err(error::DecapodError::IoError)?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn build_docker_spec(
    runtime: &str,
    repo_root: &Path,
    workspace: &Path,
    image: &str,
    agent: &str,
    user_cmd: &str,
    branch: &str,
    base_branch: &str,
    memory: &str,
    cpus: &str,
    task_id: Option<&str>,
    inherit_env: bool,
    local_only: bool,
) -> Result<DockerSpec, error::DecapodError> {
    let decapod_dir = repo_root.join(".decapod");
    fs::create_dir_all(&decapod_dir).map_err(error::DecapodError::IoError)?;
    let decapod_dir_str = decapod_dir
        .to_str()
        .ok_or_else(|| error::DecapodError::PathError("invalid .decapod path".to_string()))?;
    let workspace_str = workspace
        .to_str()
        .ok_or_else(|| error::DecapodError::PathError("invalid repository path".to_string()))?;
    let workspace_decapod_mount = format!("{workspace_str}/.decapod");
    let container_name = format!(
        "decapod-agent-{}-{}",
        sanitize_name(agent),
        &crate::core::ulid::new_ulid().to_lowercase()[..8]
    );
    let mut args = vec![
        "run".to_string(),
        "--rm".to_string(),
        "--name".to_string(),
        container_name.clone(),
        "--cap-drop".to_string(),
        "ALL".to_string(),
        "--security-opt".to_string(),
        "no-new-privileges:true".to_string(),
        "--pids-limit".to_string(),
        "512".to_string(),
        "--memory".to_string(),
        memory.to_string(),
        "--cpus".to_string(),
        cpus.to_string(),
        "--tmpfs".to_string(),
        "/tmp:rw,noexec,nosuid,size=256m".to_string(),
        "-e".to_string(),
        "DECAPOD_CONTAINER=1".to_string(),
        "-e".to_string(),
        format!("DECAPOD_AGENT_ID={}", agent),
        "-e".to_string(),
        format!("DECAPOD_TASK_ID={}", task_id.unwrap_or("")),
        "-e".to_string(),
        format!("DECAPOD_BRANCH={}", branch),
        "-e".to_string(),
        format!("DECAPOD_BASE_BRANCH={}", base_branch),
        "-e".to_string(),
        "DECAPOD_PUSH=0".to_string(),
        "-e".to_string(),
        "DECAPOD_PR=0".to_string(),
        "-e".to_string(),
        format!("DECAPOD_WORKSPACE={}", workspace_str),
        "-e".to_string(),
        "DECAPOD_LOCAL_ONLY=1".to_string(),
        "-v".to_string(),
        format!("{}:{}", workspace_str, workspace_str),
        "-v".to_string(),
        format!("{}:{}", decapod_dir_str, workspace_decapod_mount),
        "-w".to_string(),
        workspace_str.to_string(),
    ];

    if inherit_env {
        for (k, v) in inherited_env_vars() {
            args.push("-e".to_string());
            args.push(format!("{k}={v}"));
        }
    }
    args.push("-e".to_string());
    args.push("HOME=/tmp/decapod-home".to_string());
    args.push("-e".to_string());
    args.push("GIT_CONFIG_GLOBAL=/tmp/decapod-home/.gitconfig".to_string());

    if runtime == "docker"
        && env_bool("DECAPOD_CONTAINER_MAP_HOST_USER", true)
        && let Some((uid, gid)) = current_uid_gid()
    {
        args.push("--user".to_string());
        args.push(format!("{uid}:{gid}"));
    }

    if runtime != "docker" && runtime != "podman" {
        return Err(error::DecapodError::ValidationError(format!(
            "Unsupported container runtime '{runtime}'"
        )));
    }

    args.push(image.to_string());
    args.push("/bin/sh".to_string());
    args.push("-c".to_string());
    args.push(build_container_script(
        user_cmd,
        branch,
        base_branch,
        local_only,
    ));
    if env_bool("DECAPOD_CONTAINER_DEBUG", false) {
        eprintln!("debug: container args={}", args.join(" "));
    }

    Ok(DockerSpec {
        args,
        container_name,
    })
}

fn inherited_env_vars() -> BTreeMap<String, String> {
    let mut vars = BTreeMap::new();
    for (k, v) in std::env::vars() {
        if k == "PATH" || k.starts_with("BASH_FUNC_") {
            continue;
        }
        vars.insert(k, v);
    }
    vars
}

#[allow(clippy::too_many_arguments)]
fn build_container_script(
    user_cmd: &str,
    branch: &str,
    base_branch: &str,
    local_only: bool,
) -> String {
    let mut script = String::from(
        "set -eu\n\
         cd \"${DECAPOD_WORKSPACE:-$PWD}\"\n\
         mkdir -p \"${HOME:-/tmp/decapod-home}\"\n\
         git_safe() {\n\
           git -c safe.directory=\"${DECAPOD_WORKSPACE:-$PWD}\" \"$@\"\n\
         }\n\
         unset SSH_AUTH_SOCK || true\n\
         if [ \"${DECAPOD_CONTAINER_DEBUG:-0}\" = \"1\" ]; then\n\
           echo \"debug: workspace=${DECAPOD_WORKSPACE:-$PWD}\" >&2\n\
           echo \"debug: uid=$(id -u) gid=$(id -g)\" >&2\n\
           git_safe remote -v >&2 || true\n\
         fi\n\
         unset GIT_DIR GIT_WORK_TREE\n\
         git config --global user.name \"${DECAPOD_GIT_USER_NAME:-Decapod Agent}\"\n\
         git config --global user.email \"${DECAPOD_GIT_USER_EMAIL:-agent@decapod.local}\"\n\
         if ! command -v decapod >/dev/null 2>&1 && [ -f Cargo.toml ] && command -v cargo >/dev/null 2>&1; then\n\
           decapod() { cargo run --quiet --bin decapod -- \"$@\"; }\n\
         fi\n\
         if command -v decapod >/dev/null 2>&1; then\n\
           decapod version >/dev/null 2>&1 || true\n\
           if decapod --help 2>/dev/null | grep -qE \"(^|[[:space:]])update([[:space:]]|$)\"; then\n\
             decapod update\n\
           fi\n\
         fi\n",
    );
    let local_head_ref = shell_escape(&format!("refs/heads/{base_branch}"));
    let remote_head_ref = shell_escape(&format!("refs/remotes/origin/{base_branch}"));
    let base_branch_escaped = shell_escape(base_branch);
    let branch_escaped = shell_escape(branch);
    script.push_str(&format!(
        "if git_safe show-ref --verify --quiet {local_head_ref}; then\n\
           git_safe checkout -B {branch_escaped} {base_branch_escaped}\n\
         elif git_safe show-ref --verify --quiet {remote_head_ref}; then\n\
           git_safe checkout -B {branch_escaped} origin/{base_branch_escaped}\n\
         else\n\
           git_safe checkout -B {branch_escaped}\n\
         fi\n",
    ));
    script.push_str(user_cmd);
    script.push('\n');

    script.push_str(
        "if [ -n \"$(git_safe status --porcelain)\" ]; then\n  git_safe add -A\n  git_safe commit -m \"chore: automated container updates\"\nfi\n",
    );
    if !local_only {
        script.push_str(
            "if [ \"${DECAPOD_CONTAINER_DEBUG:-0}\" = \"1\" ]; then\n  echo \"debug: host control-plane handles push/PR after foldback\" >&2\nfi\n",
        );
    }

    script
}

fn shell_escape(s: &str) -> String {
    let escaped = s.replace('\'', "'\"'\"'");
    format!("'{escaped}'")
}

fn sanitize_name(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_string()
}

fn sanitize_branch_component(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_string()
}

fn default_branch_name(agent: &str, task_id: Option<&str>) -> String {
    let suffix = task_id
        .map(sanitize_branch_component)
        .unwrap_or_else(|| crate::core::ulid::new_ulid().to_lowercase());
    format!("agent/{}/{}", sanitize_branch_component(agent), suffix)
}

fn env_bool(name: &str, default_value: bool) -> bool {
    match std::env::var(name) {
        Ok(v) => matches!(v.to_ascii_lowercase().as_str(), "1" | "true" | "yes" | "on"),
        Err(_) => default_value,
    }
}

pub fn schema() -> serde_json::Value {
    let dockerfile_component = dockerfile_template_schema_component();
    json!({
        "name": "container",
        "version": "0.2.0",
        "description": "Ephemeral containerized agent execution with isolated local clone workspaces and host-branch foldback",
        "commands": [
            { "name": "run", "parameters": ["agent", "cmd", "branch", "task_id", "push", "pr", "pr_base", "pr_title", "pr_body", "image_profile", "image", "timeout_seconds", "memory", "cpus", "repo", "keep_worktree", "inherit_env", "local_only"] }
        ],
        "profiles": {
            "debian-slim": "rust:1.96.1",
            "alpine": "local build from .decapod/managed/Dockerfile.decapod (Decapod workspace seed + detected project dependencies + current decapod binary)"
        },
        "components": {
            "dockerfile_template": dockerfile_component
        },
        "safety_defaults": {
            "rm": true,
            "cap_drop": "ALL",
            "no_new_privileges": true,
            "pids_limit": 512,
            "tmpfs_tmp": true
        }
    })
}
#[cfg(test)]
#[path = "../../../tests/unit/plugins/container_tests.rs"]
mod tests;
