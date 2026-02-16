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

    let push = env_bool("DECAPOD_CLAIM_PUSH", true);
    let pr = env_bool("DECAPOD_CLAIM_PR", true);
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
) -> Result<RunSummary, error::DecapodError> {
    let repo = resolve_repo_path(repo_override)?;
    let docker = find_container_runtime()?;
    let image = resolve_runtime_image(&docker, &repo, image_profile, image_override)?;

    let branch_name = branch
        .map(|s| s.to_string())
        .unwrap_or_else(|| default_branch_name(agent, task_id));
    let workspace = prepare_workspace_clone(&repo, &branch_name, pr_base)?;

    let pr_title_val = pr_title
        .map(|s| s.to_string())
        .unwrap_or_else(|| format!("{}", branch_name));
    let pr_body_val = pr_body
        .map(|s| s.to_string())
        .unwrap_or_else(|| format!("Automated update from {}", branch_name));

    let spec = build_docker_spec(
        &docker,
        &repo,
        &workspace.path,
        &image,
        agent,
        user_cmd,
        &workspace.branch,
        &workspace.base_branch,
        push,
        pr,
        &pr_title_val,
        &pr_body_val,
        memory,
        cpus,
        task_id,
        inherit_env,
    )?;

    let start = Instant::now();
    let output = execute_container_with_timeout(&docker, &spec.args, timeout_seconds)?;
    let elapsed = start.elapsed().as_secs();

    let status = if output.status.success() {
        "ok"
    } else {
        "error"
    };
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
        "task_id": task_id,
        "push": push,
        "pr": pr,
        "keep_worktree": keep_worktree,
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

fn git_output(repo: &Path, args: &[&str]) -> Result<String, error::DecapodError> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .map_err(error::DecapodError::IoError)?;
    if !output.status.success() {
        return Err(error::DecapodError::ValidationError(format!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn prepare_workspace_clone(
    repo: &Path,
    branch: &str,
    base_branch: &str,
) -> Result<WorkspaceSpec, error::DecapodError> {
    run_git(repo, &["fetch", "origin", base_branch])?;
    let origin_url = git_output(repo, &["remote", "get-url", "origin"])?;

    let workspaces_root = repo.join(".decapod").join("workspaces");
    fs::create_dir_all(&workspaces_root).map_err(error::DecapodError::IoError)?;

    let suffix = Ulid::new().to_string().to_lowercase();
    let dir_name = format!("{}-{}", sanitize_branch_component(branch), &suffix[..8]);
    let workspace_path = workspaces_root.join(dir_name);
    let base_ref = format!("origin/{}", base_branch);

    let workspace_path_str = workspace_path
        .to_str()
        .ok_or_else(|| error::DecapodError::PathError("invalid workspace path".to_string()))?;

    let clone_output = Command::new("git")
        .arg("clone")
        .arg("--origin")
        .arg("origin")
        .arg("--branch")
        .arg(base_branch)
        .arg("--single-branch")
        .arg(&origin_url)
        .arg(workspace_path_str)
        .output()
        .map_err(error::DecapodError::IoError)?;
    if !clone_output.status.success() {
        return Err(error::DecapodError::ValidationError(format!(
            "git clone failed: {}",
            String::from_utf8_lossy(&clone_output.stderr).trim()
        )));
    }
    run_git(&workspace_path, &["checkout", "-B", branch, &base_ref])?;

    Ok(WorkspaceSpec {
        branch: branch.to_string(),
        path: workspace_path,
        base_branch: base_branch.to_string(),
        backend: "clone".to_string(),
    })
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
    push: bool,
    pr: bool,
    pr_title: &str,
    pr_body: &str,
    memory: &str,
    cpus: &str,
    task_id: Option<&str>,
    inherit_env: bool,
) -> Result<DockerSpec, error::DecapodError> {
    let decapod_dir = repo_root.join(".decapod");
    fs::create_dir_all(&decapod_dir).map_err(error::DecapodError::IoError)?;
    let repo_root_str = repo_root
        .to_str()
        .ok_or_else(|| error::DecapodError::PathError("invalid repo root path".to_string()))?;
    let workspace_str = workspace
        .to_str()
        .ok_or_else(|| error::DecapodError::PathError("invalid repository path".to_string()))?;
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
        format!("DECAPOD_PUSH={}", if push { "1" } else { "0" }),
        "-e".to_string(),
        format!("DECAPOD_PR={}", if pr { "1" } else { "0" }),
        "-e".to_string(),
        format!("DECAPOD_WORKSPACE={}", workspace_str),
        "-v".to_string(),
        format!("{}:{}", repo_root_str, repo_root_str),
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

    if env_bool("DECAPOD_CONTAINER_MAP_HOST_USER", false) {
        if let Some((uid, gid)) = current_uid_gid() {
            args.push("--user".to_string());
            args.push(format!("{}:{}", uid, gid));
        }
    }

    if let Ok(sock) = std::env::var("SSH_AUTH_SOCK") {
        if !sock.trim().is_empty() {
            args.push("-e".to_string());
            args.push(format!("SSH_AUTH_SOCK={}", sock));
            args.push("-v".to_string());
            args.push(format!("{}:{}", sock, sock));
        }
    }
    let ssh_dir = std::env::var("DECAPOD_CONTAINER_SSH_DIR")
        .ok()
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".ssh")));
    if let Some(ssh_dir) = ssh_dir {
        if ssh_dir.exists() {
            if let Some(ssh_dir_str) = ssh_dir.to_str() {
                args.push("-v".to_string());
                args.push(format!("{}:/tmp/decapod-ssh:ro", ssh_dir_str));
            }
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
    args.push(build_container_script(
        user_cmd,
        branch,
        base_branch,
        push,
        pr,
        pr_title,
        pr_body,
    ));

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

fn build_container_script(
    user_cmd: &str,
    branch: &str,
    base_branch: &str,
    push: bool,
    pr: bool,
    pr_title: &str,
    pr_body: &str,
) -> String {
    let mut script = String::from(
        "set -eu\n\
         cd \"${DECAPOD_WORKSPACE:-$PWD}\"\n\
         mkdir -p \"${HOME:-/tmp/decapod-home}\"\n\
         mkdir -p \"${HOME:-/tmp/decapod-home}/.ssh\"\n\
         chmod 700 \"${HOME:-/tmp/decapod-home}/.ssh\"\n\
         if command -v ssh-keyscan >/dev/null 2>&1; then\n\
           ssh-keyscan -t ed25519 github.com >> \"${HOME:-/tmp/decapod-home}/.ssh/known_hosts\" 2>/dev/null || true\n\
         fi\n\
         export GIT_SSH_COMMAND=\"ssh -o StrictHostKeyChecking=accept-new -o UserKnownHostsFile=${HOME:-/tmp/decapod-home}/.ssh/known_hosts\"\n\
         if [ -f /tmp/decapod-ssh/id_ed25519 ]; then\n\
           chmod 600 /tmp/decapod-ssh/id_ed25519 2>/dev/null || true\n\
           export GIT_SSH_COMMAND=\"${GIT_SSH_COMMAND} -i /tmp/decapod-ssh/id_ed25519 -o IdentitiesOnly=yes\"\n\
         elif [ -S \"${SSH_AUTH_SOCK:-}\" ]; then\n\
           export GIT_SSH_COMMAND=\"${GIT_SSH_COMMAND} -o IdentityAgent=${SSH_AUTH_SOCK}\"\n\
         fi\n\
         if [ \"${DECAPOD_CONTAINER_DEBUG:-0}\" = \"1\" ]; then\n\
           echo \"debug: workspace=${DECAPOD_WORKSPACE:-$PWD}\" >&2\n\
           echo \"debug: ssh command=${GIT_SSH_COMMAND}\" >&2\n\
           ls -ld /tmp/decapod-ssh /tmp/decapod-ssh/id_ed25519 2>/dev/null >&2 || true\n\
           git remote -v >&2 || true\n\
         fi\n\
         unset GIT_DIR GIT_WORK_TREE\n\
         git config --global --add safe.directory \"${DECAPOD_WORKSPACE:-$PWD}\" || true\n\
         git config --global user.name \"${DECAPOD_GIT_USER_NAME:-Decapod Agent}\"\n\
         git config --global user.email \"${DECAPOD_GIT_USER_EMAIL:-agent@decapod.local}\"\n\
         if command -v decapod >/dev/null 2>&1; then\n\
           decapod --version >/dev/null 2>&1 || true\n\
           if decapod --help 2>/dev/null | grep -qE \"(^|[[:space:]])update([[:space:]]|$)\"; then\n\
             decapod update\n\
           fi\n\
         fi\n",
    );
    script.push_str(&format!(
        "git fetch --no-write-fetch-head origin {}\n",
        shell_escape(base_branch)
    ));
    script.push_str(&format!("git checkout -B {}\n", shell_escape(branch)));
    script.push_str(&format!(
        "git rebase origin/{}\n",
        shell_escape(base_branch)
    ));
    script.push_str(user_cmd);
    script.push('\n');

    script.push_str(
        "if [ -n \"$(git status --porcelain)\" ]; then\n  git add -A\n  git commit -m \"chore: automated container updates\"\nfi\n",
    );

    if push || pr {
        script.push_str(&format!("git push -u origin {}\n", shell_escape(branch)));
    }

    if pr {
        script.push_str("if ! command -v gh >/dev/null 2>&1; then echo 'gh CLI required for PR creation' >&2; exit 2; fi\n");
        script.push_str("if ! gh auth status >/dev/null 2>&1; then echo 'gh auth required for PR creation (run gh auth login)' >&2; exit 2; fi\n");
        script.push_str(&format!(
            "if gh pr view --head {} >/dev/null 2>&1; then\n  echo 'PR already exists for branch'\nelse\n  gh pr create --base {} --head {} --title {} --body {}\nfi\n",
            shell_escape(branch),
            shell_escape(base_branch),
            shell_escape(branch),
            shell_escape(pr_title),
            shell_escape(pr_body)
        ));
    }

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
        "description": "Ephemeral containerized agent execution with isolated clone workspaces and optional push/PR automation",
        "commands": [
            { "name": "run", "parameters": ["agent", "cmd", "branch", "task_id", "push", "pr", "pr_base", "pr_title", "pr_body", "image_profile", "image", "timeout_seconds", "memory", "cpus", "repo", "keep_worktree", "inherit_env"] }
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
        let spec = build_docker_spec(
            "docker",
            &repo,
            &repo,
            "rust:1.85",
            "agent-a",
            "cargo test -q",
            "ahr/branch",
            "master",
            true,
            true,
            "title",
            "body",
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
        assert!(joined.contains("git fetch --no-write-fetch-head origin 'master'"));
        assert!(joined.contains("git checkout -B 'ahr/branch'"));
        assert!(joined.contains("git rebase origin/'master'"));
        assert!(joined.contains("decapod update"));
        assert!(joined.contains("git push -u origin 'ahr/branch'"));
        assert!(joined.contains("gh auth status"));
        assert!(joined.contains("gh pr create --base 'master' --head 'ahr/branch'"));
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
}
