use std::path::{Path, PathBuf};
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

fn setup_repo() -> (TempDir, PathBuf, String) {
    let tmp = TempDir::new().expect("tmpdir");
    let dir = tmp.path().to_path_buf();
    let init = Command::new("git")
        .current_dir(&dir)
        .args(["init", "-b", "master"])
        .output()
        .expect("git init");
    assert!(init.status.success(), "git init failed");

    let out = run_decapod(&dir, &["init", "--force"], &[]);
    assert!(
        out.status.success(),
        "decapod init failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let acquire = run_decapod(
        &dir,
        &["session", "acquire"],
        &[
            ("DECAPOD_AGENT_ID", "unknown"),
            ("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1"),
        ],
    );
    assert!(
        acquire.status.success(),
        "session acquire failed: {}",
        String::from_utf8_lossy(&acquire.stderr)
    );
    let stdout = String::from_utf8_lossy(&acquire.stdout);
    let password = stdout
        .lines()
        .find_map(|line| line.strip_prefix("Password: ").map(|s| s.trim().to_string()))
        .expect("session password");
    (tmp, dir, password)
}

#[test]
fn broker_strict_mode_blocks_mutator_bypass_when_disabled() {
    let (_tmp, dir, password) = setup_repo();
    let out = run_decapod(
        &dir,
        &["todo", "add", "strict-bypass-denied"],
        &[
            ("DECAPOD_AGENT_ID", "unknown"),
            ("DECAPOD_SESSION_PASSWORD", &password),
            ("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1"),
            ("DECAPOD_GROUP_BROKER_DISABLE", "1"),
            ("DECAPOD_GROUP_BROKER_ENFORCE_ROUTE", "1"),
        ],
    );
    assert!(!out.status.success(), "strict mode should deny bypass");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("BROKER_ROUTE_REQUIRED"),
        "expected typed strict-route error, got: {stderr}"
    );
}
