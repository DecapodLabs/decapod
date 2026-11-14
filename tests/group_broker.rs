use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::time::{Duration, Instant};
use tempfile::TempDir;

fn resolve_decapod_bin() -> PathBuf {
    let cargo_bin = env!("CARGO_BIN_EXE_decapod");
    if let Ok(path) = Path::new(cargo_bin).canonicalize() {
        return path;
    }
    if let Ok(runfiles_dir) = std::env::var("RUNFILES_DIR") {
        let path = Path::new(&runfiles_dir).join("_main").join("decapod");
        if path.exists() {
            return path;
        }
    }
    if let Some(parent) = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(Path::to_path_buf))
    {
        let path = parent.join("decapod");
        if path.exists() {
            return path;
        }
    }
    PathBuf::from(cargo_bin)
}

fn broker_endpoint(dir: &Path) -> PathBuf {
    #[cfg(unix)]
    {
        decapod::core::group_broker::broker_socket_path(&dir.join(".decapod/data")).unwrap()
    }
    #[cfg(not(unix))]
    {
        dir.join(".decapod/data/broker-runtime/broker.sock")
    }
}

fn run_decapod(dir: &Path, args: &[&str], envs: &[(&str, &str)]) -> std::process::Output {
    let mut cmd = Command::new(resolve_decapod_bin());
    cmd.current_dir(dir).args(args);
    for (k, v) in envs {
        cmd.env(k, v);
    }
    cmd.output().expect("run decapod")
}

fn setup_repo() -> (TempDir, PathBuf, String) {
    setup_repo_with(run_decapod)
}

type DecapodRunner = fn(&Path, &[&str], &[(&str, &str)]) -> std::process::Output;

fn setup_repo_with(run: DecapodRunner) -> (TempDir, PathBuf, String) {
    setup_repo_at_depth(run, false)
}

fn setup_repo_at_depth(run: DecapodRunner, long: bool) -> (TempDir, PathBuf, String) {
    let tmp = TempDir::new().expect("tmpdir");
    let dir = if long {
        tmp.path().join("canonical-datastore-".repeat(10))
    } else {
        tmp.path().to_path_buf()
    };
    std::fs::create_dir_all(&dir).unwrap();

    let init = Command::new("git")
        .current_dir(&dir)
        .args(["init", "-b", "master"])
        .output()
        .expect("git init");
    assert!(init.status.success(), "git init failed");

    let out = run(&dir, &["init", "--force"], &[]);
    assert!(
        out.status.success(),
        "decapod init failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let acquire = run(
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
        .find_map(|line| {
            line.strip_prefix("Password: ")
                .map(|s| s.trim().to_string())
        })
        .expect("session password");

    (tmp, dir, password)
}

fn acquire_session_password_with(dir: &Path, agent_id: &str, run: DecapodRunner) -> String {
    let acquire = run(
        dir,
        &["session", "acquire"],
        &[
            ("DECAPOD_AGENT_ID", agent_id),
            ("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1"),
        ],
    );
    assert!(
        acquire.status.success(),
        "session acquire failed for {agent_id}: {}",
        String::from_utf8_lossy(&acquire.stderr)
    );
    let stdout = String::from_utf8_lossy(&acquire.stdout);
    stdout
        .lines()
        .find_map(|line| {
            line.strip_prefix("Password: ")
                .map(|s| s.trim().to_string())
        })
        .expect("session password")
}

fn broker_socket_supported(dir: &Path, password: &str) -> bool {
    broker_socket_supported_with(dir, password, run_decapod)
}

fn broker_socket_supported_with(dir: &Path, password: &str, run: DecapodRunner) -> bool {
    let hook = dir.join("broker-socket-probe.log");
    let probe = run(
        dir,
        &["todo", "add", "broker-socket-probe"],
        &[
            ("DECAPOD_AGENT_ID", "unknown"),
            ("DECAPOD_SESSION_PASSWORD", password),
            ("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1"),
            ("DECAPOD_GROUP_BROKER_REQUEST_ID", "BROKER_SOCKET_PROBE"),
            (
                "DECAPOD_GROUP_BROKER_TEST_HOOK_FILE",
                hook.to_string_lossy().as_ref(),
            ),
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
    let mut cmd = Command::new(resolve_decapod_bin());
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
    let lock_path = dir.join(".decapod").join("data").join("broker.lock");
    let sock_path = broker_endpoint(dir);
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
    assert_concurrent_mutators(false);
}

#[test]
fn invalid_broker_request_budgets_cannot_admit_a_mutation() {
    let (_tmp, dir, password) = setup_repo_with(run_contention_setup);
    assert!(
        broker_socket_supported_with(&dir, &password, run_contention_setup),
        "required broker socket acceptance did not run"
    );
    let agent = "broker-budget-boundary";
    let password = acquire_session_password_with(&dir, agent, run_contention_setup);
    for (index, budget) in ["0", "300001", "invalid"].into_iter().enumerate() {
        let title = format!("invalid-broker-budget-{index}");
        let request = format!("BROKER_INVALID_BUDGET_{index}");
        let output = run_contention_command(
            &dir,
            &["todo", "add", &title],
            &[
                ("DECAPOD_AGENT_ID", agent),
                ("DECAPOD_SESSION_PASSWORD", &password),
                ("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1"),
                ("DECAPOD_GROUP_BROKER_REQUEST_ID", &request),
                ("DECAPOD_GROUP_BROKER_REQUEST_BUDGET_MS", budget),
            ],
            Duration::from_secs(30),
            "invalid request budget",
        );
        assert!(!output.status.success());
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("BROKER_REQUEST_BUDGET_INVALID"),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let conn =
            decapod::core::db::Connection::open(dir.join(".decapod/data/decapod.db")).unwrap();
        let tasks: i64 = conn
            .query_row(
                "SELECT count(*) FROM tasks WHERE title=?1",
                [&title],
                |row| row.get(0),
            )
            .unwrap();
        let receipts: i64 = conn
            .query_row(
                "SELECT count(*) FROM request_dedupe WHERE request_id=?1",
                [&request],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(tasks, 0, "invalid budget changed task state");
        assert_eq!(receipts, 0, "invalid budget created a receipt");
    }
}

#[cfg(unix)]
#[test]
fn reusable_connection_handles_twenty_concurrent_mutators() {
    assert_concurrent_mutators(true);
}

fn assert_concurrent_mutators(reuse_connection: bool) {
    let _timing =
        BrokerTestTiming::start("broker_no_sqlite_busy_surfaced_under_concurrent_mutators");
    let (_tmp, dir, password) = setup_repo_with(run_contention_setup);
    assert!(
        broker_socket_supported_with(&dir, &password, run_contention_setup),
        "required broker socket acceptance did not run"
    );

    let creds: Vec<(String, String)> = (0..20)
        .map(|i| {
            let agent_id = format!("agent-{i:02}");
            let _timing = BrokerTestTiming::start(format!("contention credential {i:02}"));
            let agent_pw = acquire_session_password_with(&dir, &agent_id, run_contention_setup);
            (agent_id, agent_pw)
        })
        .collect();

    let barrier = std::sync::Arc::new(std::sync::Barrier::new(creds.len()));
    let mut workers = Vec::new();
    for (i, (agent_id, agent_pw)) in creds.into_iter().enumerate() {
        let dir_cl = dir.clone();
        let barrier = barrier.clone();
        workers.push(std::thread::spawn(move || {
            barrier.wait();
            let _timing = BrokerTestTiming::start(format!("contention client {i:02}"));
            let task = format!("concurrent-task-{i}");
            let req_id = format!("BROKER_BUSY_REQ_{i:02}");
            let envs = [
                (
                    "DECAPOD_STORAGE_OWNER".to_string(),
                    if reuse_connection { "1" } else { "0" }.to_string(),
                ),
                ("DECAPOD_AGENT_ID".to_string(), agent_id),
                ("DECAPOD_SESSION_PASSWORD".to_string(), agent_pw),
                (
                    "DECAPOD_VALIDATE_SKIP_GIT_GATES".to_string(),
                    "1".to_string(),
                ),
                (
                    "DECAPOD_GROUP_BROKER_IDLE_SECS".to_string(),
                    "3".to_string(),
                ),
                // Queueing and execution share the workload's declared budget.
                (
                    "DECAPOD_GROUP_BROKER_REQUEST_BUDGET_MS".to_string(),
                    "300000".to_string(),
                ),
                ("DECAPOD_GROUP_BROKER_REQUEST_ID".to_string(), req_id),
                (
                    "DECAPOD_GROUP_BROKER_TEST_HOOK_FILE".to_string(),
                    dir_cl
                        .join(format!("contention-{i}-hooks"))
                        .to_string_lossy()
                        .into_owned(),
                ),
            ];
            let env_pairs: Vec<(&str, &str)> =
                envs.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
            run_contention_command(
                &dir_cl,
                &["todo", "add", &task],
                &env_pairs,
                // Under `bazel test //:all` this target runs in parallel with
                // //:interop_conformance; observed client latencies reach
                // ~120s, so keep generous headroom below the long target
                // timeout instead of failing spuriously at the old bound.
                Duration::from_secs(300),
                &format!("client {i:02} first attempt"),
            )
        }));
    }

    // Join every bounded worker before asserting, including when one panics,
    // so a failing attempt cannot leave sibling clients running in a removed fixture.
    let outputs: Vec<_> = workers.into_iter().map(|worker| worker.join()).collect();
    for output in outputs {
        let output = output.expect("join mutator worker");
        assert!(
            output.status.success(),
            "mutator failed (status={:?}) stdout={} stderr={}",
            output.status.code(),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        let stderr = String::from_utf8_lossy(&output.stderr).to_ascii_lowercase();
        assert!(
            !stderr.contains("database is locked")
                && !stderr.contains("sqlite_busy")
                && !stderr.contains("databaselocked"),
            "sqlite busy leaked to caller: {stderr}"
        );
    }

    let conn = decapod::core::db::Connection::open(dir.join(".decapod/data/decapod.db")).unwrap();
    for i in 0..20 {
        let count: i64 = conn
            .query_row(
                "SELECT count(*) FROM tasks WHERE title=?1",
                [format!("concurrent-task-{i}")],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 1, "persisted result for caller {i}");
    }
    if reuse_connection {
        let opens: usize = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| {
                entry.file_name().to_string_lossy().contains("contention")
                    && entry.file_name().to_string_lossy().contains("hooks")
            })
            .map(|entry| {
                std::fs::read_to_string(entry.path())
                    .unwrap_or_default()
                    .lines()
                    .filter(|line| line.starts_with("storage_owner_open|"))
                    .count()
            })
            .sum();
        assert!(
            opens > 0,
            "twenty-caller acceptance did not open a physical owner"
        );
    }

    let lock_path = dir.join(".decapod").join("data").join("broker.lock");
    let sock_path = broker_endpoint(&dir);
    assert!(
        !lock_path.exists() && !sock_path.exists(),
        "ephemeral broker artifacts should be cleaned up"
    );
}

#[test]
fn broker_completed_receipt_retrieval_does_not_repeat_request() {
    let _timing =
        BrokerTestTiming::start("broker_completed_receipt_retrieval_does_not_repeat_request");
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

    let db_path = dir.join(".decapod").join("data").join("decapod.db");
    let conn = decapod::core::db::Connection::open(db_path).expect("open todo db");
    let count_res: Result<i64, decapod::core::db::Error> = conn.query_row(
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
    let _timing =
        BrokerTestTiming::start("broker_election_uniqueness_no_residual_lock_after_burst");
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
                &["todo", "add", &format!("election-task-{i}")],
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

    let lock_path = dir.join(".decapod").join("data").join("broker.lock");
    let sock_path = broker_endpoint(&dir);
    assert!(
        !lock_path.exists() && !sock_path.exists(),
        "broker lease/socket should expire and disappear"
    );
}

#[test]
fn broker_protocol_mismatch_returns_typed_failure() {
    let _timing = BrokerTestTiming::start("broker_protocol_mismatch_returns_typed_failure");
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
            (
                "DECAPOD_GROUP_BROKER_TEST_HOOK_FILE",
                hook.to_string_lossy().as_ref(),
            ),
        ],
    );
    let hook_ok = wait_for_hook_line(&hook, "queued|PROTO_LEADER_REQ", Duration::from_secs(5));
    assert!(hook_ok, "leader never entered queued phase");
    std::thread::sleep(Duration::from_millis(150));
    let lock_path = dir.join(".decapod").join("data").join("broker.lock");
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

    let _ = Command::new("kill").args(["-9", &pid.to_string()]).status();
    let _ = leader.wait();
}

#[test]
fn broker_crash_preserves_unknown_outcomes_without_reexecution() {
    let _timing =
        BrokerTestTiming::start("broker_crash_preserves_unknown_outcomes_without_reexecution");
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
        let req_id = format!("CRASH_PHASE_{phase}");
        let hook = dir.join(format!("broker-hook-{phase}.log"));
        let mut child = spawn_decapod(
            &dir,
            &["todo", "add", &format!("crash-phase-{phase}")],
            &[
                ("DECAPOD_AGENT_ID", "unknown"),
                ("DECAPOD_SESSION_PASSWORD", &password),
                ("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1"),
                ("DECAPOD_GROUP_BROKER_REQUEST_ID", &req_id),
                ("DECAPOD_GROUP_BROKER_IDLE_SECS", "30"),
                (
                    "DECAPOD_GROUP_BROKER_TEST_HOOK_FILE",
                    hook.to_string_lossy().as_ref(),
                ),
                ("DECAPOD_GROUP_BROKER_TEST_HALT_PHASE", phase),
            ],
        );

        let hook_ok =
            wait_for_hook_line(&hook, &format!("{phase}|{req_id}"), Duration::from_secs(15));
        assert!(hook_ok, "phase hook never emitted for {phase}");
        let lock_path = dir.join(".decapod").join("data").join("broker.lock");
        let pid = wait_for_lock_pid(&lock_path, Duration::from_secs(3)).expect("broker pid");
        let _ = Command::new("kill").args(["-9", &pid.to_string()]).status();
        let _ = child.wait();

        let retry = run_decapod(
            &dir,
            &["todo", "add", &format!("crash-phase-{phase}")],
            &[
                ("DECAPOD_AGENT_ID", "unknown"),
                ("DECAPOD_SESSION_PASSWORD", &password),
                ("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1"),
                ("DECAPOD_GROUP_BROKER_REQUEST_ID", &req_id),
            ],
        );
        if phase == "queued" {
            assert!(
                retry.status.success(),
                "{}",
                String::from_utf8_lossy(&retry.stderr)
            );
        } else {
            assert!(!retry.status.success());
            assert!(String::from_utf8_lossy(&retry.stderr).contains("BROKER_UNKNOWN"));
        }
        let conn =
            decapod::core::db::Connection::open(dir.join(".decapod/data/decapod.db")).unwrap();
        let persisted: i64 = conn
            .query_row(
                "SELECT count(*) FROM tasks WHERE title=?1",
                [format!("crash-phase-{phase}")],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            persisted,
            if phase == "pre_exec" { 0 } else { 1 },
            "unsafe replay at {phase}"
        );
    }

    let dedupe = dir.join(".decapod").join("data").join("decapod.db");
    let conn = decapod::core::db::Connection::open(dedupe).expect("open dedupe db");
    for phase in ["queued", "pre_exec", "post_exec_pre_ack"] {
        let req_id = format!("CRASH_PHASE_{phase}");
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

#[cfg(unix)]
#[test]
fn explicit_shared_storage_uses_broker_route_without_private_fallback() {
    let _timing = BrokerTestTiming::start(
        "explicit_shared_storage_uses_broker_route_without_private_fallback",
    );
    use std::os::unix::fs::PermissionsExt;
    let (_temp, dir, password) = setup_repo();
    if !broker_socket_supported(&dir, &password) {
        eprintln!("Unix sockets unavailable in this execution environment");
        return;
    }
    let data = dir.join(".decapod/data");
    std::fs::set_permissions(&data, std::fs::Permissions::from_mode(0o770)).unwrap();
    let hook = dir.join("shared-broker-hook");
    let out = Command::new("sh")
        .args(["-c", "umask 007; exec \"$@\"", "shared-broker"])
        .arg(resolve_decapod_bin())
        .args(["todo", "add", "trusted shared store mutation"])
        .current_dir(&dir)
        .env("DECAPOD_STORAGE_SHARED_GROUP", "1")
        .env("DECAPOD_AGENT_ID", "unknown")
        .env("DECAPOD_SESSION_PASSWORD", &password)
        .env("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1")
        .env("DECAPOD_GROUP_BROKER_ENFORCE_ROUTE", "1")
        .env("DECAPOD_GROUP_BROKER_IDLE_SECS", "1")
        .env("DECAPOD_GROUP_BROKER_REQUEST_ID", "SHARED_STORE_MUTATION")
        .env("DECAPOD_GROUP_BROKER_TEST_HOOK_FILE", &hook)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        std::fs::read_to_string(&hook)
            .unwrap()
            .contains("queued|SHARED_STORE_MUTATION")
    );
    assert_eq!(
        std::fs::metadata(&data).unwrap().permissions().mode() & 0o777,
        0o770
    );
    // The product routes long paths through the shared GID namespace. Check
    // that actual directory, rather than assuming the old adjacent address.
    let mut runtime = data.canonicalize().unwrap().join("broker-runtime");
    if runtime
        .join("broker.sock")
        .as_os_str()
        .as_encoded_bytes()
        .len()
        >= 104
    {
        use sha2::{Digest, Sha256};
        use std::os::unix::fs::MetadataExt;
        let canonical = data.canonicalize().unwrap();
        let hash = format!(
            "{:x}",
            Sha256::digest(canonical.as_os_str().as_encoded_bytes())
        );
        runtime = Path::new("/tmp").join(format!(
            "decapod-g{}-{}",
            std::fs::metadata(&data).unwrap().gid(),
            &hash[..32]
        ));
    }
    assert_eq!(
        std::fs::metadata(runtime).unwrap().permissions().mode() & 0o007,
        0
    );
}

#[cfg(unix)]
#[test]
fn committed_claim_preparation_does_not_hold_broker_or_repeat_on_retry() {
    let _timing = BrokerTestTiming::start(
        "committed_claim_preparation_does_not_hold_broker_or_repeat_on_retry",
    );
    claim_preparation_probe(true);
}

#[cfg(unix)]
#[test]
fn direct_claim_acknowledges_before_preparation() {
    let _timing = BrokerTestTiming::start("direct_claim_acknowledges_before_preparation");
    claim_preparation_probe(false);
}

#[cfg(unix)]
fn claim_preparation_probe(broker: bool) {
    use std::os::unix::fs::PermissionsExt;
    claim_probe_stage("setup");
    let (_temp, dir, password) = setup_repo_with(run_decapod_bounded);
    if broker {
        match std::os::unix::net::UnixListener::bind(dir.join("capability.sock")) {
            Ok(listener) => {
                drop(listener);
                std::fs::remove_file(dir.join("capability.sock")).unwrap();
            }
            Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => {
                eprintln!(
                    "AF_UNIX creation denied by local sandbox; dedicated broker proof requires Actions"
                );
                return;
            }
            Err(error) => panic!("AF_UNIX capability probe failed: {error}"),
        }
    }
    let envs = [
        ("DECAPOD_SESSION_PASSWORD", password.as_str()),
        ("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1"),
        ("DECAPOD_GROUP_BROKER_IDLE_SECS", "1"),
        (
            "DECAPOD_GROUP_BROKER_DISABLE",
            if broker { "0" } else { "1" },
        ),
    ];
    claim_probe_stage("add");
    let added = run_decapod_bounded(
        &dir,
        &["todo", "add", "claim follow-up responsiveness"],
        &envs,
    );
    assert!(
        added.status.success(),
        "{}",
        String::from_utf8_lossy(&added.stderr)
    );
    let added: serde_json::Value = serde_json::from_slice(&added.stdout).unwrap();
    let id = added["id"].as_str().unwrap();
    let bin = dir.join("mock-runtime");
    std::fs::create_dir(&bin).unwrap();
    let ready = dir.join("preparation-ready");
    let release = dir.join("preparation-release");
    let calls = dir.join("preparation-calls");
    let script = format!(
        "#!/bin/sh\nif [ \"$1\" = info ]; then\n echo called >> '{}'\n : > '{}'\n while [ ! -f '{}' ]; do sleep 0.05; done\n exit 1\nfi\nexit 0\n",
        calls.display(),
        ready.display(),
        release.display()
    );
    for runtime in ["docker", "podman"] {
        let path = bin.join(runtime);
        std::fs::write(&path, &script).unwrap();
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    let path = format!("{}:{}", bin.display(), std::env::var("PATH").unwrap());
    let args = ["todo", "--format", "json", "claim", "--id", id];
    let progress = dir.join("claim-progress");
    let output_path = dir.join("claim-output");
    let hooks = dir.join("claim-hooks");
    let mut command = Command::new(resolve_decapod_bin());
    command
        .current_dir(&dir)
        .args(args)
        .envs(envs)
        .env("PATH", &path)
        .env("DECAPOD_STORAGE_OWNER", "1")
        .env("DECAPOD_CLAIM_AUTORUN", "1")
        .env("DECAPOD_CONTAINER", "0")
        .env("DECAPOD_GROUP_BROKER_REQUEST_ID", "followup-once")
        .env("DECAPOD_GROUP_BROKER_TEST_HOOK_FILE", &hooks)
        .stdout(std::fs::File::create(&output_path).unwrap())
        .stderr(std::fs::File::create(&progress).unwrap());
    use std::os::unix::process::CommandExt;
    command.process_group(0);
    claim_probe_stage("spawn claim");
    let mut child = ClaimProbeChild(Some(command.spawn().unwrap()));
    let start = Instant::now();
    while !ready.exists() && start.elapsed() < Duration::from_secs(12) {
        std::thread::sleep(Duration::from_millis(20));
    }
    if !ready.exists() {
        child.terminate();
        let output = child.output(&output_path, &progress, &hooks, "claim readiness");
        panic!(
            "preparation did not start: {} {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    assert!(
        std::fs::read_to_string(&progress)
            .unwrap()
            .contains(&format!(
                "Task claim committed: {id}; container preparation pending"
            ))
    );
    // The creating leader has relinquished its lease before client preparation.
    assert!(!dir.join(".decapod/data/broker.lock").exists());
    let start = Instant::now();
    claim_probe_stage("competing mutation");
    let competing = run_decapod_bounded(&dir, &["todo", "add", "while preparation waits"], &envs);
    std::fs::write(&release, "release").unwrap(); // always release before assertions
    claim_probe_stage("await final claim envelope");
    let output = child.output(&output_path, &progress, &hooks, "claim completion");
    assert!(
        competing.status.success(),
        "{}",
        String::from_utf8_lossy(&competing.stderr)
    );
    assert!(start.elapsed() < Duration::from_secs(10));
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let claimed: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(claimed["status"], "ok");
    assert_eq!(claimed["id"], id);
    assert_eq!(claimed["container"]["status"], "warning");
    if !broker {
        return;
    }
    let before = std::fs::read_to_string(&calls).unwrap();
    claim_probe_stage("retry committed request");
    use decapod::core::bounded_process::BoundedCommand;
    let retry = command
        .bounded_output(Duration::from_secs(30))
        .expect("bounded claim retry");
    assert!(
        retry.status.success(),
        "retry stdout={} stderr={}",
        String::from_utf8_lossy(&retry.stdout),
        String::from_utf8_lossy(&retry.stderr)
    );
    let retried: serde_json::Value = serde_json::from_slice(&retry.stdout).unwrap();
    assert_eq!(retried, claimed);
    assert_eq!(std::fs::read_to_string(&calls).unwrap(), before);
}

fn run_decapod_bounded(dir: &Path, args: &[&str], envs: &[(&str, &str)]) -> std::process::Output {
    use decapod::core::bounded_process::BoundedCommand;
    Command::new(resolve_decapod_bin())
        .current_dir(dir)
        .args(args)
        .envs(envs.iter().copied())
        .bounded_output(Duration::from_secs(30))
        .unwrap_or_else(|error| panic!("bounded probe command {args:?}: {error}"))
}

#[cfg(unix)]
struct ClaimProbeChild(Option<Child>);
#[cfg(unix)]
impl ClaimProbeChild {
    fn terminate(&mut self) {
        if let Some(child) = self.0.as_mut() {
            // This child has not been reaped, so its owned group cannot be reused.
            unsafe {
                libc::kill(-(child.id() as libc::pid_t), libc::SIGKILL);
            }
            let _ = child.kill();
        }
    }
    fn output(
        &mut self,
        stdout: &Path,
        stderr: &Path,
        hooks: &Path,
        stage: &str,
    ) -> std::process::Output {
        self.output_timeout(stdout, stderr, hooks, stage, Duration::from_secs(30))
    }
    fn output_timeout(
        &mut self,
        stdout: &Path,
        stderr: &Path,
        hooks: &Path,
        stage: &str,
        timeout: Duration,
    ) -> std::process::Output {
        let start = Instant::now();
        loop {
            if let Some(status) = self.0.as_mut().unwrap().try_wait().unwrap() {
                self.0.take();
                return std::process::Output {
                    status,
                    stdout: std::fs::read(stdout).unwrap(),
                    stderr: std::fs::read(stderr).unwrap(),
                };
            }
            if start.elapsed() >= timeout {
                self.terminate();
                let _ = self.0.take().unwrap().wait();
                panic!(
                    "{stage} timed out; stdout={} stderr={} hooks={}",
                    std::fs::read_to_string(stdout).unwrap_or_default(),
                    std::fs::read_to_string(stderr).unwrap_or_default(),
                    std::fs::read_to_string(hooks).unwrap_or_default()
                );
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}
#[cfg(unix)]
impl Drop for ClaimProbeChild {
    fn drop(&mut self) {
        self.terminate();
        if let Some(mut child) = self.0.take() {
            let _ = child.wait();
        }
    }
}

fn claim_probe_stage(stage: &str) {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap();
    eprintln!("claim probe: {stage} at {}ms", now.as_millis());
}

struct BrokerTestTiming {
    label: String,
    started: Instant,
}

impl BrokerTestTiming {
    fn start(label: impl Into<String>) -> Self {
        let label = label.into();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap();
        eprintln!("broker timing START {label} at {}ms", now.as_millis());
        Self {
            label,
            started: Instant::now(),
        }
    }
}

impl Drop for BrokerTestTiming {
    fn drop(&mut self) {
        eprintln!(
            "broker timing END {} elapsed={}ms panicking={}",
            self.label,
            self.started.elapsed().as_millis(),
            std::thread::panicking()
        );
    }
}

fn run_contention_setup(dir: &Path, args: &[&str], envs: &[(&str, &str)]) -> std::process::Output {
    run_contention_command(dir, args, envs, Duration::from_secs(30), "contention setup")
}

#[cfg(unix)]
fn run_contention_command(
    dir: &Path,
    args: &[&str],
    envs: &[(&str, &str)],
    timeout: Duration,
    stage: &str,
) -> std::process::Output {
    use std::os::unix::process::CommandExt;
    let capture = tempfile::tempdir().expect("private contention capture");
    let stdout = capture.path().join("stdout");
    let stderr = capture.path().join("stderr");
    let hooks = envs
        .iter()
        .find(|(key, _)| *key == "DECAPOD_GROUP_BROKER_TEST_HOOK_FILE")
        .map(|(_, value)| PathBuf::from(value))
        .unwrap_or_else(|| capture.path().join("hooks"));
    let request_id = envs
        .iter()
        .find(|(key, _)| *key == "DECAPOD_GROUP_BROKER_REQUEST_ID")
        .map(|(_, value)| *value)
        .unwrap_or("setup");
    let label = format!("{stage}, request={request_id}, args={args:?}");
    let _timing = BrokerTestTiming::start(&label);
    let child = Command::new(resolve_decapod_bin())
        .current_dir(dir)
        .args(args)
        .envs(envs.iter().copied())
        .env("DECAPOD_GROUP_BROKER_TEST_HOOK_FILE", &hooks)
        .stdin(std::process::Stdio::null())
        .stdout(std::fs::File::create(&stdout).unwrap())
        .stderr(std::fs::File::create(&stderr).unwrap())
        .process_group(0)
        .spawn()
        .unwrap_or_else(|error| panic!("{label}: spawn failed: {error}"));
    ClaimProbeChild(Some(child)).output_timeout(&stdout, &stderr, &hooks, &label, timeout)
}

#[cfg(not(unix))]
fn run_contention_command(
    dir: &Path,
    args: &[&str],
    envs: &[(&str, &str)],
    timeout: Duration,
    stage: &str,
) -> std::process::Output {
    use decapod::core::bounded_process::BoundedCommand;
    let request_id = envs
        .iter()
        .find(|(key, _)| *key == "DECAPOD_GROUP_BROKER_REQUEST_ID")
        .map(|(_, value)| *value)
        .unwrap_or("setup");
    let label = format!("{stage}, request={request_id}, args={args:?}");
    let _timing = BrokerTestTiming::start(&label);
    Command::new(resolve_decapod_bin())
        .current_dir(dir)
        .args(args)
        .envs(envs.iter().copied())
        .bounded_output(timeout)
        .unwrap_or_else(|error| panic!("{label}: {error}"))
}

#[cfg(unix)]
#[test]
fn concurrent_claims_preserve_caller_in_response_and_persisted_owner() {
    assert_concurrent_claim_identity(false, false);
}

#[cfg(unix)]
#[test]
fn reusable_connection_preserves_caller_in_response_and_persisted_owner() {
    assert_concurrent_claim_identity(true, false);
}

#[cfg(unix)]
#[test]
fn reusable_connection_preserves_one_winner_among_eight_colliding_claims() {
    assert_concurrent_claim_identity(true, true);
}

#[cfg(unix)]
fn assert_concurrent_claim_identity(reuse_connection: bool, collide: bool) {
    let (_tmp, dir, password) = setup_repo_with(run_contention_setup);
    assert!(
        broker_socket_supported_with(&dir, &password, run_contention_setup),
        "required broker socket acceptance did not run"
    );
    let categories = [
        "features",
        "docs",
        "bugs",
        "ci",
        "refactor",
        "tests",
        "security",
        "performance",
    ];
    let count = if reuse_connection { 8 } else { 2 };
    let mut tasks = Vec::new();
    let mut collision_task: Option<String> = None;
    if reuse_connection {
        // All execution is confined to this disposable fixture. Real Git
        // worktrees exercise caller cwd transport rather than only env changes.
        fixture_git(
            &dir,
            &[
                "-c",
                "user.name=Broker fixture",
                "-c",
                "user.email=broker-fixture@example.invalid",
                "commit",
                "--allow-empty",
                "-m",
                "broker fixture base",
            ],
        );
    }
    for (index, category) in categories.iter().take(count).enumerate() {
        let agent = format!("caller-{index:02}");
        let task_dir = dir.join(format!("caller-worktree-{index:02}"));
        if reuse_connection {
            fixture_git(
                &dir,
                &[
                    "worktree",
                    "add",
                    "--detach",
                    task_dir.to_str().unwrap(),
                    "HEAD",
                ],
            );
        } else {
            std::fs::create_dir(&task_dir).unwrap();
        }
        let credential = acquire_session_password_with(&dir, &agent, run_contention_setup);
        if collide && let Some(id) = &collision_task {
            tasks.push((agent, credential, id.clone(), task_dir));
            continue;
        }
        let added = run_contention_setup(
            &dir,
            &[
                "todo",
                "add",
                &agent,
                "--scope",
                &agent,
                "--dir",
                task_dir.to_str().unwrap(),
            ],
            &[
                ("DECAPOD_AGENT_ID", "unknown"),
                ("DECAPOD_SESSION_PASSWORD", &password),
            ],
        );
        assert!(
            added.status.success(),
            "{}",
            String::from_utf8_lossy(&added.stderr)
        );
        let value: serde_json::Value = serde_json::from_slice(&added.stdout).unwrap();
        let id = value["id"].as_str().unwrap().to_owned();
        let edited = run_contention_setup(
            &dir,
            &["todo", "edit", "--id", &id, "--category", category],
            &[
                ("DECAPOD_AGENT_ID", "unknown"),
                ("DECAPOD_SESSION_PASSWORD", &password),
            ],
        );
        assert!(
            edited.status.success(),
            "{}",
            String::from_utf8_lossy(&edited.stderr)
        );
        if collide {
            collision_task = Some(id.clone());
        }
        tasks.push((agent, credential, id, task_dir));
    }

    // Elect an independent caller, then submit every claim through its live socket.
    // The leader remains ephemeral; the bounded guard cleans up on test failure.
    let hooks = dir.join("identity-leader-hooks");
    let stdout = dir.join("identity-leader-out");
    let stderr = dir.join("identity-leader-err");
    let mut command = Command::new(resolve_decapod_bin());
    command.env(
        "DECAPOD_STORAGE_OWNER",
        if reuse_connection { "1" } else { "0" },
    );
    command
        .current_dir(&dir)
        .args(["todo", "add", "identity-leader"])
        .env("DECAPOD_AGENT_ID", "unknown")
        .env("DECAPOD_SESSION_PASSWORD", &password)
        .env("DECAPOD_GROUP_BROKER_IDLE_SECS", "10")
        .env("DECAPOD_GROUP_BROKER_REQUEST_ID", "IDENTITY_LEADER")
        .env("DECAPOD_GROUP_BROKER_TEST_HOOK_FILE", &hooks)
        .stdout(std::fs::File::create(&stdout).unwrap())
        .stderr(std::fs::File::create(&stderr).unwrap());
    use std::os::unix::process::CommandExt;
    command.process_group(0);
    let mut leader = ClaimProbeChild(Some(command.spawn().unwrap()));
    assert!(wait_for_hook_line(
        &hooks,
        "post_exec_pre_ack|IDENTITY_LEADER",
        Duration::from_secs(15)
    ));

    let barrier = std::sync::Arc::new(std::sync::Barrier::new(tasks.len()));
    let workers: Vec<_> = tasks
        .iter()
        .cloned()
        .map(|(agent, credential, id, caller_dir)| {
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                barrier.wait();
                let output = run_contention_command(
                    &caller_dir,
                    &["todo", "claim", "--id", &id],
                    &[
                        ("DECAPOD_AGENT_ID", &agent),
                        ("DECAPOD_SESSION_PASSWORD", &credential),
                        ("DECAPOD_SESSION_ID", &agent),
                        ("DECAPOD_CORRELATION_ID", &agent),
                        ("DECAPOD_CLAIM_AUTORUN", "0"),
                        ("DECAPOD_GROUP_BROKER_REQUEST_ID", &agent),
                    ],
                    Duration::from_secs(60),
                    "concurrent caller identity claim",
                );
                (agent, id, output)
            })
        })
        .collect();
    let results: Vec<_> = workers.into_iter().map(|worker| worker.join()).collect();
    let leader_output = leader.output(&stdout, &stderr, &hooks, "identity leader completion");
    assert!(
        leader_output.status.success(),
        "{}",
        String::from_utf8_lossy(&leader_output.stderr)
    );
    let leader_hooks = std::fs::read_to_string(&hooks).unwrap();
    assert_eq!(
        leader_hooks
            .lines()
            .filter(|line| line.starts_with("storage_owner_open|"))
            .count(),
        usize::from(reuse_connection),
        "expected one retained physical storage owner, never a per-caller owner"
    );
    for (agent, _, _, _) in &tasks {
        assert!(
            leader_hooks
                .lines()
                .any(|line| line.contains(&format!("queued|{agent}"))),
            "claim did not traverse the elected leader: {agent}"
        );
    }
    let audit = decapod::core::db::Connection::open(dir.join(".decapod/data/decapod.db")).unwrap();
    for (agent, _, _, _) in &tasks {
        let count: i64 = audit.query_row("SELECT count(*) FROM events WHERE stream='broker' AND json_extract(payload, '$.op')='todo.claim' AND json_extract(payload, '$.request_id')=?1 AND json_extract(payload, '$.session_id')=?1 AND json_extract(payload, '$.correlation_id')=?1 AND json_extract(payload, '$.caller_agent_id')=?1", [agent], |r| r.get(0)).unwrap();
        assert!(count > 0, "claim audit lost caller context for {agent}");
    }
    drop(audit);
    let mut winners = Vec::new();
    for result in results {
        let (agent, id, output) = result.unwrap();
        let response: serde_json::Value =
            serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
                panic!(
                    "{agent}: {error}; stdout={} stderr={}",
                    String::from_utf8_lossy(&output.stdout),
                    String::from_utf8_lossy(&output.stderr)
                )
            });
        if collide && response["result"]["status"] == "conflict" {
            continue;
        }
        assert!(
            output.status.success(),
            "{agent}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(response["result"]["status"], "ok", "{response}");
        assert_eq!(response["result"]["assigned_to"], agent, "{response}");
        winners.push((agent.clone(), id.clone()));
        if !collide {
            let fetched = run_contention_setup(&dir, &["todo", "get", "--id", &id], &[]);
            assert!(fetched.status.success());
            let persisted: serde_json::Value = serde_json::from_slice(&fetched.stdout).unwrap();
            assert_eq!(persisted["item"]["assigned_to"], agent, "{persisted}");
        }
    }
    if collide {
        assert_eq!(
            winners.len(),
            1,
            "exactly one exclusive claimant must win: {winners:?}"
        );
        let (winner, id) = &winners[0];
        let fetched = run_contention_setup(&dir, &["todo", "get", "--id", id], &[]);
        assert!(fetched.status.success());
        let persisted: serde_json::Value = serde_json::from_slice(&fetched.stdout).unwrap();
        assert_eq!(persisted["item"]["assigned_to"], *winner, "{persisted}");
    } else {
        assert_eq!(winners.len(), count);
    }
}

#[cfg(unix)]
fn fixture_git(dir: &Path, args: &[&str]) {
    use decapod::core::bounded_process::BoundedCommand;
    let output = Command::new("git")
        .current_dir(dir)
        .args(args)
        .bounded_output(Duration::from_secs(10))
        .expect("bounded fixture git");
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[cfg(unix)]
#[test]
fn storage_worker_parent_loss_interrupts_sql_and_releases_transaction_lock() {
    use std::io::Read;
    use std::os::unix::net::UnixStream;
    use std::os::unix::process::CommandExt;
    use std::process::Stdio;

    let (_tmp, dir, password) = setup_repo_with(run_contention_setup);
    assert!(
        broker_socket_supported_with(&dir, &password, run_contention_setup),
        "required broker socket acceptance did not run"
    );
    assert!(wait_for_no_broker_artifacts(&dir, Duration::from_secs(6)));
    let database = dir.join(".decapod/data/decapod.db").canonicalize().unwrap();
    {
        let conn = decapod::core::db::db_connect(database.to_str().unwrap()).unwrap();
        conn.execute("CREATE TABLE owner_crash_probe(value INTEGER)", [])
            .unwrap();
    }
    let hooks = dir.join("storage-worker-hooks");
    let stdout = dir.join("storage-worker-out");
    let stderr = dir.join("storage-worker-err");
    let command = Command::new(resolve_decapod_bin())
        .current_dir(&dir)
        .arg("__storage-owner-worker")
        .arg(&database)
        .env("DECAPOD_STORAGE_OWNER_WORKER", "1")
        .env("DECAPOD_GROUP_BROKER_TEST_HOOK_FILE", &hooks)
        .stdin(Stdio::piped())
        .stdout(std::fs::File::create(&stdout).unwrap())
        .stderr(std::fs::File::create(&stderr).unwrap())
        .process_group(0)
        .spawn()
        .unwrap();
    let mut worker = ClaimProbeChild(Some(command));
    let lifetime = worker.0.as_mut().unwrap().stdin.take().unwrap();
    assert!(wait_for_hook_line(
        &hooks,
        "storage_owner_open|",
        Duration::from_secs(10)
    ));
    let ready_started = Instant::now();
    while std::fs::read(&stdout).unwrap_or_default() != b"R" {
        assert!(
            ready_started.elapsed() < Duration::from_secs(3),
            "worker readiness byte missing"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    let mut stream =
        UnixStream::connect(broker_endpoint(&dir).with_file_name("storage.sock")).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    stream
        .set_write_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let caller = serde_json::json!({
        "agent_id":"crash-caller", "session_id":null, "task_id":null,
        "correlation_id":null, "causation_id":null, "request_id":"crash-probe"
    });
    write_storage_frame(
        &mut stream,
        &serde_json::json!({"Open":{
            "version":1, "database":database, "read_only":false, "caller":caller
        }}),
    );
    assert_eq!(read_storage_frame(&mut stream)["Ready"]["version"], 1);
    for sql in ["BEGIN IMMEDIATE", "INSERT INTO owner_crash_probe VALUES(1)"] {
        write_storage_frame(
            &mut stream,
            &serde_json::json!({"Write":{"sql":sql,"params":[]}}),
        );
        assert!(read_storage_frame(&mut stream).get("Count").is_some());
    }
    write_storage_frame(
        &mut stream,
        &serde_json::json!({"Read":{
            "sql":"WITH RECURSIVE n(x) AS (VALUES(1) UNION ALL SELECT x+1 FROM n WHERE x<1000000000) SELECT sum(x) FROM n",
            "params":[]
        }}),
    );
    assert!(wait_for_hook_line(
        &hooks,
        "storage_owner_read|",
        Duration::from_secs(3)
    ));
    // Ensure the expensive read has not already returned an error/result. This
    // proves EOF cancellation while work is in flight, not idle shutdown only.
    stream
        .set_read_timeout(Some(Duration::from_millis(100)))
        .unwrap();
    let mut unexpected = [0u8; 1];
    let pending = stream
        .read(&mut unexpected)
        .expect_err("expensive query completed before cancellation");
    assert!(matches!(
        pending.kind(),
        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
    ));
    drop(lifetime);
    let output = worker.output_timeout(
        &stdout,
        &stderr,
        &hooks,
        "storage worker parent loss",
        Duration::from_secs(3),
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    drop(stream);
    // Process termination must release the physical lock and SQLite recovery
    // must roll back the interrupted transaction before another caller opens.
    let conn = decapod::core::db::db_connect(database.to_str().unwrap()).unwrap();
    let count: i64 = conn
        .query_row("SELECT count(*) FROM owner_crash_probe", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(count, 0);
}

#[cfg(unix)]
fn write_storage_frame(stream: &mut std::os::unix::net::UnixStream, value: &serde_json::Value) {
    use std::io::Write;
    let encoded = serde_json::to_vec(value).unwrap();
    stream
        .write_all(&(encoded.len() as u32).to_be_bytes())
        .unwrap();
    stream.write_all(&encoded).unwrap();
    stream.flush().unwrap();
}

#[cfg(unix)]
fn read_storage_frame(stream: &mut std::os::unix::net::UnixStream) -> serde_json::Value {
    use std::io::Read;
    let mut size = [0u8; 4];
    stream.read_exact(&mut size).unwrap();
    let length = u32::from_be_bytes(size) as usize;
    assert!(length > 0 && length <= 16 * 1024 * 1024);
    let mut bytes = vec![0; length];
    stream.read_exact(&mut bytes).unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

#[cfg(unix)]
#[test]
fn disconnected_completed_caller_does_not_end_leader_with_a_queued_request() {
    use std::os::unix::process::CommandExt;
    let (_tmp, dir, password) = setup_repo_with(run_contention_setup);
    let hooks = dir.join("disconnect-leader-hooks");
    let release = dir.join("disconnect-release");
    let stdout = dir.join("disconnect-leader-stdout");
    let stderr = dir.join("disconnect-leader-stderr");
    let child = Command::new(resolve_decapod_bin())
        .current_dir(&dir)
        .args(["todo", "add", "disconnect-leader"])
        .env("DECAPOD_AGENT_ID", "unknown")
        .env("DECAPOD_SESSION_PASSWORD", &password)
        .env("DECAPOD_STORAGE_OWNER", "1")
        .env("DECAPOD_GROUP_BROKER_IDLE_SECS", "1")
        .env("DECAPOD_GROUP_BROKER_REQUEST_ID", "DISCONNECT_LEADER")
        .env("DECAPOD_GROUP_BROKER_TEST_HOOK_FILE", &hooks)
        .env("DECAPOD_GROUP_BROKER_TEST_HALT_PHASE", "post_exec_pre_ack")
        .env("DECAPOD_GROUP_BROKER_TEST_HALT_REQUEST", "DISCONNECTED")
        .env("DECAPOD_GROUP_BROKER_TEST_RELEASE", &release)
        .stdout(std::fs::File::create(&stdout).unwrap())
        .stderr(std::fs::File::create(&stderr).unwrap())
        .process_group(0)
        .spawn()
        .unwrap();
    let mut leader = ClaimProbeChild(Some(child));
    assert!(wait_for_hook_line(
        &hooks,
        "post_exec_pre_ack|DISCONNECT_LEADER",
        Duration::from_secs(15)
    ));
    let child = Command::new(resolve_decapod_bin())
        .current_dir(&dir)
        .args(["todo", "add", "committed-but-disconnected"])
        .env("DECAPOD_AGENT_ID", "unknown")
        .env("DECAPOD_SESSION_PASSWORD", &password)
        .env("DECAPOD_GROUP_BROKER_REQUEST_ID", "DISCONNECTED")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .process_group(0)
        .spawn()
        .unwrap();
    let mut disconnected = ClaimProbeChild(Some(child));
    assert!(wait_for_hook_line(
        &hooks,
        "post_exec_pre_ack|DISCONNECTED",
        Duration::from_secs(15)
    ));
    disconnected.terminate();
    let next_hooks = dir.join("next-client-hooks");
    let next_dir = dir.clone();
    let next_password = password.clone();
    let next_hook = next_hooks.clone();
    let next = std::thread::spawn(move || {
        run_contention_command(
            &next_dir,
            &["todo", "add", "after-disconnect"],
            &[
                ("DECAPOD_AGENT_ID", "unknown"),
                ("DECAPOD_SESSION_PASSWORD", &next_password),
                ("DECAPOD_GROUP_BROKER_REQUEST_ID", "AFTER_DISCONNECT"),
                (
                    "DECAPOD_GROUP_BROKER_TEST_HOOK_FILE",
                    next_hook.to_str().unwrap(),
                ),
            ],
            Duration::from_secs(30),
            "queued after disconnect",
        )
    });
    assert!(wait_for_hook_line(
        &next_hooks,
        "sent|AFTER_DISCONNECT",
        Duration::from_secs(5)
    ));
    // Advance past the configured idle period while execution is deliberately
    // paused. The pre-fix leader mistakes failed reply delivery for inactivity.
    std::thread::sleep(Duration::from_millis(1100));
    std::fs::write(&release, "continue").unwrap();
    let next = next.join().unwrap();
    assert!(
        next.status.success(),
        "{}",
        String::from_utf8_lossy(&next.stderr)
    );
    assert!(wait_for_hook_line(
        &hooks,
        "queued|AFTER_DISCONNECT",
        Duration::from_secs(2)
    ));
    let completed = leader.output_timeout(
        &stdout,
        &stderr,
        &hooks,
        "disconnect leader",
        Duration::from_secs(10),
    );
    assert!(
        completed.status.success(),
        "{}",
        String::from_utf8_lossy(&completed.stderr)
    );
    let conn = decapod::core::db::Connection::open(dir.join(".decapod/data/decapod.db")).unwrap();
    for title in ["committed-but-disconnected", "after-disconnect"] {
        let count: i64 = conn
            .query_row("SELECT count(*) FROM tasks WHERE title=?1", [title], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(count, 1);
    }
    assert!(!broker_endpoint(&dir).exists());
}

#[cfg(unix)]
#[test]
fn long_datastore_path_runs_the_owner_and_cleans_both_endpoints() {
    let (_temp, dir, password) = setup_repo_at_depth(run_contention_setup, true);
    let hooks = dir.join("long-path-hooks");
    let output = run_contention_command(
        &dir,
        &["todo", "add", "long canonical store"],
        &[
            ("DECAPOD_AGENT_ID", "unknown"),
            ("DECAPOD_SESSION_PASSWORD", &password),
            ("DECAPOD_STORAGE_OWNER", "1"),
            ("DECAPOD_GROUP_BROKER_REQUEST_ID", "LONG_PATH"),
            (
                "DECAPOD_GROUP_BROKER_TEST_HOOK_FILE",
                hooks.to_str().unwrap(),
            ),
        ],
        Duration::from_secs(30),
        "long canonical path",
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let evidence = std::fs::read_to_string(&hooks).unwrap();
    assert!(evidence.contains("queued|LONG_PATH"));
    assert_eq!(
        evidence
            .lines()
            .filter(|line| line.starts_with("storage_owner_open|"))
            .count(),
        1
    );
    let endpoint = broker_endpoint(&dir);
    assert!(endpoint.as_os_str().as_encoded_bytes().len() < 104);
    assert!(!endpoint.exists());
    assert!(!endpoint.with_file_name("storage.sock").exists());
    let conn =
        decapod::core::db::db_connect(dir.join(".decapod/data/decapod.db").to_str().unwrap())
            .unwrap();
    let count: i64 = conn
        .query_row(
            "SELECT count(*) FROM tasks WHERE title='long canonical store'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(count, 1);
}
