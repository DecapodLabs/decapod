// Moved from src/decapod/core/group_broker.rs
#[test]
fn broker_diagnostic_scope_is_request_local() {
    use crate::core::bounded_process::BoundedCommand;
    const PROBE: &str = "DECAPOD_TEST_BROKER_DIAGNOSTIC_SCOPE";
    let names = [
        "DECAPOD_VALIDATE_SKIP_GIT_GATES",
        "DECAPOD_VALIDATE_SKIP_TOOLING_GATES",
        "DECAPOD_VALIDATE_SKIP_FINGERPRINT_GATES",
    ];
    let mode = std::env::var(PROBE).unwrap_or_default();
    if mode == "observe_scope" {
        assert_eq!(std::env::var(names[0]).unwrap(), "0");
        assert!(std::env::var_os(names[1]).is_none());
        assert_eq!(std::env::var(names[2]).unwrap(), "false");
        let error =
            crate::core::validation_runtime::require_full_readiness_environment().unwrap_err();
        assert!(
            error
                .to_string()
                .contains("READINESS_GATE_OVERRIDE_UNSUPPORTED")
        );
        return;
    }
    if mode == "observe_clear" {
        for name in names {
            assert!(std::env::var_os(name).is_none(), "{name}");
        }
        crate::core::validation_runtime::require_full_readiness_environment().unwrap();
        return;
    }
    let case = format!(
        "{}::broker_diagnostic_scope_is_request_local",
        module_path!().split_once("::").unwrap().1
    );
    let command = |mode| {
        let mut command = std::process::Command::new(std::env::current_exe().unwrap());
        command
            .args(["--exact", &case, "--nocapture"])
            .env(PROBE, mode);
        command
    };
    if mode == "capture_scope" || mode == "capture_clear" {
        let caller = super::CallerContext::capture().unwrap();
        let mut child = command(if mode == "capture_scope" {
            "observe_scope"
        } else {
            "observe_clear"
        });
        for name in names {
            child.env(name, "previous-worker-scope");
        }
        caller.apply(&mut child);
        let output = child
            .bounded_output(std::time::Duration::from_secs(30))
            .unwrap();
        assert!(
            output.status.success(),
            "{} {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(String::from_utf8_lossy(&output.stdout).contains("1 passed"));
        return;
    }
    for mode in ["capture_scope", "capture_clear"] {
        let mut child = command(mode);
        for name in names {
            child.env_remove(name);
        }
        child
            .env("DECAPOD_AGENT_ID", "broker-scope-agent")
            .env_remove("DECAPOD_SESSION_PASSWORD");
        if mode == "capture_scope" {
            child.env(names[0], "0").env(names[2], "false");
        }
        let output = child
            .bounded_output(std::time::Duration::from_secs(60))
            .unwrap();
        assert!(
            output.status.success(),
            "{} {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(String::from_utf8_lossy(&output.stdout).contains("1 passed"));
    }
}

#[test]
fn stale_pid_marker_does_not_block_or_signal_unrelated_process() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("broker.lock");
    // Even a reused/live PID is only a diagnostic. Lock ownership is kernel based.
    std::fs::write(&path, format!("{}\n", std::process::id())).unwrap();
    let lease = super::try_acquire_lock(&path).unwrap().unwrap();
    assert!(super::try_acquire_lock(&path).unwrap().is_none());
    drop(lease);
    assert!(!path.exists());
    assert!(super::try_acquire_lock(&path).unwrap().is_some());
}

#[test]
fn invalid_or_dead_pid_marker_is_recovered_idempotently() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("broker.lock");
    for marker in ["", "not-a-pid", "4294967295\n"] {
        std::fs::write(&path, marker).unwrap();
        drop(super::try_acquire_lock(&path).unwrap().unwrap());
        assert!(!path.exists());
    }
}

#[test]
fn concurrent_election_has_one_winner() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("broker.lock");
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(8));
    let workers: Vec<_> = (0..8)
        .map(|_| {
            let path = path.clone();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                barrier.wait();
                let lease = super::try_acquire_lock(&path).unwrap();
                let won = lease.is_some();
                barrier.wait();
                drop(lease);
                won
            })
        })
        .collect();
    let winners = workers
        .into_iter()
        .map(|worker| worker.join().unwrap() as usize)
        .sum::<usize>();
    assert_eq!(winners, 1);
}

#[test]
fn broker_lock_child_helper() {
    let Some(path) = std::env::var_os("DECAPOD_TEST_BROKER_LOCK") else {
        return;
    };
    let path = std::path::PathBuf::from(path);
    let _lease = super::try_acquire_lock(&path).unwrap().unwrap();
    std::fs::write(path.with_extension("ready"), "ready").unwrap();
    std::thread::sleep(std::time::Duration::from_secs(30));
}

#[test]
fn crashed_leader_releases_election_for_next_invocation() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("broker.lock");
    let mut child = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "core::group_broker::tests::broker_lock_child_helper",
        ])
        .env("DECAPOD_TEST_BROKER_LOCK", &path)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
    let start = std::time::Instant::now();
    while !path.with_extension("ready").exists()
        && start.elapsed() < std::time::Duration::from_secs(5)
    {
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let ready = path.with_extension("ready").exists();
    child.kill().unwrap();
    child.wait().unwrap();
    assert!(ready, "helper did not acquire lock before test deadline");
    let lease = super::try_acquire_lock(&path).unwrap().unwrap();
    drop(lease);
    assert!(!path.exists());
}

#[test]
fn socket_directory_is_confined_even_under_permissive_umask() {
    use std::os::unix::fs::PermissionsExt;
    if std::env::var_os("DECAPOD_BROKER_PERMISSION_CHILD").is_none() {
        for shared in ["0", "1"] {
            let output = std::process::Command::new("sh")
                .args(["-c", "umask 000; exec \"$@\"", "broker-permission-child"])
                .arg(std::env::current_exe().unwrap())
                .args(["--exact", "core::group_broker::tests::socket_directory_is_confined_even_under_permissive_umask", "--test-threads=1"])
                .env("DECAPOD_BROKER_PERMISSION_CHILD", "1")
                .env(crate::core::fs_permissions::SHARED_STORAGE_ENV, shared)
                .output().unwrap();
            assert!(
                output.status.success(),
                "{}\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }
        return;
    }
    let temp = tempfile::tempdir().unwrap();
    std::fs::set_permissions(temp.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    let store = temp.path().join("data");
    crate::core::fs_permissions::ensure_storage_dir(&store).unwrap();
    let socket = super::broker_socket_path(&store).unwrap();
    super::ensure_socket_directory(socket.parent().unwrap()).unwrap();
    let expected = if crate::core::fs_permissions::shared_storage() {
        0o770
    } else {
        0o700
    };
    assert_eq!(
        std::fs::metadata(socket.parent().unwrap())
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        expected
    );
    let lease = super::try_acquire_lock(&store.join("broker.lock"))
        .unwrap()
        .unwrap();
    assert!(
        super::try_acquire_lock(&store.join("broker.lock"))
            .unwrap()
            .is_none()
    );
    drop(lease);
    std::fs::set_permissions(
        socket.parent().unwrap(),
        std::fs::Permissions::from_mode(0o755),
    )
    .unwrap();
    assert!(
        super::ensure_socket_directory(socket.parent().unwrap())
            .unwrap_err()
            .to_string()
            .contains("BROKER_UNSAFE_SOCKET_DIRECTORY")
    );
    assert_eq!(
        std::fs::metadata(socket.parent().unwrap())
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o755
    );
}

fn caller_context(agent: &str) -> super::CallerContext {
    super::CallerContext {
        cwd: std::path::PathBuf::from("/workspace/repo"),
        environment: std::collections::BTreeMap::from([
            ("DECAPOD_AGENT_ID".into(), agent.into()),
            ("DECAPOD_SESSION_PASSWORD".into(), "ephemeral-secret".into()),
        ]),
    }
}

#[test]
fn broker_replay_binds_actor_workspace_and_execution_context() {
    let args = vec![
        "todo".into(),
        "claim".into(),
        "--id".into(),
        "task-a".into(),
    ];
    let caller = caller_context("agent-a");
    let original = super::hash_payload(&args, &caller);
    assert_ne!(
        original,
        super::hash_payload(&args, &caller_context("agent-b"))
    );
    let mut other = caller.clone();
    other.cwd = "/workspace/another-worktree".into();
    assert_ne!(original, super::hash_payload(&args, &other));
    other = caller.clone();
    other
        .environment
        .insert("DECAPOD_CLAIM_AUTORUN".into(), "0".into());
    assert_ne!(original, super::hash_payload(&args, &other));
    other = caller;
    other
        .environment
        .insert("DECAPOD_SESSION_PASSWORD".into(), "rotated-secret".into());
    assert_eq!(original, super::hash_payload(&args, &other));
}

#[test]
fn caller_context_cannot_forward_arbitrary_environment_or_broker_controls() {
    let mut caller = caller_context("agent-a");
    assert!(caller.validate().is_ok());
    for key in [
        "PATH",
        "DECAPOD_GROUP_BROKER_INTERNAL",
        "DECAPOD_VALIDATE_UNKNOWN_BYPASS",
    ] {
        caller.environment.insert(key.into(), "1".into());
        assert!(caller.validate().is_err(), "accepted unscoped key {key}");
        caller.environment.remove(key);
    }
    caller.cwd = "relative-path".into();
    assert!(caller.validate().is_err());
}

#[test]
fn caller_context_clears_absent_leader_credentials_and_metadata() {
    let mut caller = caller_context("agent-a");
    caller.environment.remove("DECAPOD_SESSION_PASSWORD");
    let mut command = std::process::Command::new("unused");
    for key in super::CALLER_ENV_KEYS {
        command.env(key, "leader-value");
    }
    caller.apply(&mut command);
    let overrides: std::collections::BTreeMap<_, _> = command.get_envs().collect();
    assert_eq!(command.get_current_dir(), Some(caller.cwd.as_path()));
    assert_eq!(
        overrides[std::ffi::OsStr::new("DECAPOD_AGENT_ID")],
        Some(std::ffi::OsStr::new("agent-a"))
    );
    for key in super::CALLER_ENV_KEYS {
        if *key != "DECAPOD_AGENT_ID" {
            assert_eq!(
                overrides[std::ffi::OsStr::new(key)],
                None,
                "inherited {key}"
            );
        }
    }
}

#[test]
fn lost_acknowledgement_is_unknown_and_does_not_elect_or_replay() {
    use std::io::BufRead;
    let dir = tempfile::tempdir().unwrap();
    let socket = super::broker_socket_path(dir.path()).unwrap();
    crate::core::local_socket::ensure_directory(&socket, dir.path()).unwrap();
    let listener = std::os::unix::net::UnixListener::bind(&socket).unwrap();
    let peer = std::thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        let mut line = String::new();
        std::io::BufReader::new(stream)
            .read_line(&mut line)
            .unwrap();
        assert!(line.contains("request_id"));
        // Peer drops after receiving the complete request. It could have committed.
    });
    let err = super::run_unix_broker(dir.path(), &["unused".into()], &mut |_, _| {
        panic!("unexpected execution")
    })
    .unwrap_err();
    peer.join().unwrap();
    assert!(err.to_string().contains("BROKER_UNKNOWN"));
    assert!(
        !dir.path().join("broker.election").exists(),
        "re-elected after uncertain send"
    );
}

#[test]
fn durable_started_receipt_prevents_reexecution_and_binds_caller() {
    let dir = tempfile::tempdir().unwrap();
    let caller = caller_context("caller-a");
    let argv = vec!["must-never-spawn".into()];
    let mut request = super::BrokerRequest {
        protocol_version: super::BROKER_PROTOCOL_DEFAULT,
        request_id: "uncertain-request".into(),
        payload_hash: super::hash_payload(&argv, &caller),
        argv,
        caller,
        deadline_ms: super::epoch_ms() + 30_000,
    };
    super::dedupe_store(
        dir.path(),
        &request,
        &super::BrokerResponse {
            protocol_version: super::BROKER_PROTOCOL_DEFAULT,
            status: "UNKNOWN".into(),
            commit_marker: None,
            retry_after_ms_hint: None,
            result_envelope: serde_json::json!({"error":"interrupted"}),
        },
    )
    .unwrap();
    let result = super::execute_request(dir.path(), &request, false).unwrap();
    assert_eq!(result.status, "UNKNOWN");
    request.caller = caller_context("caller-b");
    request.payload_hash = super::hash_payload(&request.argv, &request.caller);
    let mismatch = super::execute_request(dir.path(), &request, false).unwrap();
    assert_eq!(
        mismatch.result_envelope["error"],
        "BROKER_DEDUPE_PAYLOAD_MISMATCH"
    );
}

#[test]
fn queued_request_deadline_prevents_execution_without_a_receipt() {
    let dir = tempfile::tempdir().unwrap();
    let caller = caller_context("caller-a");
    let argv = vec!["must-never-spawn".into()];
    let request = super::BrokerRequest {
        protocol_version: super::BROKER_PROTOCOL_DEFAULT,
        request_id: "expired".into(),
        payload_hash: super::hash_payload(&argv, &caller),
        argv,
        caller,
        deadline_ms: 1,
    };
    let result = super::execute_request(dir.path(), &request, false).unwrap();
    assert_eq!(
        result.result_envelope["error"],
        "BROKER_DEADLINE_BEFORE_EXECUTION"
    );
    assert!(
        super::dedupe_lookup(dir.path(), &request)
            .unwrap()
            .is_none()
    );
}

#[test]
fn slow_unframed_client_has_a_bounded_read_deadline() {
    let (server, _slow_client) = std::os::unix::net::UnixStream::pair().unwrap();
    let start = std::time::Instant::now();
    assert!(super::read_request_line(&server).is_err());
    assert!(start.elapsed() < std::time::Duration::from_secs(3));
}

#[test]
fn trickling_reply_cannot_extend_the_absolute_deadline() {
    use std::io::Write;
    let (server, mut peer) = std::os::unix::net::UnixStream::pair().unwrap();
    let writer = std::thread::spawn(move || {
        for _ in 0..100 {
            if peer.write_all(b"x").is_err() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    });
    let start = std::time::Instant::now();
    assert!(
        super::read_line_until(&server, start + std::time::Duration::from_millis(100), 1024,)
            .is_err()
    );
    assert!(start.elapsed() < std::time::Duration::from_millis(500));
    drop(server);
    writer.join().unwrap();
}

#[test]
fn oversized_native_deadline_is_rejected_before_durable_admission() {
    let dir = tempfile::tempdir().unwrap();
    let caller = caller_context("caller-a");
    let argv = vec!["must-never-spawn".into()];
    let request = super::BrokerRequest {
        protocol_version: super::BROKER_PROTOCOL_DEFAULT,
        request_id: "oversized-deadline".into(),
        payload_hash: super::hash_payload(&argv, &caller),
        argv,
        caller,
        deadline_ms: u64::MAX,
    };
    let result = super::execute_request(dir.path(), &request, false).unwrap();
    assert_eq!(result.status, "NOT_COMMITTED");
    assert_eq!(
        result.result_envelope["error"],
        "BROKER_REQUEST_BUDGET_INVALID"
    );
    assert!(result.commit_marker.is_none());
    assert!(!super::dedupe_db_path(dir.path()).exists());
}
