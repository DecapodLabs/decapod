use std::fs;
use std::path::Path;
use std::process::Command;
use tempfile::TempDir;

fn run_decapod(dir: &Path, args: &[&str], envs: &[(&str, &str)]) -> std::process::Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_decapod"));
    cmd.current_dir(dir).args(args);
    for (k, v) in envs {
        cmd.env(k, v);
    }
    cmd.output().expect("run decapod")
}

fn run_decapod_with_password(
    dir: &Path,
    args: &[&str],
    password: &str,
    extra_envs: &[(&str, &str)],
) -> std::process::Output {
    let mut envs = vec![
        ("DECAPOD_AGENT_ID", "git-init-test-agent"),
        ("DECAPOD_SESSION_PASSWORD", password),
    ];
    for (k, v) in extra_envs {
        envs.push((*k, *v));
    }
    run_decapod(dir, args, &envs)
}

#[test]
fn test_init_in_non_git_directory_with_git_opt_in() {
    let tmp = TempDir::new().expect("tmpdir");
    let dir = tmp.path();

    // Run decapod init with --git (explicit opt-in)
    let out = run_decapod(
        dir,
        &[
            "init",
            "with",
            "--force",
            "--git",
            "--product-name",
            "Test Git Project",
            "--product-summary",
            "Summary",
            "--primary-language",
            "Rust",
        ],
        &[],
    );

    assert!(
        out.status.success(),
        "decapod init with --git failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    // Verify .git exists
    assert!(dir.join(".git").exists(), "Expected .git to be created");

    // Verify git status succeeds
    let git_status = Command::new("git")
        .args(["status", "--short", "--branch"])
        .current_dir(dir)
        .output()
        .expect("git status");
    assert!(git_status.status.success(), "git status failed after init");
}

#[test]
fn test_init_in_existing_git_repository_preserves_state() {
    let tmp = TempDir::new().expect("tmpdir");
    let dir = tmp.path();

    // Manually run git init and create an initial commit
    let git_init = Command::new("git")
        .args(["init", "-b", "main"])
        .current_dir(dir)
        .output()
        .expect("git init");
    assert!(git_init.status.success());

    // Configure dummy user for git
    let _ = Command::new("git")
        .args(["config", "user.name", "Test User"])
        .current_dir(dir)
        .output();
    let _ = Command::new("git")
        .args(["config", "user.email", "test@example.com"])
        .current_dir(dir)
        .output();

    fs::write(dir.join("dummy.txt"), "hello").expect("write dummy");
    let _ = Command::new("git")
        .args(["add", "."])
        .current_dir(dir)
        .output();
    let _ = Command::new("git")
        .args(["commit", "-m", "first commit"])
        .current_dir(dir)
        .output();

    let git_head_before = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(dir)
        .output()
        .expect("git rev-parse");
    let head_before = String::from_utf8_lossy(&git_head_before.stdout)
        .trim()
        .to_string();

    // Run decapod init (it should skip git init and preserve the repo state)
    let out = run_decapod(
        dir,
        &[
            "init",
            "with",
            "--force",
            "--git",
            "--product-name",
            "Existing Project",
            "--product-summary",
            "Summary",
            "--primary-language",
            "Rust",
        ],
        &[],
    );
    assert!(
        out.status.success(),
        "decapod init on existing git repo failed"
    );

    // Check HEAD is still the same (state preserved)
    let git_head_after = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(dir)
        .output()
        .expect("git rev-parse");
    let head_after = String::from_utf8_lossy(&git_head_after.stdout)
        .trim()
        .to_string();

    assert_eq!(head_before, head_after, "Git HEAD changed after init");
}

#[test]
fn test_init_with_no_git_declined() {
    let tmp = TempDir::new().expect("tmpdir");
    let dir = tmp.path();

    // Run decapod init with --no-git (explicitly decline)
    let out = run_decapod(
        dir,
        &[
            "init",
            "with",
            "--force",
            "--no-git",
            "--product-name",
            "No Git Project",
            "--product-summary",
            "Summary",
            "--primary-language",
            "Rust",
        ],
        &[],
    );

    assert!(
        out.status.success(),
        "decapod init with --no-git failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    // Verify .git does NOT exist
    assert!(
        !dir.join(".git").exists(),
        "Expected .git to not be created when declined"
    );

    // Verify stdout/stderr contains warning
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("Git repository was not initialized")
            || String::from_utf8_lossy(&out.stderr).contains("Git repository was not initialized"),
        "Warning message should be printed"
    );
}

#[test]
fn test_workspace_creation_immediately_after_init() {
    let tmp = TempDir::new().expect("tmpdir");
    let dir = tmp.path();

    // Run init with git enabled
    let out = run_decapod(
        dir,
        &[
            "init",
            "with",
            "--force",
            "--git",
            "--product-name",
            "Clean Project",
            "--product-summary",
            "Summary",
            "--primary-language",
            "Rust",
        ],
        &[],
    );
    assert!(out.status.success());

    // Configure dummy user for git
    let _ = Command::new("git")
        .args(["config", "user.name", "Test User"])
        .current_dir(dir)
        .output();
    let _ = Command::new("git")
        .args(["config", "user.email", "test@example.com"])
        .current_dir(dir)
        .output();

    // Make an initial commit
    let _ = Command::new("git")
        .args(["add", "."])
        .current_dir(dir)
        .output();
    let _ = Command::new("git")
        .args(["commit", "-m", "first commit"])
        .current_dir(dir)
        .output();

    // Acquire session
    let acquire = run_decapod(
        dir,
        &["session", "acquire"],
        &[("DECAPOD_AGENT_ID", "git-init-test-agent")],
    );
    assert!(acquire.status.success());

    let stdout = String::from_utf8_lossy(&acquire.stdout);
    let password = stdout
        .lines()
        .find_map(|line| {
            line.strip_prefix("Password: ")
                .map(|s| s.trim().to_string())
        })
        .expect("password in session acquire output");

    // Add a todo
    let todo_add = run_decapod_with_password(
        dir,
        &["todo", "add", "workspace test task", "--priority", "high"],
        &password,
        &[],
    );
    assert!(
        todo_add.status.success(),
        "todo add failed: {}",
        String::from_utf8_lossy(&todo_add.stderr)
    );

    // Get todo ID from stdout
    let todo_stdout = String::from_utf8_lossy(&todo_add.stdout);
    let todo_id = todo_stdout
        .lines()
        .find_map(|line| {
            if line.contains("\"id\":\"") {
                let parts: Vec<&str> = line.split("\"id\":\"").collect();
                if parts.len() > 1 {
                    let end_parts: Vec<&str> = parts[1].split('"').collect();
                    return Some(end_parts[0].to_string());
                }
            }
            None
        })
        .expect("todo ID in output");

    // Claim todo
    let todo_claim =
        run_decapod_with_password(dir, &["todo", "claim", "--id", &todo_id], &password, &[]);
    assert!(todo_claim.status.success(), "todo claim failed");

    // Ensure workspace
    let workspace_ensure = run_decapod_with_password(dir, &["workspace", "ensure"], &password, &[]);
    assert!(
        workspace_ensure.status.success(),
        "workspace ensure failed: {}",
        String::from_utf8_lossy(&workspace_ensure.stderr)
    );
}
