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

    let worktree_dir = tmp.path().join("worktree");
    let worktree = Command::new("git")
        .current_dir(&repo_dir)
        .args([
            "worktree",
            "add",
            "-b",
            "agent/test/commit-often",
            worktree_dir
                .to_str()
                .expect("tempdir path should be valid unicode"),
            "HEAD",
        ])
        .output()
        .expect("git worktree add");
    assert!(worktree.status.success(), "git worktree add failed");

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

#[test]
fn commit_often_gate_fails_when_dirty_files_exceed_threshold() {
    let (_tmp, dir, password) = setup_repo();

    for i in 0..5 {
        let p = dir.join(format!("dirty-{i}.txt"));
        fs::write(p, format!("dirty {i}\n")).expect("write dirty file");
    }

    let validate = run_decapod(
        &dir,
        &["validate"],
        &[
            ("DECAPOD_AGENT_ID", "unknown"),
            ("DECAPOD_SESSION_PASSWORD", &password),
            ("DECAPOD_CONTAINER", "1"),
            ("DECAPOD_COMMIT_OFTEN_MAX_DIRTY_FILES", "3"),
        ],
    );
    assert!(
        !validate.status.success(),
        "validate should fail when dirty files exceed threshold"
    );
    let combined = format!(
        "{}\n{}",
        String::from_utf8_lossy(&validate.stdout),
        String::from_utf8_lossy(&validate.stderr)
    );
    assert!(
        combined.contains("Commit-often mandate violation"),
        "expected commit-often gate failure marker, got: {combined}"
    );
}

#[test]
fn commit_often_gate_passes_when_dirty_files_within_threshold() {
    let (_tmp, dir, password) = setup_repo();

    for i in 0..2 {
        let p = dir.join(format!("dirty-ok-{i}.txt"));
        fs::write(p, format!("dirty-ok {i}\n")).expect("write dirty file");
    }

    let validate = run_decapod(
        &dir,
        &["validate"],
        &[
            ("DECAPOD_AGENT_ID", "unknown"),
            ("DECAPOD_SESSION_PASSWORD", &password),
            ("DECAPOD_CONTAINER", "1"),
            ("DECAPOD_COMMIT_OFTEN_MAX_DIRTY_FILES", "3"),
        ],
    );
    assert!(
        validate.status.success(),
        "validate should pass when dirty files are within threshold; stderr: {}",
        String::from_utf8_lossy(&validate.stderr)
    );
}
