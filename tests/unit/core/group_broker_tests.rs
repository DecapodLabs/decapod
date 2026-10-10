// Moved from src/decapod/core/group_broker.rs
#[test]
fn broker_falls_back_for_socket_unavailable_errors() {
    assert!(super::broker_io_error_allows_direct_fallback(
        std::io::ErrorKind::PermissionDenied
    ));
    assert!(super::broker_io_error_allows_direct_fallback(
        std::io::ErrorKind::InvalidInput
    ));
    assert!(!super::broker_io_error_allows_direct_fallback(
        std::io::ErrorKind::Other
    ));
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
    let socket = super::broker_socket_path(&store);
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
