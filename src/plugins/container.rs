use crate::core::error;
use crate::core::time;
use crate::core::store::Store;
use clap::{Parser, Subcommand, ValueEnum};
use serde_json::json;
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
    /// Execute one command in a fresh container against the mounted repository.
    Run {
        #[clap(long)]
        agent: String,
        #[clap(long)]
        cmd: String,
        #[clap(long)]
        branch: Option<String>,
        #[clap(long, default_value_t = false)]
        push: bool,
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
    },
}

#[derive(Debug, Clone)]
struct DockerSpec {
    args: Vec<String>,
    container_name: String,
}

pub fn run_container_cli(store: &Store, cli: ContainerCli) -> Result<(), error::DecapodError> {
    match cli.command {
        ContainerCommand::Run {
            agent,
            cmd,
            branch,
            push,
            image_profile,
            image,
            timeout_seconds,
            memory,
            cpus,
            repo,
        } => run_container(
            store,
            &agent,
            &cmd,
            branch.as_deref(),
            push,
            image_profile,
            image.as_deref(),
            timeout_seconds,
            &memory,
            &cpus,
            repo.as_deref(),
        ),
    }
}

#[allow(clippy::too_many_arguments)]
fn run_container(
    _store: &Store,
    agent: &str,
    user_cmd: &str,
    branch: Option<&str>,
    push: bool,
    image_profile: ImageProfile,
    image_override: Option<&str>,
    timeout_seconds: u64,
    memory: &str,
    cpus: &str,
    repo_override: Option<&str>,
) -> Result<(), error::DecapodError> {
    let repo = resolve_repo_path(repo_override)?;
    let docker = find_container_runtime()?;
    let image = image_override
        .map(|s| s.to_string())
        .unwrap_or_else(|| default_image_for_profile(image_profile).to_string());
    let spec = build_docker_spec(
        &docker,
        &repo,
        &image,
        agent,
        user_cmd,
        branch,
        push,
        memory,
        cpus,
    )?;

    let start = Instant::now();
    let mut child = Command::new(&docker)
        .args(&spec.args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(error::DecapodError::IoError)?;

    let timeout = Duration::from_secs(timeout_seconds);
    loop {
        if let Some(status) = child.try_wait().map_err(error::DecapodError::IoError)? {
            let output = child.wait_with_output().map_err(error::DecapodError::IoError)?;
            let elapsed = start.elapsed().as_secs();
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({
                    "ts": time::now_epoch_z(),
                    "cmd": "container.run",
                    "status": if status.success() { "ok" } else { "error" },
                    "agent": agent,
                    "runtime": docker,
                    "image": image,
                    "container_name": spec.container_name,
                    "repo": repo,
                    "branch": branch,
                    "push": push,
                    "exit_code": status.code(),
                    "elapsed_seconds": elapsed,
                    "stdout": String::from_utf8_lossy(&output.stdout),
                    "stderr": String::from_utf8_lossy(&output.stderr)
                }))
                .unwrap()
            );
            if status.success() {
                return Ok(());
            }
            return Err(error::DecapodError::ValidationError(format!(
                "Container command failed (exit {:?})",
                status.code()
            )));
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
        ImageProfile::DebianSlim => "rust:1.85-slim",
        ImageProfile::Alpine => "rust:1.85-alpine",
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

#[allow(clippy::too_many_arguments)]
fn build_docker_spec(
    runtime: &str,
    repo: &Path,
    image: &str,
    agent: &str,
    user_cmd: &str,
    branch: Option<&str>,
    push: bool,
    memory: &str,
    cpus: &str,
) -> Result<DockerSpec, error::DecapodError> {
    let repo_str = repo
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
        format!("DECAPOD_PUSH={}", if push { "1" } else { "0" }),
        "-v".to_string(),
        format!("{}:/workspace", repo_str),
        "-w".to_string(),
        "/workspace".to_string(),
    ];

    if let Some((uid, gid)) = current_uid_gid() {
        args.push("--user".to_string());
        args.push(format!("{}:{}", uid, gid));
    }

    if let Ok(sock) = std::env::var("SSH_AUTH_SOCK") {
        if !sock.trim().is_empty() {
            args.push("-e".to_string());
            args.push(format!("SSH_AUTH_SOCK={}", sock));
            args.push("-v".to_string());
            args.push(format!("{}:{}", sock, sock));
        }
    }

    if let Some(branch_name) = branch {
        args.push("-e".to_string());
        args.push(format!("DECAPOD_BRANCH={}", branch_name));
    }

    args.push(image.to_string());
    args.push("sh".to_string());
    args.push("-lc".to_string());
    args.push(build_container_script(user_cmd, branch, push));

    if runtime != "docker" && runtime != "podman" {
        return Err(error::DecapodError::ValidationError(format!(
            "Unsupported container runtime '{}'",
            runtime
        )));
    }

    Ok(DockerSpec {
        args,
        container_name,
    })
}

fn build_container_script(user_cmd: &str, branch: Option<&str>, push: bool) -> String {
    let mut script = String::from(
        "set -euo pipefail\ngit config --global --add safe.directory /workspace || true\n",
    );
    if let Some(branch_name) = branch {
        script.push_str(&format!("git checkout -B {}\n", shell_escape(branch_name)));
    }
    script.push_str(user_cmd);
    script.push('\n');
    if push {
        if let Some(branch_name) = branch {
            script.push_str(&format!(
                "git push -u origin {}\n",
                shell_escape(branch_name)
            ));
        } else {
            script.push_str("echo 'push requested but --branch missing' >&2\nexit 2\n");
        }
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

pub fn schema() -> serde_json::Value {
    json!({
        "name": "container",
        "version": "0.1.0",
        "description": "Ephemeral containerized agent execution with repo mount isolation",
        "commands": [
            { "name": "run", "parameters": ["agent", "cmd", "branch", "push", "image_profile", "image", "timeout_seconds", "memory", "cpus", "repo"] }
        ],
        "profiles": {
            "debian-slim": "rust:1.85-slim",
            "alpine": "rust:1.85-alpine"
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
    fn docker_spec_contains_safety_flags() {
        let repo = PathBuf::from("/tmp/repo");
        let spec = build_docker_spec(
            "docker",
            &repo,
            "rust:1.85-slim",
            "agent-a",
            "cargo test -q",
            Some("ahr/branch"),
            true,
            "2g",
            "2.0",
        )
        .expect("spec");

        let joined = spec.args.join(" ");
        assert!(joined.contains("--rm"));
        assert!(joined.contains("--cap-drop ALL"));
        assert!(joined.contains("--security-opt no-new-privileges:true"));
        assert!(joined.contains("git checkout -B 'ahr/branch'"));
        assert!(joined.contains("git push -u origin 'ahr/branch'"));
    }
}
