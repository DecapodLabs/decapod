use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::TempDir;

fn run_decapod(dir: &Path, args: &[&str], envs: &[(&str, &str)]) -> std::process::Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_decapod"));
    cmd.current_dir(dir).args(args);
    cmd.env_remove("DECAPOD_VALIDATE_SKIP_GIT_GATES");
    cmd.env_remove("DECAPOD_VALIDATE_SKIP_TOOLING_GATES");
    for (k, v) in envs {
        cmd.env(k, v);
    }
    cmd.output().expect("run decapod")
}

fn setup_repo() -> (TempDir, PathBuf, String) {
    setup_repo_with_branch("agent/test/git-push-pr")
}

fn setup_repo_with_branch(branch: &str) -> (TempDir, PathBuf, String) {
    let tmp = TempDir::new().expect("tmpdir");
    let repo_dir = tmp.path().to_path_buf();

    let init = Command::new("git")
        .current_dir(&repo_dir)
        .args(["init", "-b", "master"])
        .output()
        .expect("git init");
    assert!(init.status.success(), "git init failed");

    let out = run_decapod(&repo_dir, &["init", "--force"], &[]);
    assert!(
        out.status.success(),
        "decapod init failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let config_name = Command::new("git")
        .current_dir(&repo_dir)
        .args(["config", "user.name", "Test User"])
        .output()
        .expect("git config user.name");
    assert!(config_name.status.success(), "git config user.name failed");

    let config_email = Command::new("git")
        .current_dir(&repo_dir)
        .args(["config", "user.email", "test@example.com"])
        .output()
        .expect("git config user.email");
    assert!(
        config_email.status.success(),
        "git config user.email failed"
    );

    let add = Command::new("git")
        .current_dir(&repo_dir)
        .args(["add", "."])
        .output()
        .expect("git add");
    assert!(add.status.success(), "git add failed");

    let commit = Command::new("git")
        .current_dir(&repo_dir)
        .args(["commit", "-m", "init"])
        .output()
        .expect("git commit");
    assert!(commit.status.success(), "git commit failed");

    let worktree_dir = repo_dir
        .join(".decapod")
        .join("workspaces")
        .join("test-worktree");
    fs::create_dir_all(worktree_dir.parent().unwrap()).unwrap();

    let worktree = Command::new("git")
        .current_dir(&repo_dir)
        .args([
            "worktree",
            "add",
            "-b",
            branch,
            worktree_dir
                .to_str()
                .expect("tempdir path should be valid unicode"),
            "HEAD",
        ])
        .output()
        .expect("git worktree add");
    assert!(worktree.status.success(), "git worktree add failed");

    let branch_check = Command::new("git")
        .current_dir(&worktree_dir)
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .output()
        .expect("resolve worktree branch");
    assert!(
        branch_check.status.success(),
        "resolve worktree branch failed"
    );
    assert_eq!(
        String::from_utf8_lossy(&branch_check.stdout).trim(),
        branch,
        "worktree setup must leave validation on the requested working branch"
    );

    let acquire = run_decapod(
        &worktree_dir,
        &["session", "acquire"],
        &[("DECAPOD_AGENT_ID", "unknown")],
    );
    assert!(
        acquire.status.success(),
        "session acquire failed: {}",
        String::from_utf8_lossy(&acquire.stderr)
    );
    let password = String::from_utf8_lossy(&acquire.stdout)
        .lines()
        .find_map(|line| {
            line.strip_prefix("Password: ")
                .map(|s| s.trim().to_string())
        })
        .expect("session password in output");

    (tmp, worktree_dir, password)
}

fn prepend_fake_gh_to_path(tmp: &TempDir, body: &str) -> String {
    let bin_dir = tmp.path().join("bin");
    fs::create_dir_all(&bin_dir).expect("create fake bin dir");
    let gh_path = bin_dir.join("gh");
    fs::write(
        &gh_path,
        format!(
            "#!/usr/bin/env sh\nprintf '%s\\n' '{}'\n",
            body.replace('\'', "'\\''")
        ),
    )
    .expect("write fake gh");
    let chmod = Command::new("chmod")
        .args(["+x", gh_path.to_str().unwrap()])
        .output()
        .expect("chmod fake gh");
    assert!(chmod.status.success(), "chmod fake gh failed");
    format!(
        "{}:{}",
        bin_dir.display(),
        std::env::var("PATH").unwrap_or_default()
    )
}

#[test]
fn git_push_pr_gate_warns_when_unpushed() {
    let (_tmp, dir, password) = setup_repo();

    let validate = run_decapod(
        &dir,
        &["validate", "-v"],
        &[
            ("DECAPOD_AGENT_ID", "unknown"),
            ("DECAPOD_SESSION_PASSWORD", &password),
            ("DECAPOD_CONTAINER", "1"),
        ],
    );

    let combined = format!(
        "{}\n{}",
        String::from_utf8_lossy(&validate.stdout),
        String::from_utf8_lossy(&validate.stderr)
    );
    assert!(
        combined.contains("has unpushed commits or does not exist on origin"),
        "expected git-push-pr gate warning marker, got: {combined}"
    );
}

#[test]
fn git_push_pr_gate_fails_when_open_pr_lacks_workunit_trajectory() {
    let (tmp, dir, password) = setup_repo_with_branch("agent/test/test_123456");
    let repo_dir = dir
        .parent()
        .and_then(|p| p.parent())
        .and_then(|p| p.parent())
        .expect("resolve repo root");
    let bare = tmp.path().join("origin.git");
    let init_bare = Command::new("git")
        .args(["init", "--bare", bare.to_str().unwrap()])
        .output()
        .expect("git init bare");
    assert!(init_bare.status.success(), "git init bare failed");
    let add_remote = Command::new("git")
        .current_dir(repo_dir)
        .args(["remote", "add", "origin", bare.to_str().unwrap()])
        .output()
        .expect("git remote add");
    assert!(add_remote.status.success(), "git remote add failed");
    let push = Command::new("git")
        .current_dir(&dir)
        .args(["push", "-u", "origin", "agent/test/test_123456"])
        .output()
        .expect("git push");
    assert!(
        push.status.success(),
        "git push failed: {}",
        String::from_utf8_lossy(&push.stderr)
    );

    let fake_path = prepend_fake_gh_to_path(&tmp, r#"[{"number":1}]"#);
    let validate = run_decapod(
        &dir,
        &["validate", "-v"],
        &[
            ("DECAPOD_AGENT_ID", "unknown"),
            ("DECAPOD_SESSION_PASSWORD", &password),
            ("DECAPOD_CONTAINER", "1"),
            ("PATH", &fake_path),
        ],
    );

    let combined = format!(
        "{}\n{}",
        String::from_utf8_lossy(&validate.stdout),
        String::from_utf8_lossy(&validate.stderr)
    );
    assert!(
        combined.contains("PR_TRAJECTORY_MISSING"),
        "expected PR trajectory failure marker, got: {combined}"
    );
}
