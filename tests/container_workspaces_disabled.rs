use std::fs;
use std::io::Write;
use std::process::{Command, Stdio};
use tempfile::TempDir;

fn setup_isolated_repo_with_worktree(base_dir: &std::path::Path) -> std::path::PathBuf {
    Command::new("git")
        .args(["init", "-q"])
        .current_dir(base_dir)
        .status()
        .expect("git init");
    Command::new("git")
        .args(["config", "user.email", "test@test.com"])
        .current_dir(base_dir)
        .status()
        .expect("git config email");
    Command::new("git")
        .args(["config", "user.name", "Test"])
        .current_dir(base_dir)
        .status()
        .expect("git config name");

    fs::write(base_dir.join("README.md"), "# test\n").expect("write readme");
    Command::new("git")
        .args(["add", "."])
        .current_dir(base_dir)
        .status()
        .expect("git add");
    Command::new("git")
        .args(["commit", "-m", "init"])
        .current_dir(base_dir)
        .status()
        .expect("git commit");

    let init_out = Command::new(env!("CARGO_BIN_EXE_decapod"))
        .args(["init", "--force"])
        .current_dir(base_dir)
        .output()
        .expect("decapod init");
    assert!(
        init_out.status.success(),
        "init failed: {}",
        String::from_utf8_lossy(&init_out.stderr)
    );

    fs::write(base_dir.join("test.rs"), "fn main() {}\n").expect("write test file");
    Command::new("git")
        .args(["add", "-A"])
        .current_dir(base_dir)
        .status()
        .expect("git add all");
    Command::new("git")
        .args(["commit", "-m", "add test file"])
        .current_dir(base_dir)
        .status()
        .expect("git commit");

    let worktree_dir = base_dir.join(".decapod/workspaces/test-cw-worktree");
    Command::new("git")
        .args([
            "worktree",
            "add",
            "-b",
            "agent/test-cw",
            worktree_dir.to_str().unwrap(),
        ])
        .current_dir(base_dir)
        .status()
        .expect("git worktree add");

    worktree_dir
}

fn run_decapod_validate(
    dir: &std::path::Path,
    extra_envs: &[(&str, &str)],
) -> std::process::Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_decapod"));
    cmd.args(["validate", "-v"]).current_dir(dir);
    for (k, v) in extra_envs {
        cmd.env(k, v);
    }
    cmd.output().expect("decapod validate")
}

fn acquire_session(dir: &std::path::Path) -> String {
    let out = Command::new(env!("CARGO_BIN_EXE_decapod"))
        .args(["session", "acquire"])
        .env("DECAPOD_AGENT_ID", "test-agent-cw")
        .current_dir(dir)
        .output()
        .expect("session acquire");
    assert!(
        out.status.success(),
        "session acquire failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let stdout = String::from_utf8_lossy(&out.stdout);
    stdout
        .lines()
        .find(|l| l.starts_with("Password: "))
        .expect("Password in output")
        .strip_prefix("Password: ")
        .unwrap()
        .trim()
        .to_string()
}

fn set_container_workspaces_in_config(worktree_dir: &std::path::Path, enabled: bool) {
    let config_path = worktree_dir.join(".decapod").join("config.toml");
    let content = fs::read_to_string(&config_path).expect("read config");

    let new_content = if content.contains("container_workspaces") {
        content
            .lines()
            .map(|line| {
                if line.trim().starts_with("container_workspaces") {
                    format!("container_workspaces = {}", enabled)
                } else {
                    line.to_string()
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    } else {
        format!("{}\ncontainer_workspaces = {}", content.trim(), enabled)
    };

    fs::write(&config_path, new_content).expect("write config");
}

#[test]
fn test_container_workspaces_false_skips_container_requirement() {
    let tmp = TempDir::new().expect("tempdir");
    let base_dir = tmp.path();
    let worktree_dir = setup_isolated_repo_with_worktree(base_dir);

    set_container_workspaces_in_config(&worktree_dir, false);

    let out = run_decapod_validate(&worktree_dir, &[]);

    let stderr = String::from_utf8_lossy(&out.stderr);
    let stdout = String::from_utf8_lossy(&out.stdout);

    assert!(
        !stderr.contains("container_workspace_required"),
        "should NOT fail container_workspace_required when disabled. Stderr: {}",
        stderr
    );

    assert!(
        stdout.contains("skip") || stdout.contains("disabled") || out.status.success(),
        "should skip container check when container_workspaces disabled. stdout: {}",
        stdout
    );
}

#[test]
fn test_container_workspaces_true_requires_container() {
    let tmp = TempDir::new().expect("tempdir");
    let base_dir = tmp.path();
    let worktree_dir = setup_isolated_repo_with_worktree(base_dir);

    set_container_workspaces_in_config(&worktree_dir, true);

    let out = run_decapod_validate(&worktree_dir, &[]);

    let stderr = String::from_utf8_lossy(&out.stderr);

    assert!(
        stderr.contains("container_workspace_required"),
        "should require container when container_workspaces = true. Stderr: {}",
        stderr
    );
}

#[test]
fn test_init_with_no_container_workspaces_flag() {
    let tmp = TempDir::new().expect("tempdir");
    let base_dir = tmp.path();

    Command::new("git")
        .args(["init", "-q"])
        .current_dir(base_dir)
        .status()
        .expect("git init");
    Command::new("git")
        .args(["config", "user.email", "test@test.com"])
        .current_dir(base_dir)
        .status()
        .expect("git config email");
    Command::new("git")
        .args(["config", "user.name", "Test"])
        .current_dir(base_dir)
        .status()
        .expect("git config name");

    fs::write(base_dir.join("README.md"), "# test\n").expect("write readme");
    Command::new("git")
        .args(["add", "."])
        .current_dir(base_dir)
        .status()
        .expect("git add");
    Command::new("git")
        .args(["commit", "-m", "init"])
        .current_dir(base_dir)
        .status()
        .expect("git commit");

    let out = Command::new(env!("CARGO_BIN_EXE_decapod"))
        .args(["init", "--force", "--no-container-workspaces"])
        .current_dir(base_dir)
        .output()
        .expect("decapod init --no-container-workspaces");

    assert!(
        out.status.success(),
        "init failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let config_path = base_dir.join(".decapod").join("config.toml");
    let config_content = fs::read_to_string(&config_path).expect("read config");
    assert!(
        config_content.contains("container_workspaces = false"),
        "config should have container_workspaces = false. Content: {}",
        config_content
    );
}

#[test]
fn test_agent_rpc_with_container_workspaces_disabled() {
    let tmp = TempDir::new().expect("tempdir");
    let base_dir = tmp.path();
    let worktree_dir = setup_isolated_repo_with_worktree(base_dir);

    set_container_workspaces_in_config(&worktree_dir, false);

    let password = acquire_session(&worktree_dir);

    let request = serde_json::json!({
        "jsonrpc": "2.0",
        "id": "test-1",
        "op": "agent.init",
        "params": {}
    });

    let mut child = Command::new(env!("CARGO_BIN_EXE_decapod"))
        .args(["rpc", "--stdin"])
        .current_dir(&worktree_dir)
        .env("DECAPOD_AGENT_ID", "test-agent-cw")
        .env("DECAPOD_SESSION_PASSWORD", &password)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("spawn rpc");

    let mut stdin = child.stdin.take().expect("stdin");
    stdin
        .write_all(serde_json::to_string(&request).unwrap().as_bytes())
        .expect("write rpc request");
    drop(stdin);

    let output = child.wait_with_output().expect("wait for rpc");
    assert!(
        output.status.success(),
        "rpc should succeed with container_workspaces disabled. Stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn test_todo_operations_with_container_workspaces_disabled() {
    let tmp = TempDir::new().expect("tempdir");
    let base_dir = tmp.path();
    let worktree_dir = setup_isolated_repo_with_worktree(base_dir);

    set_container_workspaces_in_config(&worktree_dir, false);

    let password = acquire_session(&worktree_dir);

    let out = Command::new(env!("CARGO_BIN_EXE_decapod"))
        .args(["todo", "add", "test container_workspaces disabled todo"])
        .current_dir(&worktree_dir)
        .env("DECAPOD_AGENT_ID", "test-agent-cw")
        .env("DECAPOD_SESSION_PASSWORD", &password)
        .output()
        .expect("todo add");

    assert!(
        out.status.success(),
        "todo add should succeed with container_workspaces disabled. Stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn test_e2e_validate_passes_with_no_container_workspaces() {
    let tmp = TempDir::new().expect("tempdir");
    let base_dir = tmp.path();
    let worktree_dir = setup_isolated_repo_with_worktree(base_dir);

    set_container_workspaces_in_config(&worktree_dir, false);

    let out = run_decapod_validate(&worktree_dir, &[]);

    let stdout = String::from_utf8_lossy(&out.stdout);

    assert!(
        out.status.success(),
        "validate should pass with container_workspaces disabled. stdout: {}",
        stdout
    );

    assert!(
        !stdout.contains("container_workspace_required"),
        "should not have container_workspace_required failure. stdout: {}",
        stdout
    );
}

#[test]
fn test_e2e_validate_container_sequences_never_fire_when_disabled() {
    let tmp = TempDir::new().expect("tempdir");
    let base_dir = tmp.path();
    let worktree_dir = setup_isolated_repo_with_worktree(base_dir);

    set_container_workspaces_in_config(&worktree_dir, false);

    let out = run_decapod_validate(&worktree_dir, &[]);

    let stderr = String::from_utf8_lossy(&out.stderr);
    let stdout = String::from_utf8_lossy(&out.stdout);

    assert!(
        !stderr.contains("container") && !stdout.contains("container"),
        "container sequences should never fire when container_workspaces disabled. stderr: {}, stdout: {}",
        stderr,
        stdout
    );

    assert!(
        !stderr.contains("docker run") && !stdout.contains("docker run"),
        "docker run should never execute when container_workspaces disabled. stderr: {}, stdout: {}",
        stderr,
        stdout
    );

    assert!(
        !stderr.contains("podman run") && !stdout.contains("podman run"),
        "podman run should never execute when container_workspaces disabled. stderr: {}, stdout: {}",
        stderr,
        stdout
    );

    assert!(
        !stderr.contains("ensure --container") && !stdout.contains("ensure --container"),
        "workspace ensure --container should never be triggered when container_workspaces disabled. stderr: {}, stdout: {}",
        stderr,
        stdout
    );
}

#[test]
fn test_e2e_init_then_validate_flow_with_workspaces_disabled() {
    let tmp = TempDir::new().expect("tempdir");
    let base_dir = tmp.path();

    Command::new("git")
        .args(["init", "-q"])
        .current_dir(base_dir)
        .status()
        .expect("git init");
    Command::new("git")
        .args(["config", "user.email", "test@test.com"])
        .current_dir(base_dir)
        .status()
        .expect("git config email");
    Command::new("git")
        .args(["config", "user.name", "Test"])
        .current_dir(base_dir)
        .status()
        .expect("git config name");

    fs::write(base_dir.join("README.md"), "# test\n").expect("write readme");
    Command::new("git")
        .args(["add", "."])
        .current_dir(base_dir)
        .status()
        .expect("git add");
    Command::new("git")
        .args(["commit", "-m", "init"])
        .current_dir(base_dir)
        .status()
        .expect("git commit");

    let init_out = Command::new(env!("CARGO_BIN_EXE_decapod"))
        .args(["init", "--force", "--no-container-workspaces"])
        .current_dir(base_dir)
        .output()
        .expect("decapod init --no-container-workspaces");
    assert!(init_out.status.success(), "init should succeed");

    let config_path = base_dir.join(".decapod").join("config.toml");
    let config_content = fs::read_to_string(&config_path).expect("read config");
    assert!(
        config_content.contains("container_workspaces = false"),
        "container_workspaces should be set to false in config after init --no-container-workspaces"
    );

    fs::write(base_dir.join("test.rs"), "fn main() {}\n").expect("write test file");
    Command::new("git")
        .args(["add", "-A"])
        .current_dir(base_dir)
        .status()
        .expect("git add all");
    Command::new("git")
        .args(["commit", "-m", "add test file"])
        .current_dir(base_dir)
        .status()
        .expect("git commit");

    let worktree_dir = base_dir.join(".decapod/workspaces/test-cw-init-flow");
    Command::new("git")
        .args([
            "worktree",
            "add",
            "-b",
            "agent/test-cw-init",
            worktree_dir.to_str().unwrap(),
        ])
        .current_dir(base_dir)
        .status()
        .expect("git worktree add");

    let validate_out = run_decapod_validate(&worktree_dir, &[]);
    let stdout = String::from_utf8_lossy(&validate_out.stdout);
    let stderr = String::from_utf8_lossy(&validate_out.stderr);

    assert!(
        validate_out.status.success(),
        "validate should pass after init --no-container-workspaces in worktree. stdout: {}, stderr: {}",
        stdout,
        stderr
    );

    assert!(
        !stderr.contains("container_workspace_required"),
        "container_workspace_required should never appear. stderr: {}",
        stderr
    );
}
