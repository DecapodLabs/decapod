#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::tempdir;

fn decapod_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_decapod"))
}

fn git(dir: &Path, args: &[&str]) {
    let output = Command::new("git")
        .current_dir(dir)
        .args(args)
        .output()
        .expect("run git");
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

struct TestWorkspace {
    repo: PathBuf,
    worktree: PathBuf,
    task_id: String,
}

fn init_test_workspace(root: &Path) -> TestWorkspace {
    let repo = root.join("repo");
    fs::create_dir_all(&repo).expect("mkdir repo");
    git(&repo, &["init", "-b", "master"]);
    git(&repo, &["config", "user.name", "Podman 1820 Test"]);
    git(
        &repo,
        &["config", "user.email", "podman1820@example.invalid"],
    );
    fs::write(repo.join("README.md"), "# test\n").expect("write README");
    git(&repo, &["add", "README.md"]);
    git(&repo, &["commit", "-m", "initial"]);

    let init = Command::new(decapod_bin())
        .args(["init", "--force"])
        .current_dir(&repo)
        .env("DECAPOD_AGENT_ID", "podman-1820-test")
        .output()
        .expect("decapod init");
    assert!(
        init.status.success(),
        "decapod init failed: {}\n{}",
        String::from_utf8_lossy(&init.stdout),
        String::from_utf8_lossy(&init.stderr)
    );
    git(&repo, &["add", "-A"]);
    git(&repo, &["commit", "-m", "chore: decapod init"]);

    let add = Command::new(decapod_bin())
        .args([
            "todo",
            "add",
            "Test #1820 cgroup parent",
            "--format",
            "json",
        ])
        .current_dir(&repo)
        .env("DECAPOD_AGENT_ID", "podman-1820-test")
        .output()
        .expect("todo add");
    assert!(
        add.status.success(),
        "todo add failed: {}",
        String::from_utf8_lossy(&add.stderr)
    );
    let add_json: serde_json::Value = serde_json::from_slice(&add.stdout).expect("todo add json");
    let task_id = add_json["id"].as_str().expect("todo id").to_string();

    let claim = Command::new(decapod_bin())
        .args([
            "todo",
            "claim",
            "--id",
            &task_id,
            "--agent",
            "podman-1820-test",
        ])
        .current_dir(&repo)
        .env("DECAPOD_AGENT_ID", "podman-1820-test")
        .env("DECAPOD_CLAIM_AUTORUN", "0")
        .output()
        .expect("todo claim");
    assert!(
        claim.status.success(),
        "todo claim failed: {}",
        String::from_utf8_lossy(&claim.stderr)
    );

    let branch = format!("agent/podman-1820-test/{task_id}-cgroup");
    let ensure = Command::new(decapod_bin())
        .args(["workspace", "ensure", "--branch", &branch])
        .current_dir(&repo)
        .env("DECAPOD_AGENT_ID", "podman-1820-test")
        .env("DECAPOD_CLAIM_AUTORUN", "0")
        .output()
        .expect("workspace ensure");
    assert!(
        ensure.status.success(),
        "workspace ensure failed: {}\n{}",
        String::from_utf8_lossy(&ensure.stdout),
        String::from_utf8_lossy(&ensure.stderr)
    );
    let ensure_json: serde_json::Value =
        serde_json::from_slice(&ensure.stdout).expect("workspace ensure json");
    let worktree = PathBuf::from(
        ensure_json["worktree_path"]
            .as_str()
            .expect("worktree_path"),
    );

    TestWorkspace {
        repo,
        worktree,
        task_id,
    }
}

fn write_fake_podman(bin_dir: &Path, body: &str) -> PathBuf {
    fs::create_dir_all(bin_dir).expect("mkdir fake bin");
    let path = bin_dir.join("podman");
    let staging = bin_dir.join("podman.tmp");
    fs::write(&staging, body).expect("write fake podman");
    fs::set_permissions(&staging, fs::Permissions::from_mode(0o755)).expect("chmod fake podman");
    fs::rename(&staging, &path).expect("install fake podman");
    path
}

fn fake_path_with(bin_dir: &Path) -> std::ffi::OsString {
    let existing = std::env::var_os("PATH").unwrap_or_default();
    let mut entries = vec![bin_dir.to_path_buf()];
    entries.extend(std::env::split_paths(&existing));
    std::env::join_paths(entries).expect("join PATH")
}

#[test]
fn podman_container_run_passes_cli_cgroup_parent_and_preserves_resource_limits() {
    let temp = tempdir().expect("tempdir");
    let ws = init_test_workspace(temp.path());

    let bin_dir = temp.path().join("fake-bin");
    let args_log = temp.path().join("podman-run-args.log");
    write_fake_podman(
        &bin_dir,
        &format!(
            r#"#!/bin/sh
if [ "$1" = "info" ]; then
  cat <<'YAML'
host:
  cgroupManager: cgroupfs
  cgroupVersion: v2
  serviceIsRemote: false
  security:
    rootless: true
YAML
  exit 0
fi
if [ "$1" = "image" ] && [ "$2" = "inspect" ]; then
  exit 0
fi
if [ "$1" = "run" ]; then
  printf '%s\n' "$@" > '{}'
  printf 'ok\n'
  exit 0
fi
exit 0
"#,
            args_log.display()
        ),
    );

    let out = Command::new(decapod_bin())
        .args([
            "auto",
            "container",
            "run",
            "--agent",
            "podman-1820-test",
            "--task-id",
            &ws.task_id,
            "--repo",
            ws.repo.to_str().unwrap(),
            "--cmd",
            "true",
            "--image",
            "decapod-test:local",
            "--cgroup-parent",
            "/delegated/decapod",
            "--local-only",
        ])
        .current_dir(&ws.worktree)
        .env("PATH", fake_path_with(&bin_dir))
        .env("DECAPOD_AGENT_ID", "podman-1820-test")
        .env_remove("DECAPOD_CONTAINER_CGROUP_PARENT")
        .output()
        .expect("run decapod auto container run");

    assert!(
        out.status.success(),
        "expected success:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );

    let summary: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("parse container run json");
    assert_eq!(summary["runtime"], "podman");
    assert_eq!(summary["cgroup_parent"], "/delegated/decapod");

    let recorded = fs::read_to_string(&args_log).expect("read podman args");
    let argv: Vec<&str> = recorded.lines().collect();
    let cgroup_idx = argv
        .iter()
        .position(|a| *a == "--cgroup-parent")
        .expect("--cgroup-parent must be passed to podman run");
    assert_eq!(argv[cgroup_idx + 1], "/delegated/decapod");

    let pids_idx = argv
        .iter()
        .position(|a| *a == "--pids-limit")
        .expect("--pids-limit must remain enforced");
    assert_eq!(argv[pids_idx + 1], "512");

    let mem_idx = argv
        .iter()
        .position(|a| *a == "--memory")
        .expect("--memory must remain enforced");
    assert_eq!(argv[mem_idx + 1], "2g");

    let cpus_idx = argv
        .iter()
        .position(|a| *a == "--cpus")
        .expect("--cpus must remain enforced");
    assert_eq!(argv[cpus_idx + 1], "2.0");

    assert!(
        !argv.iter().any(|a| a.starts_with("--cgroups")),
        "launcher must never disable cgroups: {argv:?}"
    );
}

#[test]
fn podman_container_run_honors_env_cgroup_parent_and_cli_precedence() {
    let temp = tempdir().expect("tempdir");
    let ws = init_test_workspace(temp.path());

    let bin_dir = temp.path().join("fake-bin");
    let args_log = temp.path().join("podman-run-args.log");
    write_fake_podman(
        &bin_dir,
        &format!(
            r#"#!/bin/sh
if [ "$1" = "info" ]; then
  printf 'host:\n  cgroupManager: cgroupfs\n  cgroupVersion: v2\n  security:\n    rootless: true\n'
  exit 0
fi
if [ "$1" = "image" ] && [ "$2" = "inspect" ]; then
  exit 0
fi
if [ "$1" = "run" ]; then
  printf '%s\n' "$@" > '{}'
  exit 0
fi
exit 0
"#,
            args_log.display()
        ),
    );

    // 1. Environment variable supplies delegated cgroup parent.
    let env_run = Command::new(decapod_bin())
        .args([
            "auto",
            "container",
            "run",
            "--agent",
            "podman-1820-test",
            "--task-id",
            &ws.task_id,
            "--repo",
            ws.repo.to_str().unwrap(),
            "--cmd",
            "true",
            "--image",
            "decapod-test:local",
            "--local-only",
        ])
        .current_dir(&ws.worktree)
        .env("PATH", fake_path_with(&bin_dir))
        .env("DECAPOD_AGENT_ID", "podman-1820-test")
        .env("DECAPOD_CONTAINER_CGROUP_PARENT", "/delegated/from-env")
        .output()
        .expect("run with env cgroup parent");
    assert!(
        env_run.status.success(),
        "env run failed: {}",
        String::from_utf8_lossy(&env_run.stderr)
    );
    let recorded_env = fs::read_to_string(&args_log).expect("read env args");
    let env_argv: Vec<&str> = recorded_env.lines().collect();
    let idx = env_argv
        .iter()
        .position(|a| *a == "--cgroup-parent")
        .expect("env --cgroup-parent present");
    assert_eq!(env_argv[idx + 1], "/delegated/from-env");
    assert!(env_argv.contains(&"--pids-limit"));
    assert!(env_argv.contains(&"--memory"));
    assert!(env_argv.contains(&"--cpus"));

    // 2. Explicit CLI flag overrides environment variable.
    let cli_run = Command::new(decapod_bin())
        .args([
            "auto",
            "container",
            "run",
            "--agent",
            "podman-1820-test",
            "--task-id",
            &ws.task_id,
            "--repo",
            ws.repo.to_str().unwrap(),
            "--cmd",
            "true",
            "--image",
            "decapod-test:local",
            "--cgroup-parent",
            "/delegated/from-cli",
            "--local-only",
        ])
        .current_dir(&ws.worktree)
        .env("PATH", fake_path_with(&bin_dir))
        .env("DECAPOD_AGENT_ID", "podman-1820-test")
        .env("DECAPOD_CONTAINER_CGROUP_PARENT", "/delegated/from-env")
        .output()
        .expect("run with cli override");
    assert!(
        cli_run.status.success(),
        "cli override run failed: {}",
        String::from_utf8_lossy(&cli_run.stderr)
    );
    let recorded_cli = fs::read_to_string(&args_log).expect("read cli args");
    let cli_argv: Vec<&str> = recorded_cli.lines().collect();
    let idx = cli_argv
        .iter()
        .position(|a| *a == "--cgroup-parent")
        .expect("cli --cgroup-parent present");
    assert_eq!(cli_argv[idx + 1], "/delegated/from-cli");
}

#[test]
fn podman_container_run_rejects_invalid_cgroup_parent_before_runtime_launch() {
    let temp = tempdir().expect("tempdir");
    let ws = init_test_workspace(temp.path());

    let bin_dir = temp.path().join("fake-bin");
    let invoked_log = temp.path().join("podman-invoked.log");
    write_fake_podman(
        &bin_dir,
        &format!(
            "#!/bin/sh\nprintf '%s\\n' \"$*\" >> '{}'\nexit 0\n",
            invoked_log.display()
        ),
    );

    for bad_cli in [
        "/delegated/../escape",
        "/sys/fs/cgroup/delegated",
        "/delegated;id",
        "/delegated space",
        "/",
        "",
    ] {
        let out = Command::new(decapod_bin())
            .args([
                "auto",
                "container",
                "run",
                "--agent",
                "podman-1820-test",
                "--task-id",
                &ws.task_id,
                "--repo",
                ws.repo.to_str().unwrap(),
                "--cmd",
                "true",
                "--image",
                "decapod-test:local",
                "--cgroup-parent",
                bad_cli,
                "--local-only",
            ])
            .current_dir(&ws.worktree)
            .env("PATH", fake_path_with(&bin_dir))
            .env("DECAPOD_AGENT_ID", "podman-1820-test")
            .env_remove("DECAPOD_CONTAINER_CGROUP_PARENT")
            .output()
            .expect("run invalid cli cgroup parent");
        assert!(
            !out.status.success(),
            "invalid --cgroup-parent {bad_cli:?} must fail closed"
        );
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            stderr.contains("container_invalid_cgroup_parent"),
            "expected container_invalid_cgroup_parent for {bad_cli:?}, got: {stderr}"
        );
    }

    // Invalid environment variable must also fail closed before runtime launch.
    let bad_env = Command::new(decapod_bin())
        .args([
            "auto",
            "container",
            "run",
            "--agent",
            "podman-1820-test",
            "--task-id",
            &ws.task_id,
            "--repo",
            ws.repo.to_str().unwrap(),
            "--cmd",
            "true",
            "--image",
            "decapod-test:local",
            "--local-only",
        ])
        .current_dir(&ws.worktree)
        .env("PATH", fake_path_with(&bin_dir))
        .env("DECAPOD_AGENT_ID", "podman-1820-test")
        .env("DECAPOD_CONTAINER_CGROUP_PARENT", "--privileged")
        .output()
        .expect("run invalid env cgroup parent");
    assert!(!bad_env.status.success());
    assert!(String::from_utf8_lossy(&bad_env.stderr).contains("container_invalid_cgroup_parent"));
    assert!(
        !invoked_log.exists(),
        "container runtime must never be invoked when cgroup parent validation fails"
    );
}

#[test]
fn podman_container_run_distinguishes_cgroup_permission_from_workspace_permission() {
    let temp = tempdir().expect("tempdir");
    let ws = init_test_workspace(temp.path());

    let bin_dir = temp.path().join("fake-bin");
    write_fake_podman(
        &bin_dir,
        r#"#!/bin/sh
if [ "$1" = "info" ]; then
  printf 'host:\n  cgroupManager: cgroupfs\n  cgroupVersion: v2\n  security:\n    rootless: true\n'
  exit 0
fi
if [ "$1" = "image" ] && [ "$2" = "inspect" ]; then
  exit 0
fi
if [ "$1" = "run" ]; then
  if [ "$FAKE_FAILURE_MODE" = "cgroup" ]; then
    printf 'crun: cannot set limits without cgroups: create /sys/fs/cgroup/libpod-deadbeef: Permission denied\n' >&2
    exit 126
  else
    printf 'fatal: unable to create /workspace/.git/index.lock: Permission denied\n' >&2
    exit 128
  fi
fi
exit 0
"#,
    );

    let cgroup_err = Command::new(decapod_bin())
        .args([
            "auto",
            "container",
            "run",
            "--agent",
            "podman-1820-test",
            "--task-id",
            &ws.task_id,
            "--repo",
            ws.repo.to_str().unwrap(),
            "--cmd",
            "true",
            "--image",
            "decapod-test:local",
            "--local-only",
        ])
        .current_dir(&ws.worktree)
        .env("PATH", fake_path_with(&bin_dir))
        .env("DECAPOD_AGENT_ID", "podman-1820-test")
        .env("FAKE_FAILURE_MODE", "cgroup")
        .env_remove("DECAPOD_CONTAINER_CGROUP_PARENT")
        .output()
        .expect("run cgroup failure");
    assert!(!cgroup_err.status.success());
    let cgroup_stderr = String::from_utf8_lossy(&cgroup_err.stderr);
    assert!(
        cgroup_stderr.contains("code=container_cgroup_permission_denied"),
        "expected container_cgroup_permission_denied, got:\n{cgroup_stderr}"
    );
    assert!(
        !cgroup_stderr.contains("code=container_workspace_permission_denied"),
        "cgroup failure must not be misattributed to workspace permissions:\n{cgroup_stderr}"
    );
    assert!(cgroup_stderr.contains("--cgroup-parent"));
    assert!(cgroup_stderr.contains("DECAPOD_CONTAINER_CGROUP_PARENT"));

    let workspace_err = Command::new(decapod_bin())
        .args([
            "auto",
            "container",
            "run",
            "--agent",
            "podman-1820-test",
            "--task-id",
            &ws.task_id,
            "--repo",
            ws.repo.to_str().unwrap(),
            "--cmd",
            "true",
            "--image",
            "decapod-test:local",
            "--local-only",
        ])
        .current_dir(&ws.worktree)
        .env("PATH", fake_path_with(&bin_dir))
        .env("DECAPOD_AGENT_ID", "podman-1820-test")
        .env("FAKE_FAILURE_MODE", "workspace")
        .env_remove("DECAPOD_CONTAINER_CGROUP_PARENT")
        .output()
        .expect("run workspace failure");
    assert!(!workspace_err.status.success());
    let workspace_stderr = String::from_utf8_lossy(&workspace_err.stderr);
    assert!(
        workspace_stderr.contains("code=container_workspace_permission_denied"),
        "real workspace permission error must stay container_workspace_permission_denied, got:\n{workspace_stderr}"
    );
    assert!(
        !workspace_stderr.contains("code=container_cgroup_permission_denied"),
        "real workspace permission error must not be classified as cgroup error:\n{workspace_stderr}"
    );
}
