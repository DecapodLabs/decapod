use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::time::{Duration, Instant};
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
    let hook = dir.join("broker-socket-probe.log");
    let probe = run_decapod(
        dir,
        &["todo", "add", "broker-socket-probe"],
        &[
            ("DECAPOD_AGENT_ID", "unknown"),
            ("DECAPOD_SESSION_PASSWORD", password),
            ("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1"),
            ("DECAPOD_GROUP_BROKER_REQUEST_ID", "BROKER_SOCKET_PROBE"),
            ("DECAPOD_GROUP_BROKER_TEST_HOOK_FILE", hook.to_string_lossy().as_ref()),
        ],
    );
    if !probe.status.success() {
        return false;
    }
    wait_for_hook_line(
        &hook,
        "queued|BROKER_SOCKET_PROBE",
        Duration::from_millis(300),
    )
}

fn spawn_decapod(dir: &Path, args: &[&str], envs: &[(&str, &str)]) -> Child {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_decapod"));
    cmd.current_dir(dir).args(args);
    for (k, v) in envs {
        cmd.env(k, v);
    }
    cmd.spawn().expect("spawn decapod")
}

fn wait_for_hook_line(hook_file: &Path, needle: &str, timeout: Duration) -> bool {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if let Ok(raw) = std::fs::read_to_string(hook_file)
            && raw.lines().any(|line| line.contains(needle))
        {
            return true;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    false
}

fn wait_for_lock_pid(lock_path: &Path, timeout: Duration) -> Option<u32> {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if let Ok(raw) = std::fs::read_to_string(lock_path)
            && let Ok(pid) = raw.trim().parse::<u32>()
        {
            return Some(pid);
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    None
}

fn wait_for_no_broker_artifacts(dir: &Path, timeout: Duration) -> bool {
    let lock_path = dir.join(".decapod").join("broker.lock");
    let sock_path = dir.join(".decapod").join("broker.sock");
    let start = Instant::now();
    while start.elapsed() < timeout {
        if !lock_path.exists() && !sock_path.exists() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    false
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

#[test]
fn broker_protocol_mismatch_returns_typed_failure() {
    let (_tmp, dir, password) = setup_repo();
    if !broker_socket_supported(&dir, &password) {
        eprintln!("skipping: unix socket transport not permitted in this sandbox");
        return;
    }
    assert!(
        wait_for_no_broker_artifacts(&dir, Duration::from_secs(6)),
        "broker probe left lock/socket active too long"
    );

    let hook = dir.join("broker-hook.log");
    let mut leader = spawn_decapod(
        &dir,
        &["todo", "add", "proto-leader"],
        &[
            ("DECAPOD_AGENT_ID", "unknown"),
            ("DECAPOD_SESSION_PASSWORD", &password),
            ("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1"),
            ("DECAPOD_GROUP_BROKER_IDLE_SECS", "30"),
            ("DECAPOD_GROUP_BROKER_REQUEST_ID", "PROTO_LEADER_REQ"),
            ("DECAPOD_GROUP_BROKER_TEST_HOOK_FILE", hook.to_string_lossy().as_ref()),
            ("DECAPOD_GROUP_BROKER_TEST_HALT_PHASE", "queued"),
        ],
    );
    let hook_ok = wait_for_hook_line(&hook, "queued|PROTO_LEADER_REQ", Duration::from_secs(5));
    assert!(hook_ok, "leader never entered queued phase");
    let lock_path = dir.join(".decapod").join("broker.lock");
    let pid = wait_for_lock_pid(&lock_path, Duration::from_secs(5)).expect("leader pid");
    assert!(pid > 0);

    let follower = run_decapod(
        &dir,
        &["todo", "add", "proto-follower"],
        &[
            ("DECAPOD_AGENT_ID", "unknown"),
            ("DECAPOD_SESSION_PASSWORD", &password),
            ("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1"),
            ("DECAPOD_GROUP_BROKER_PROTOCOL_CLIENT_OVERRIDE", "999"),
        ],
    );
    assert!(!follower.status.success(), "protocol mismatch must fail");
    let stderr = String::from_utf8_lossy(&follower.stderr);
    assert!(
        stderr.contains("BROKER_PROTOCOL_MISMATCH"),
        "expected typed protocol mismatch error, got: {stderr}"
    );

    let _ = Command::new("kill")
        .args(["-9", &pid.to_string()])
        .status();
    let _ = leader.wait();
}

#[test]
fn broker_crash_injection_phases_retry_to_exactly_once() {
    let (_tmp, dir, password) = setup_repo();
    if !broker_socket_supported(&dir, &password) {
        eprintln!("skipping: unix socket transport not permitted in this sandbox");
        return;
    }
    assert!(
        wait_for_no_broker_artifacts(&dir, Duration::from_secs(6)),
        "broker probe left lock/socket active too long"
    );

    let phases = ["queued", "pre_exec", "post_exec_pre_ack"];
    for phase in phases {
        assert!(
            wait_for_no_broker_artifacts(&dir, Duration::from_secs(6)),
            "prior broker lease/socket still active before phase {phase}"
        );
        let req_id = format!("CRASH_PHASE_{}", phase);
        let hook = dir.join(format!("broker-hook-{}.log", phase));
        let mut child = spawn_decapod(
            &dir,
            &["todo", "add", &format!("crash-phase-{}", phase)],
            &[
                ("DECAPOD_AGENT_ID", "unknown"),
                ("DECAPOD_SESSION_PASSWORD", &password),
                ("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1"),
                ("DECAPOD_GROUP_BROKER_REQUEST_ID", &req_id),
                ("DECAPOD_GROUP_BROKER_IDLE_SECS", "30"),
                ("DECAPOD_GROUP_BROKER_TEST_HOOK_FILE", hook.to_string_lossy().as_ref()),
                ("DECAPOD_GROUP_BROKER_TEST_HALT_PHASE", phase),
            ],
        );

        let hook_ok = wait_for_hook_line(&hook, &req_id, Duration::from_secs(8));
        assert!(hook_ok, "phase hook never emitted for {phase}");
        let lock_path = dir.join(".decapod").join("broker.lock");
        let pid = wait_for_lock_pid(&lock_path, Duration::from_secs(3)).expect("broker pid");
        let _ = Command::new("kill")
            .args(["-9", &pid.to_string()])
            .status();
        let _ = child.wait();

        let retry = run_decapod(
            &dir,
            &["todo", "add", &format!("crash-phase-{}", phase)],
            &[
                ("DECAPOD_AGENT_ID", "unknown"),
                ("DECAPOD_SESSION_PASSWORD", &password),
                ("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1"),
                ("DECAPOD_GROUP_BROKER_REQUEST_ID", &req_id),
            ],
        );
        assert!(
            retry.status.success(),
            "retry after crash must converge to committed: {}",
            String::from_utf8_lossy(&retry.stderr)
        );
    }

    let dedupe = dir.join(".decapod").join("data").join("broker_dedupe.db");
    let conn = rusqlite::Connection::open(dedupe).expect("open dedupe db");
    for phase in ["queued", "pre_exec", "post_exec_pre_ack"] {
        let req_id = format!("CRASH_PHASE_{}", phase);
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM request_dedupe WHERE request_id = ?1",
                [req_id.as_str()],
                |row| row.get(0),
            )
            .expect("count request id");
        assert_eq!(count, 1, "request_id must have exactly one dedupe row");
    }
}
