use crate::core::error;
use crate::core::store::Store;
use crate::core::time;
use clap::{Parser, Subcommand, ValueEnum};
use serde_json::json;
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};
use ulid::Ulid;

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
        #[clap(long, default_value = "master")]
        pr_base: String,
        #[clap(long)]
        pr_title: Option<String>,
        #[clap(long)]
        pr_body: Option<String>,
        #[clap(long, value_enum, default_value = "alpine")]
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

#[derive(Debug, Clone)]
pub struct RunSummary {
    pub value: serde_json::Value,
}

pub(crate) const CONTAINER_DISABLE_MARKER: &str = "DECAPOD_CONTAINER_RUNTIME_DISABLED=true";

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
            &pr_base,
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
    let keep_worktree = env_bool("DECAPOD_CLAIM_KEEP_WORKTREE", false);
    let pr_title = std::env::var("DECAPOD_CLAIM_PR_TITLE")
        .ok()
        .or_else(|| Some(format!("{} [{}]", task_title, task_id)));
    let pr_body = std::env::var("DECAPOD_CLAIM_PR_BODY").ok().or_else(|| {
        Some(format!(
            "Automated container run for claimed task {}",
            task_id
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
        "master",
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
    pr_base: &str,
    _pr_title: Option<&str>,
    _pr_body: Option<&str>,
    image_profile: ImageProfile,
    image_override: Option<&str>,
    timeout_seconds: u64,
    memory: &str,
    cpus: &str,
    repo_override: Option<&str>,
    keep_worktree: bool,
    inherit_env: bool,
    _local_only: bool,
) -> Result<RunSummary, error::DecapodError> {
    let repo = resolve_repo_path(repo_override)?;
    if container_runtime_disabled(&repo)? {
        return Err(error::DecapodError::ValidationError(
            "Container subsystem is disabled by .decapod/OVERRIDE.md. \
Remove the disable marker after installing Docker/Podman and configuring a dedicated local SSH key. \
Warning: running without isolated containers means concurrent agents can step on each other."
                .to_string(),
        ));
    }

    let docker = match find_container_runtime() {
        Ok(runtime) => runtime,
        Err(_) => {
            disable_container_runtime_override(
                &repo,
                "No docker/podman runtime found",
                "Install Docker or Podman first, then re-run the task.",
            )?;
            return Err(error::DecapodError::ValidationError(
                "No container runtime found (docker/podman).\n\
Please install Docker or Podman.\n\
I also wrote .decapod/OVERRIDE.md with container runtime disabled so agent runs stay safe by default.\n\
Warning: without isolated containers, concurrent agents can step on each other."
                    .to_string(),
            ));
        }
    };
    if push || pr {
        return Err(error::DecapodError::ValidationError(
            "Container runs are local-workspace-only. --push/--pr are disabled; fold branch locally, then push from host repo if needed."
                .to_string(),
        ));
    }

    let image = resolve_runtime_image(&docker, &repo, image_profile, image_override)?;

    let branch_name = branch
        .map(|s| s.to_string())
        .unwrap_or_else(|| default_branch_name(agent, task_id));
    let workspace = prepare_workspace_clone(&repo, &branch_name, pr_base)?;

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
    )?;

    let start = Instant::now();
    let output = execute_container_with_timeout(&docker, &spec.args, timeout_seconds).map_err(
        |exec_err| {
            let sync_msg =
                match sync_workspace_branch_to_host_repo(&repo, &workspace.path, &workspace.branch)
                {
                    Ok(_) => "branch foldback: synced to host repo after container termination"
                        .to_string(),
                    Err(sync_err) => format!(
                        "branch foldback: sync failed after container termination: {}",
                        sync_err
                    ),
                };
            if !keep_worktree {
                let _ = cleanup_workspace_clone(&workspace.path);
            }
            error::DecapodError::ValidationError(format!(
                "container runtime terminated before normal completion: {}\n{}",
                exec_err, sync_msg
            ))
        },
    )?;
    let elapsed = start.elapsed().as_secs();

    let status = if output.status.success() {
        "ok"
    } else {
        "error"
    };
    sync_workspace_branch_to_host_repo(&repo, &workspace.path, &workspace.branch)?;
    let branch_returned_to_host = true;

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
        "local_only": true,
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
        return Err(error::DecapodError::ValidationError(format!(
            "Container command failed (exit {:?})\nstdout:\n{}\nstderr:\n{}",
            output.status.code(),
            stdout.trim(),
            stderr.trim()
        )));
    }
    if let Some(err) = cleanup_err {
        return Err(err);
    }

    Ok(RunSummary { value: summary })
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
            return Err(error::DecapodError::ValidationError(format!(
                "Container command timed out after {}s",
                timeout_seconds
            )));
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

fn disable_container_runtime_override(
    repo_root: &Path,
    reason: &str,
    remediation: &str,
) -> Result<(), error::DecapodError> {
    let path = override_file_path(repo_root);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(error::DecapodError::IoError)?;
    }
    let mut content = if path.exists() {
        fs::read_to_string(&path).map_err(error::DecapodError::IoError)?
    } else {
        String::new()
    };
    if content.contains(CONTAINER_DISABLE_MARKER) {
        return Ok(());
    }
    if !content.ends_with('\n') && !content.is_empty() {
        content.push('\n');
    }
    content.push_str(
        "\n### plugins/CONTAINER.md\n\
## Runtime Guard Override (auto-generated)\n\
",
    );
    content.push_str(CONTAINER_DISABLE_MARKER);
    content.push('\n');
    content.push_str(&format!("reason: {}\n", reason));
    content.push_str(&format!("remediation: {}\n", remediation));
    content.push_str("warning: disabling isolated containers increases risk of concurrent agents stepping on each other.\n");
    fs::write(path, content).map_err(error::DecapodError::IoError)?;
    Ok(())
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

fn find_container_runtime() -> Result<String, error::DecapodError> {
    if command_exists("docker") {
        return Ok("docker".to_string());
    }
    if command_exists("podman") {
        return Ok("podman".to_string());
    }
    Err(error::DecapodError::NotFound(
        "No container runtime found (docker/podman)".to_string(),
    ))
}

fn command_exists(cmd: &str) -> bool {
    Command::new(cmd)
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn default_image_for_profile(profile: ImageProfile) -> &'static str {
    match profile {
        ImageProfile::DebianSlim => "rust:1.85",
        ImageProfile::Alpine => "alpine:3.20",
    }
}

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
        ImageProfile::Alpine => ensure_local_alpine_image(runtime, repo),
    }
}

fn ensure_local_alpine_image(runtime: &str, repo: &Path) -> Result<String, error::DecapodError> {
    let generated_dir = repo.join(".decapod").join("generated");
    fs::create_dir_all(&generated_dir).map_err(error::DecapodError::IoError)?;

    let repo_slug = repo
        .file_name()
        .and_then(|s| s.to_str())
        .map(sanitize_name)
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "repo".to_string());
    let image_tag = format!("decapod-local-{}:alpine", repo_slug);

    let dockerfile = generated_dir.join("Dockerfile");
    let capabilities = detect_project_capabilities(repo);
    let contents = render_generated_dockerfile(&capabilities);
    fs::write(&dockerfile, contents).map_err(error::DecapodError::IoError)?;

    let output = Command::new(runtime)
        .arg("build")
        .arg("-f")
        .arg(&dockerfile)
        .arg("-t")
        .arg(&image_tag)
        .arg(&generated_dir)
        .output()
        .map_err(error::DecapodError::IoError)?;
    if !output.status.success() {
        return Err(error::DecapodError::ValidationError(format!(
            "Failed to build local alpine image '{}'\nstdout:\n{}\nstderr:\n{}",
            image_tag,
            String::from_utf8_lossy(&output.stdout).trim(),
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }

    Ok(image_tag)
}

#[derive(Debug, Clone, Copy)]
struct ProjectCapabilities {
    rust: bool,
    node: bool,
    python: bool,
    go: bool,
}

fn detect_project_capabilities(repo: &Path) -> ProjectCapabilities {
    ProjectCapabilities {
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

fn render_generated_dockerfile(capabilities: &ProjectCapabilities) -> String {
    let extra = std::env::var("DECAPOD_CONTAINER_APK_PACKAGES").unwrap_or_default();
    let mut pkgs: BTreeSet<String> = BTreeSet::new();
    for base in ["git", "openssh-client", "ca-certificates", "bash", "curl"] {
        pkgs.insert(base.to_string());
    }
    if capabilities.node {
        pkgs.insert("nodejs".to_string());
        pkgs.insert("npm".to_string());
    }
    if capabilities.python {
        pkgs.insert("python3".to_string());
        pkgs.insert("py3-pip".to_string());
    }
    if capabilities.go {
        pkgs.insert("go".to_string());
    }
    for p in extra.split_whitespace().filter(|s| !s.trim().is_empty()) {
        pkgs.insert(p.trim().to_string());
    }
    let pkg_line = pkgs.into_iter().collect::<Vec<_>>().join(" ");
    let base = if capabilities.rust {
        "rust:1.85-alpine"
    } else {
        "alpine:3.20"
    };
    format!(
        "# Generated by decapod container profile\n\
         # Path: .decapod/generated/Dockerfile\n\
         # Regenerate via: decapod auto container run --image-profile alpine\n\
         FROM {}\n\
         RUN apk add --no-cache {}\n\
         RUN update-ca-certificates\n",
        base, pkg_line
    )
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

    let suffix = Ulid::new().to_string().to_lowercase();
    let dir_name = format!("{}-{}", sanitize_branch_component(branch), &suffix[..8]);
    let workspace_path = workspaces_root.join(dir_name);
    let workspace_path_str = workspace_path
        .to_str()
        .ok_or_else(|| error::DecapodError::PathError("invalid workspace path".to_string()))?;

    let base_ref = format!("refs/heads/{}", base_branch);
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

    let local_base_ref = format!("refs/heads/{}", base_branch);
    let remote_base_ref = format!("refs/remotes/origin/{}", base_branch);
    if git_ref_exists(&workspace_path, &local_base_ref)? {
        run_git(&workspace_path, &["checkout", "-B", branch, base_branch])?;
    } else if git_ref_exists(&workspace_path, &remote_base_ref)? {
        let from_remote = format!("origin/{}", base_branch);
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
    let branch_ref = format!("refs/heads/{}", branch);
    if !git_ref_exists(workspace, &branch_ref)? {
        return Err(error::DecapodError::ValidationError(format!(
            "workspace branch '{}' does not exist; cannot sync back to host repo",
            branch
        )));
    }

    let workspace_str = workspace.to_str().ok_or_else(|| {
        error::DecapodError::PathError("invalid workspace path for host sync".to_string())
    })?;
    let refspec = format!("+{}:{}", branch_ref, branch_ref);
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
    Err(error::DecapodError::ValidationError(format!(
        "failed syncing workspace branch '{}' back to host repo: {}",
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
) -> Result<DockerSpec, error::DecapodError> {
    let decapod_dir = repo_root.join(".decapod");
    fs::create_dir_all(&decapod_dir).map_err(error::DecapodError::IoError)?;
    let decapod_dir_str = decapod_dir
        .to_str()
        .ok_or_else(|| error::DecapodError::PathError("invalid .decapod path".to_string()))?;
    let workspace_str = workspace
        .to_str()
        .ok_or_else(|| error::DecapodError::PathError("invalid repository path".to_string()))?;
    let workspace_decapod_mount = format!("{}/.decapod", workspace_str);
    let container_name = format!(
        "decapod-agent-{}-{}",
        sanitize_name(agent),
        &Ulid::new().to_string().to_lowercase()[..8]
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
            args.push(format!("{}={}", k, v));
        }
    }
    args.push("-e".to_string());
    args.push("HOME=/tmp/decapod-home".to_string());
    args.push("-e".to_string());
    args.push("GIT_CONFIG_GLOBAL=/tmp/decapod-home/.gitconfig".to_string());

    if env_bool("DECAPOD_CONTAINER_MAP_HOST_USER", true) {
        if let Some((uid, gid)) = current_uid_gid() {
            args.push("--user".to_string());
            args.push(format!("{}:{}", uid, gid));
        }
    }

    if runtime != "docker" && runtime != "podman" {
        return Err(error::DecapodError::ValidationError(format!(
            "Unsupported container runtime '{}'",
            runtime
        )));
    }

    args.push(image.to_string());
    args.push("/bin/sh".to_string());
    args.push("-lc".to_string());
    args.push(build_container_script(user_cmd, branch, base_branch));
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
        if k.starts_with("BASH_FUNC_") {
            continue;
        }
        vars.insert(k, v);
    }
    vars
}

#[allow(clippy::too_many_arguments)]
fn build_container_script(user_cmd: &str, branch: &str, base_branch: &str) -> String {
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
         if command -v decapod >/dev/null 2>&1; then\n\
           decapod version >/dev/null 2>&1 || true\n\
           if decapod --help 2>/dev/null | grep -qE \"(^|[[:space:]])update([[:space:]]|$)\"; then\n\
             decapod update\n\
           fi\n\
         fi\n",
    );
    let local_head_ref = shell_escape(&format!("refs/heads/{}", base_branch));
    let remote_head_ref = shell_escape(&format!("refs/remotes/origin/{}", base_branch));
    let base_branch_escaped = shell_escape(base_branch);
    let branch_escaped = shell_escape(branch);
    script.push_str(&format!(
        "if git_safe show-ref --verify --quiet {}; then\n\
           git_safe checkout -B {} {}\n\
         elif git_safe show-ref --verify --quiet {}; then\n\
           git_safe checkout -B {} origin/{}\n\
         else\n\
           git_safe checkout -B {}\n\
         fi\n",
        local_head_ref,
        branch_escaped,
        base_branch_escaped,
        remote_head_ref,
        branch_escaped,
        base_branch_escaped,
        branch_escaped,
    ));
    script.push_str(user_cmd);
    script.push('\n');

    script.push_str(
        "if [ -n \"$(git_safe status --porcelain)\" ]; then\n  git_safe add -A\n  git_safe commit -m \"chore: automated container updates\"\nfi\n",
    );

    script
}

fn shell_escape(s: &str) -> String {
    let escaped = s.replace('\'', "'\"'\"'");
    format!("'{}'", escaped)
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
        .unwrap_or_else(|| Ulid::new().to_string().to_lowercase());
    format!("agent/{}/{}", sanitize_branch_component(agent), suffix)
}

fn env_bool(name: &str, default_value: bool) -> bool {
    match std::env::var(name) {
        Ok(v) => matches!(v.to_ascii_lowercase().as_str(), "1" | "true" | "yes" | "on"),
        Err(_) => default_value,
    }
}

pub fn schema() -> serde_json::Value {
    json!({
        "name": "container",
        "version": "0.2.0",
        "description": "Ephemeral containerized agent execution with isolated local clone workspaces and host-branch foldback",
        "commands": [
            { "name": "run", "parameters": ["agent", "cmd", "branch", "task_id", "push", "pr", "pr_base", "pr_title", "pr_body", "image_profile", "image", "timeout_seconds", "memory", "cpus", "repo", "keep_worktree", "inherit_env", "local_only"] }
        ],
        "profiles": {
            "debian-slim": "rust:1.85",
            "alpine": "local build from .decapod/generated/Dockerfile (alpine + detected project dependencies)"
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
mod tests {
    use super::*;

    #[test]
    fn docker_spec_contains_safety_flags_and_sdlc_steps() {
        let repo = PathBuf::from("/tmp/repo");
        let workspace = PathBuf::from("/tmp/repo/.decapod/workspaces/w1");
        let spec = build_docker_spec(
            "docker",
            &repo,
            &workspace,
            "rust:1.85",
            "agent-a",
            "cargo test -q",
            "ahr/branch",
            "master",
            "2g",
            "2.0",
            Some("R_123"),
            false,
        )
        .expect("spec");

        let joined = spec.args.join(" ");
        assert!(joined.contains("--rm"));
        assert!(joined.contains("--cap-drop ALL"));
        assert!(joined.contains("--security-opt no-new-privileges:true"));
        assert!(
            joined.contains("-v /tmp/repo/.decapod/workspaces/w1:/tmp/repo/.decapod/workspaces/w1")
        );
        assert!(joined.contains("-v /tmp/repo/.decapod:/tmp/repo/.decapod/workspaces/w1/.decapod"));
        assert!(joined.contains("DECAPOD_LOCAL_ONLY=1"));
        assert!(joined.contains("git_safe checkout -B 'ahr/branch'"));
        assert!(!joined.contains("git_safe fetch --no-write-fetch-head origin 'master'"));
        assert!(!joined.contains("git_safe rebase origin/'master'"));
        assert!(joined.contains("decapod update"));
        assert!(!joined.contains("git_safe push -u origin 'ahr/branch'"));
        assert!(!joined.contains("gh auth status"));
        assert!(!joined.contains("gh pr create --base 'master' --head 'ahr/branch'"));
    }

    #[test]
    fn docker_spec_local_only_avoids_remote_git_operations() {
        let repo = PathBuf::from("/tmp/repo");
        let workspace = PathBuf::from("/tmp/repo/.decapod/workspaces/w1");
        let spec = build_docker_spec(
            "docker",
            &repo,
            &workspace,
            "rust:1.85",
            "agent-a",
            "cargo test -q",
            "ahr/branch",
            "master",
            "2g",
            "2.0",
            Some("R_123"),
            false,
        )
        .expect("spec");

        let joined = spec.args.join(" ");
        assert!(joined.contains("DECAPOD_LOCAL_ONLY=1"));
        assert!(!joined.contains("git_safe fetch --no-write-fetch-head origin 'master'"));
        assert!(!joined.contains("git_safe rebase origin/'master'"));
        assert!(!joined.contains("git_safe push -u origin 'ahr/branch'"));
        assert!(!joined.contains("gh pr create --base 'master' --head 'ahr/branch'"));
        assert!(!joined.contains("ssh-keyscan -t ed25519 github.com"));
        assert!(joined.contains("git_safe checkout -B 'ahr/branch' 'master'"));
    }

    #[test]
    fn sanitize_name_normalizes_agent_identifiers() {
        assert_eq!(sanitize_name("Agent_One"), "agent-one");
        assert_eq!(sanitize_name("  team/a  "), "team-a");
    }

    #[test]
    fn default_branch_name_includes_agent_and_task() {
        let branch = default_branch_name("Agent_One", Some("R_ABC-123"));
        assert_eq!(branch, "agent/agent-one/r-abc-123");
    }

    #[test]
    fn alpine_dockerfile_includes_git_ssh_and_rust_when_needed() {
        let content = render_generated_dockerfile(&ProjectCapabilities {
            rust: true,
            node: false,
            python: false,
            go: false,
        });
        assert!(content.contains("FROM rust:1.85-alpine"));
        assert!(content.contains("git"));
        assert!(content.contains("openssh-client"));
    }

    #[test]
    fn alpine_dockerfile_can_skip_rust_for_non_rust_projects() {
        let content = render_generated_dockerfile(&ProjectCapabilities {
            rust: false,
            node: false,
            python: false,
            go: false,
        });
        assert!(content.contains("FROM alpine:3.20"));
        assert!(content.contains("git"));
        assert!(!content.contains("rust:1.85-alpine"));
    }

    #[test]
    fn generated_dockerfile_expands_with_detected_stacks() {
        let content = render_generated_dockerfile(&ProjectCapabilities {
            rust: false,
            node: true,
            python: true,
            go: true,
        });
        assert!(content.contains("nodejs"));
        assert!(content.contains("python3"));
        assert!(content.contains("go"));
    }

    #[test]
    fn disable_override_marks_container_runtime_disabled() {
        let root = std::env::temp_dir().join(format!(
            "decapod-container-override-{}",
            Ulid::new().to_string().to_lowercase()
        ));
        fs::create_dir_all(&root).expect("mkdir");
        disable_container_runtime_override(&root, "test-reason", "test-remediation")
            .expect("write");
        let override_path = root.join(".decapod").join("OVERRIDE.md");
        let content = fs::read_to_string(&override_path).expect("override");
        assert!(content.contains(CONTAINER_DISABLE_MARKER));
        assert!(content.contains("warning: disabling isolated containers"));
        assert!(container_runtime_disabled(&root).expect("disabled check"));
        let _ = fs::remove_dir_all(root);
    }
}
