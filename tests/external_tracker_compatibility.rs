use std::fs;
use std::process::Command;
use tempfile::TempDir;

fn setup_repo(dir: &std::path::Path) {
    Command::new("git")
        .args(["init", "-q"])
        .current_dir(dir)
        .status()
        .expect("git init");
    Command::new("git")
        .args(["config", "user.email", "test@test.com"])
        .current_dir(dir)
        .status()
        .expect("git config email");
    Command::new("git")
        .args(["config", "user.name", "Test"])
        .current_dir(dir)
        .status()
        .expect("git config name");

    fs::write(dir.join("README.md"), "# test\n").expect("write readme");
    Command::new("git")
        .args(["add", "."])
        .current_dir(dir)
        .status()
        .expect("git add");
    Command::new("git")
        .args(["commit", "-m", "init"])
        .current_dir(dir)
        .status()
        .expect("git commit");

    let init_out = Command::new(env!("CARGO_BIN_EXE_decapod"))
        .args(["init", "--force"])
        .current_dir(dir)
        .output()
        .expect("decapod init");
    assert!(init_out.status.success(), "init failed");

    // Commit everything including decapod files so the repo is "clean"
    Command::new("git")
        .args(["add", "-A"])
        .current_dir(dir)
        .status()
        .expect("git add all");
    Command::new("git")
        .args(["commit", "-m", "decapod init"])
        .current_dir(dir)
        .status()
        .expect("git commit init");
}

#[test]
fn test_external_tracker_env_var_relaxation() {
    let tmp = TempDir::new().expect("tempdir");
    let dir = tmp.path();
    setup_repo(dir);

    // 1. Create a non-scoped worktree branch
    let branch = "feature/BEADS-123";
    let worktree_dir = dir.join(".decapod/workspaces/test-worktree");
    // Don't mkdir, git worktree add will do it or fail if exists

    Command::new("git")
        .args([
            "worktree",
            "add",
            "-b",
            branch,
            worktree_dir.to_str().unwrap(),
        ])
        .current_dir(dir)
        .status()
        .expect("git worktree add");

    // 3. Run without env var - should fail
    let out = Command::new(env!("CARGO_BIN_EXE_decapod"))
        .args(["workspace", "ensure"])
        .current_dir(&worktree_dir)
        .output()
        .expect("workspace ensure");

    let stderr = String::from_utf8_lossy(&out.stderr);
    let stdout = String::from_utf8_lossy(&out.stdout);

    // If it succeeded, it might have created a NEW worktree because it didn't like the current one.
    // We want to ensure it doesn't just "succeed" by ignoring the current worktree and making a new one.
    if out.status.success() {
        let json: serde_json::Value = serde_json::from_str(&stdout).expect("valid json");
        assert_ne!(
            json["branch"], branch,
            "should NOT have stayed on branch without tracker signal. JSON: {}",
            stdout
        );
    } else {
        assert!(
            stderr.contains("WORKSPACE_BRANCH_NOT_TODO_SCOPED"),
            "missing error code in: {}",
            stderr
        );
    }

    // 4. Run WITH env var - should succeed and STAY
    let out = Command::new(env!("CARGO_BIN_EXE_decapod"))
        .args(["workspace", "ensure"])
        .current_dir(&worktree_dir)
        .env("BEADS_TASK_ID", "123")
        .output()
        .expect("workspace ensure with env");

    assert!(
        out.status.success(),
        "should succeed with BEADS_TASK_ID. Stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let stdout = String::from_utf8_lossy(&out.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout).expect("valid json");
    assert_eq!(
        json["branch"], branch,
        "should have stayed on branch with BEADS_TASK_ID. JSON: {}",
        stdout
    );
    assert_eq!(json["status"], "ok");
}

#[test]
fn test_external_tracker_override_md_relaxation() {
    let tmp = TempDir::new().expect("tempdir");
    let dir = tmp.path();
    setup_repo(dir);

    let branch = "feature/external-task";
    let worktree_dir = dir.join(".decapod/workspaces/override-test");

    Command::new("git")
        .args([
            "worktree",
            "add",
            "-b",
            branch,
            worktree_dir.to_str().unwrap(),
        ])
        .current_dir(dir)
        .status()
        .expect("git worktree add");

    // 1. Add marker to OVERRIDE.md
    let override_path = dir.join(".decapod/OVERRIDE.md");
    let mut content = fs::read_to_string(&override_path).expect("read override");
    content.push_str("\nDECAPOD_EXTERNAL_TRACKER=true\n");
    fs::write(&override_path, content).expect("write override");

    // 2. Run - should succeed even without env var and STAY
    let out = Command::new(env!("CARGO_BIN_EXE_decapod"))
        .args(["workspace", "ensure"])
        .current_dir(&worktree_dir)
        .output()
        .expect("workspace ensure with override.md");

    assert!(
        out.status.success(),
        "should succeed with OVERRIDE.md marker. Stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let stdout = String::from_utf8_lossy(&out.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout).expect("valid json");
    assert_eq!(
        json["branch"], branch,
        "should have stayed on branch with OVERRIDE.md marker. JSON: {}",
        stdout
    );
    assert_eq!(json["status"], "ok");
}

#[test]
fn test_external_tracker_config_toml_relaxation() {
    let tmp = TempDir::new().expect("tempdir");
    let dir = tmp.path();
    setup_repo(dir);

    let branch = "feature/config-toml-task";
    let worktree_dir = dir.join(".decapod/workspaces/config-test");

    Command::new("git")
        .args([
            "worktree",
            "add",
            "-b",
            branch,
            worktree_dir.to_str().unwrap(),
        ])
        .current_dir(dir)
        .status()
        .expect("git worktree add");

    // 1. Add toggle to config.toml
    let config_path = dir.join(".decapod/config.toml");
    let mut content = fs::read_to_string(&config_path).expect("read config");
    content.push_str("\nexternal_tracker = true\n");
    fs::write(&config_path, content).expect("write config");

    // 2. Run - should succeed and STAY
    let out = Command::new(env!("CARGO_BIN_EXE_decapod"))
        .args(["workspace", "ensure"])
        .current_dir(&worktree_dir)
        .output()
        .expect("workspace ensure with config.toml");

    assert!(
        out.status.success(),
        "should succeed with config.toml toggle. Stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let stdout = String::from_utf8_lossy(&out.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout).expect("valid json");
    assert_eq!(
        json["branch"], branch,
        "should have stayed on branch with config.toml toggle. JSON: {}",
        stdout
    );
    assert_eq!(json["status"], "ok");
}

#[test]
fn test_workspace_ensure_json_orchestration_data() {
    let tmp = TempDir::new().expect("tempdir");
    let dir = tmp.path();
    setup_repo(dir);

    // Run workspace ensure --container on the main repo (protected branch)
    // This should fail/block but provide JSON data for orchestration
    let out = Command::new(env!("CARGO_BIN_EXE_decapod"))
        .args(["workspace", "ensure", "--container"])
        .current_dir(dir)
        .output()
        .expect("workspace ensure --container");

    let stdout = String::from_utf8_lossy(&out.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout).expect("valid json");

    // Status should be pending/ok depending on blockers
    // But importantly, it should contain blockers and required_actions
    assert!(
        json.get("blockers").is_some(),
        "missing blockers in JSON: {}",
        stdout
    );
    assert!(
        json.get("required_actions").is_some(),
        "missing required_actions in JSON: {}",
        stdout
    );

    let blockers = json["blockers"].as_array().expect("blockers is array");
    assert!(
        !blockers.is_empty(),
        "should have blockers for protected branch + container request"
    );

    // Check for resolve_hint if container environment is being prepared
    // In this test environment, it might just be the "on protected branch" blocker
    let has_workspace_blocker = blockers.iter().any(|b| b["kind"] == "workspace_required");
    if has_workspace_blocker {
        let blocker = blockers
            .iter()
            .find(|b| b["kind"] == "workspace_required")
            .unwrap();
        let hint = blocker["resolve_hint"].as_str().unwrap();
        // It should start with either docker or podman
        assert!(
            hint.starts_with("docker ") || hint.starts_with("podman "),
            "hint should use detected runtime: {}",
            hint
        );
    }
}
