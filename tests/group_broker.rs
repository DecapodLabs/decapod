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
    let tmp = TempDir::new().expect("tmpdir");
    let dir = tmp.path().to_path_buf();

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
    let sock_path = dir
        .join(".decapod")
        .join("data")
        .join("broker-runtime")
        .join("broker.sock");
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
    let _timing =
        BrokerTestTiming::start("broker_no_sqlite_busy_surfaced_under_concurrent_mutators");
    let (_tmp, dir, password) = setup_repo_with(run_contention_setup);
    if !broker_socket_supported_with(&dir, &password, run_contention_setup) {
        eprintln!("skipping: unix socket transport not permitted in this sandbox");
        return;
    }

    let creds: Vec<(String, String)> = (0..20)
        .map(|i| {
            let agent_id = format!("agent-{i:02}");
            let _timing = BrokerTestTiming::start(format!("contention credential {i:02}"));
            let agent_pw = acquire_session_password_with(&dir, &agent_id, run_contention_setup);
            (agent_id, agent_pw)
        })
        .collect();

    let mut workers = Vec::new();
    for (i, (agent_id, agent_pw)) in creds.into_iter().enumerate() {
        let dir_cl = dir.clone();
        workers.push(std::thread::spawn(move || {
            let _timing = BrokerTestTiming::start(format!("contention client {i:02}"));
            let task = format!("concurrent-task-{i}");
            let req_id = format!("BROKER_BUSY_REQ_{i:02}");
            let envs = [
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
                ("DECAPOD_GROUP_BROKER_REQUEST_ID".to_string(), req_id),
            ];
            let env_pairs: Vec<(&str, &str)> =
                envs.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
            let first = run_contention_command(
                &dir_cl,
                &["todo", "add", &task],
                &env_pairs,
                Duration::from_secs(120),
                &format!("client {i:02} first attempt"),
            );
            if first.status.success() {
                return first;
            }
            let stderr = String::from_utf8_lossy(&first.stderr).to_string();
            let stdout = String::from_utf8_lossy(&first.stdout).to_string();
            if stderr.contains("BROKER_UNKNOWN") || stdout.contains("BROKER_UNKNOWN") {
                let _retry_timing = BrokerTestTiming::start(format!("contention retry {i:02}"));
                std::thread::sleep(Duration::from_millis(250));
                return run_contention_command(
                    &dir_cl,
                    &["todo", "add", &task],
                    &env_pairs,
                    Duration::from_secs(120),
                    &format!("client {i:02} same-ID retry"),
                );
            }
            first
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

    let lock_path = dir.join(".decapod").join("data").join("broker.lock");
    let sock_path = dir
        .join(".decapod")
        .join("data")
        .join("broker-runtime")
        .join("broker.sock");
    assert!(
        !lock_path.exists() && !sock_path.exists(),
        "ephemeral broker artifacts should be cleaned up"
    );
}

#[test]
fn broker_dedupe_returns_exactly_once_per_request_id() {
    let _timing = BrokerTestTiming::start("broker_dedupe_returns_exactly_once_per_request_id");
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
    let sock_path = dir
        .join(".decapod")
        .join("data")
        .join("broker-runtime")
        .join("broker.sock");
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
fn broker_crash_injection_phases_retry_to_exactly_once() {
    let _timing = BrokerTestTiming::start("broker_crash_injection_phases_retry_to_exactly_once");
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

        let hook_ok = wait_for_hook_line(&hook, &req_id, Duration::from_secs(8));
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
        assert!(
            retry.status.success(),
            "retry after crash must converge to committed: {}",
            String::from_utf8_lossy(&retry.stderr)
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
    assert_eq!(
        std::fs::metadata(data.join("broker-runtime"))
            .unwrap()
            .permissions()
            .mode()
            & 0o007,
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
