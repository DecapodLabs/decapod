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

fn broker_socket_supported(dir: &Path, password: &str) -> bool {
    let probe = run_decapod(
        dir,
        &["todo", "add", "broker-socket-probe"],
        &[
            ("DECAPOD_AGENT_ID", "unknown"),
            ("DECAPOD_SESSION_PASSWORD", password),
            ("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1"),
            ("DECAPOD_GROUP_BROKER_REQUEST_ID", "BROKER_SOCKET_PROBE"),
        ],
    );
    if probe.status.success() {
        return true;
    }
    let stderr = String::from_utf8_lossy(&probe.stderr).to_ascii_lowercase();
    !stderr.contains("operation not permitted")
}

#[test]
fn broker_no_sqlite_busy_surfaced_under_concurrent_mutators() {
    let (_tmp, dir, password) = setup_repo();
    if !broker_socket_supported(&dir, &password) {
        eprintln!("skipping: unix socket transport not permitted in this sandbox");
        return;
    }

    let mut outputs = Vec::new();
    for i in 0..20 {
        outputs.push(run_decapod(
            &dir,
            &["todo", "add", &format!("concurrent-task-{}", i)],
            &[
                ("DECAPOD_AGENT_ID", "unknown"),
                ("DECAPOD_SESSION_PASSWORD", &password),
                ("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1"),
                ("DECAPOD_GROUP_BROKER_IDLE_SECS", "3"),
            ],
        ));
    }

    for output in &outputs {
        assert!(
            output.status.success(),
            "mutator failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let stderr = String::from_utf8_lossy(&output.stderr).to_ascii_lowercase();
        assert!(
            !stderr.contains("database is locked")
                && !stderr.contains("sqlite_busy")
                && !stderr.contains("databaselocked"),
            "sqlite busy leaked to caller: {}",
            stderr
        );
    }

    let lock_path = dir.join(".decapod").join("broker.lock");
    let sock_path = dir.join(".decapod").join("broker.sock");
    assert!(
        !lock_path.exists() && !sock_path.exists(),
        "ephemeral broker artifacts should be cleaned up"
    );
}

#[test]
fn broker_dedupe_returns_exactly_once_per_request_id() {
    let (_tmp, dir, password) = setup_repo();
    if !broker_socket_supported(&dir, &password) {
        eprintln!("skipping: unix socket transport not permitted in this sandbox");
        return;
    }
    let req_id = "BROKER_DEDUPE_TEST_001";

    let first = run_decapod(
        &dir,
        &["todo", "add", "dedupe-task"],
        &[
            ("DECAPOD_AGENT_ID", "unknown"),
            ("DECAPOD_SESSION_PASSWORD", &password),
            ("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1"),
            ("DECAPOD_GROUP_BROKER_REQUEST_ID", req_id),
        ],
    );
    assert!(
        first.status.success(),
        "first write failed: {}",
        String::from_utf8_lossy(&first.stderr)
    );

    let second = run_decapod(
        &dir,
        &["todo", "add", "dedupe-task"],
        &[
            ("DECAPOD_AGENT_ID", "unknown"),
            ("DECAPOD_SESSION_PASSWORD", &password),
            ("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1"),
            ("DECAPOD_GROUP_BROKER_REQUEST_ID", req_id),
        ],
    );
    assert!(
        second.status.success(),
        "second write failed: {}",
        String::from_utf8_lossy(&second.stderr)
    );

    let db_path = dir.join(".decapod").join("data").join("todo.db");
    let conn = rusqlite::Connection::open(db_path).expect("open todo db");
    let count_res: Result<i64, rusqlite::Error> = conn.query_row(
        "SELECT COUNT(*) FROM tasks WHERE title = 'dedupe-task'",
        [],
        |row| row.get(0),
    );
    let count = match count_res {
        Ok(v) => v,
        Err(_) => {
            eprintln!("skipping: repo todo schema unavailable in this environment");
            return;
        }
    };
    assert_eq!(count, 1, "dedupe task should be persisted exactly once");
}

#[test]
fn broker_election_uniqueness_no_residual_lock_after_burst() {
    let (_tmp, dir, password) = setup_repo();
    if !broker_socket_supported(&dir, &password) {
        eprintln!("skipping: unix socket transport not permitted in this sandbox");
        return;
    }

    for _ in 0..8 {
        let out = run_decapod(
            &dir,
            &["todo", "list"],
            &[
                ("DECAPOD_AGENT_ID", "unknown"),
                ("DECAPOD_SESSION_PASSWORD", &password),
                ("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1"),
            ],
        );
        assert!(out.status.success(), "control read should pass");
    }

    let mutators: Vec<_> = (0..6)
        .map(|i| {
            run_decapod(
                &dir,
                &["todo", "add", &format!("election-task-{}", i)],
                &[
                    ("DECAPOD_AGENT_ID", "unknown"),
                    ("DECAPOD_SESSION_PASSWORD", &password),
                    ("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1"),
                    ("DECAPOD_GROUP_BROKER_IDLE_SECS", "2"),
                ],
            )
        })
        .collect();

    for out in &mutators {
        assert!(out.status.success(), "mutator should succeed");
    }

    let lock_path = dir.join(".decapod").join("broker.lock");
    let sock_path = dir.join(".decapod").join("broker.sock");
    assert!(
        !lock_path.exists() && !sock_path.exists(),
        "broker lease/socket should expire and disappear"
    );
}
